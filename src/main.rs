mod client;
mod distribution;
mod session;
mod update;

use clap::{Args, Parser, Subcommand};
use client::{AuthClient, ClientError, ExecutionRequest, ExecutionResponse, Identity};
use session::{SessionError, SessionStore};
use std::{
    fmt,
    fmt::Write as _,
    fs,
    io::{self, IsTerminal, Read},
    path::PathBuf,
    time::Duration,
};
use update::UpdateRequest;
use uuid::Uuid;

const DEFAULT_TIMEOUT_MS: u64 = 3_000;
const DEFAULT_MEMORY_MB: u64 = 128;
const DEFAULT_MAX_POLL_ATTEMPTS: usize = 240;
const MAX_CODE_BYTES: usize = 16 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(250);

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| "must be a positive integer".to_owned())?;
    (parsed > 0)
        .then_some(parsed)
        .ok_or_else(|| "must be a positive integer".to_owned())
}

#[derive(Debug, Parser)]
#[command(name = "mpaka", version, about = "mPaka CLI")]
struct Cli {
    #[arg(
        long,
        env = "MPAKA_API_URL",
        default_value = "https://api.mpaka.dev",
        global = true
    )]
    endpoint: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Register(AuthArgs),
    Login(AuthArgs),
    Whoami,
    Logout,
    Run(RunArgs),
    SelfUpdate(SelfUpdateArgs),
    #[command(subcommand)]
    Executions(ExecutionsCommand),
    #[cfg(windows)]
    #[command(name = "__mpaka-update-helper", hide = true)]
    UpdateHelper(UpdateHelperArgs),
}

#[derive(Debug, Subcommand)]
enum ExecutionsCommand {
    List(ExecutionOutputArgs),
    Get(ExecutionIdArgs),
    Cancel(ExecutionIdArgs),
}

#[derive(Debug, Args)]
struct ExecutionOutputArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ExecutionIdArgs {
    id: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct AuthArgs {
    #[arg(long)]
    email: String,
    #[arg(long, help = "read the password from stdin without echoing it")]
    password_stdin: bool,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[arg(long, conflicts_with = "stdin")]
    file: Option<PathBuf>,
    #[arg(long, conflicts_with = "file")]
    stdin: bool,
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS)]
    timeout_ms: u64,
    #[arg(long, default_value_t = DEFAULT_MEMORY_MB)]
    memory_mb: u64,
    #[arg(long, value_name = "KEY")]
    idempotency_key: Option<String>,
    #[arg(long, default_value_t = DEFAULT_MAX_POLL_ATTEMPTS, value_parser = parse_positive_usize)]
    max_poll_attempts: usize,
}

#[derive(Debug, Args)]
struct SelfUpdateArgs {
    #[arg(long, value_name = "VERSION")]
    version: Option<String>,
    #[arg(long)]
    check: bool,
}

#[cfg(windows)]
#[derive(Debug, Args)]
struct UpdateHelperArgs {
    parent_pid: u32,
    current: PathBuf,
    replacement: PathBuf,
}

#[derive(Debug)]
enum CliError {
    Client(ClientError),
    Session(SessionError),
    Update(update::UpdateError),
    Input(io::Error),
    InteractiveInputRequired,
    MissingExecutionApiKey,
    InvalidRunInput(&'static str),
    PollingTimeout { id: String, status: String },
    ExecutionFailed(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(error) => error.fmt(formatter),
            Self::Session(error) => error.fmt(formatter),
            Self::Update(error) => error.fmt(formatter),
            Self::Input(error) => write!(formatter, "could not read password: {error}"),
            Self::InteractiveInputRequired => {
                formatter.write_str("password input requires a terminal or --password-stdin")
            }
            Self::MissingExecutionApiKey => {
                formatter.write_str("mpaka run requires the MPAKA_API_KEY environment variable")
            }
            Self::InvalidRunInput(message) => formatter.write_str(message),
            Self::PollingTimeout { id, status } => write!(
                formatter,
                "execution polling limit reached for {id}; last known status: {status}"
            ),
            Self::ExecutionFailed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}

impl From<ClientError> for CliError {
    fn from(error: ClientError) -> Self {
        Self::Client(error)
    }
}

impl From<SessionError> for CliError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

impl From<update::UpdateError> for CliError {
    fn from(error: update::UpdateError) -> Self {
        Self::Update(error)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Input(error)
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli).await {
        eprintln!("mpaka: {error}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let store = SessionStore::from_environment()?;
    let client = AuthClient::new(&cli.endpoint)?;
    match cli.command {
        Command::Register(args) => authenticate(&client, &store, args, true).await?,
        Command::Login(args) => authenticate(&client, &store, args, false).await?,
        Command::Whoami => {
            let identity = client.me(&store.load()?).await?;
            print_identity(&identity, false);
        }
        Command::Logout => {
            client.logout(&store.load()?).await?;
            store.remove()?;
            println!("Successfully signed out.");
        }
        Command::Run(args) => run_execution(&client, args).await?,
        Command::SelfUpdate(args) => {
            let result = update::self_update(UpdateRequest {
                version: args.version,
                check_only: args.check,
                executable: None,
            })
            .await?;
            if result.applied {
                println!(
                    "Updated mpaka from {} to {}.",
                    result.current_version, result.candidate_version
                );
            } else if result.current_version == result.candidate_version {
                println!("mPaka is already up to date at {}.", result.current_version);
            } else {
                println!(
                    "Update available: {} (current {}).",
                    result.candidate_version, result.current_version
                );
            }
        }
        Command::Executions(command) => run_executions(&client, command).await?,
        #[cfg(windows)]
        Command::UpdateHelper(args) => {
            update::run_windows_helper(args.parent_pid, &args.current, &args.replacement)?;
        }
    }
    Ok(())
}

async fn run_execution(client: &AuthClient, args: RunArgs) -> Result<(), CliError> {
    let source = read_source(&args)?;
    let api_key = execution_api_key()?;
    let request = ExecutionRequest {
        language: "python".into(),
        code: source,
        timeout_ms: args.timeout_ms,
        memory_mb: args.memory_mb,
    };
    let (idempotency_key, generated) = resolve_idempotency_key(args.idempotency_key);
    if generated {
        eprintln!("{}", generated_idempotency_key_notice(&idempotency_key));
    }
    let execution = client
        .create_execution(&api_key, &request, &idempotency_key)
        .await?;
    let execution = poll_execution(client, &api_key, execution, args.max_poll_attempts).await?;
    print_execution(&execution);
    classify_execution_result(&execution)
}

fn resolve_idempotency_key(provided: Option<String>) -> (String, bool) {
    match provided {
        Some(key) => (key, false),
        None => (Uuid::new_v4().to_string(), true),
    }
}

fn generated_idempotency_key_notice(key: &str) -> String {
    format!("Generated idempotency key: {key}")
}

async fn run_executions(client: &AuthClient, command: ExecutionsCommand) -> Result<(), CliError> {
    let api_key = execution_api_key()?;
    match command {
        ExecutionsCommand::List(args) => {
            let executions = client.list_executions(&api_key).await?;
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&executions)
                        .map_err(|_| ClientError::InvalidResponse)?
                );
            } else {
                print!("{}", render_execution_list(&executions));
            }
        }
        ExecutionsCommand::Get(args) => {
            let execution = client.get_execution(&api_key, &args.id).await?;
            if args.json {
                println!("{}", render_execution_json(&execution)?);
            } else {
                print_execution(&execution);
            }
        }
        ExecutionsCommand::Cancel(args) => {
            let execution = client.cancel_execution(&api_key, &args.id).await?;
            if args.json {
                println!("{}", render_execution_json(&execution)?);
            } else {
                print_execution(&execution);
            }
        }
    }
    Ok(())
}

fn execution_api_key() -> Result<String, CliError> {
    execution_api_key_from(std::env::var("MPAKA_API_KEY").ok().as_deref())
}

fn execution_api_key_from(value: Option<&str>) -> Result<String, CliError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or(CliError::MissingExecutionApiKey)
}

async fn poll_execution(
    client: &AuthClient,
    api_key: &str,
    mut execution: ExecutionResponse,
    max_poll_attempts: usize,
) -> Result<ExecutionResponse, CliError> {
    for attempt in 0..max_poll_attempts {
        if is_terminal_status(&execution.status) {
            return Ok(execution);
        }
        if attempt + 1 == max_poll_attempts {
            return Err(CliError::PollingTimeout {
                id: execution.id,
                status: execution.status,
            });
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        execution = client.get_execution(api_key, &execution.id).await?;
    }
    unreachable!("positive max_poll_attempts is enforced by clap")
}

fn read_source(args: &RunArgs) -> Result<String, CliError> {
    let mut source = String::new();
    match (&args.file, args.stdin) {
        (Some(path), false) => fs::File::open(path)?.read_to_string(&mut source)?,
        (None, true) => io::stdin().read_to_string(&mut source)?,
        (None, false) => {
            return Err(CliError::InvalidRunInput(
                "run requires --file <PATH> or --stdin",
            ));
        }
        (Some(_), true) => {
            return Err(CliError::InvalidRunInput(
                "run accepts only one input source",
            ));
        }
    };
    if source.trim().is_empty() {
        return Err(CliError::InvalidRunInput("Python source cannot be empty"));
    }
    if source.len() > MAX_CODE_BYTES {
        return Err(CliError::InvalidRunInput("Python source exceeds 16 KiB"));
    }
    Ok(source)
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "timed_out" | "cancelled")
}

fn classify_execution_result(execution: &ExecutionResponse) -> Result<(), CliError> {
    if execution.status != "completed" {
        return Err(CliError::ExecutionFailed(execution.status.clone()));
    }
    if execution.mode == "mock_no_code_execution" {
        return Err(CliError::ExecutionFailed(
            "execution completed but submitted code was not executed".into(),
        ));
    }
    match execution.exit_code {
        Some(0) => Ok(()),
        Some(code) => Err(CliError::ExecutionFailed(format!(
            "program exited with code {code}"
        ))),
        None => Err(CliError::ExecutionFailed(
            "execution completed with no exit code".into(),
        )),
    }
}

fn print_execution(execution: &ExecutionResponse) {
    println!("Execution: {}", execution.id);
    println!("Status: {}", execution.status);
    println!("Mode: {}", execution.mode);
    println!(
        "Exit code: {}",
        execution
            .exit_code
            .map_or_else(|| "not available".into(), |code| code.to_string())
    );
    println!("stdout:");
    print!("{}", execution.stdout);
    if !execution.stdout.ends_with('\n') {
        println!();
    }
    println!("stderr:");
    print!("{}", execution.stderr);
    if !execution.stderr.is_empty() && !execution.stderr.ends_with('\n') {
        println!();
    }
}

fn render_execution_list(executions: &[ExecutionResponse]) -> String {
    let mut output = String::new();
    for execution in executions {
        let _ = writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            execution.id,
            execution.status,
            execution.language,
            execution.created_at,
            execution.finished_at.as_deref().unwrap_or("-")
        );
    }
    output
}

fn render_execution_json(execution: &ExecutionResponse) -> Result<String, CliError> {
    serde_json::to_string_pretty(execution).map_err(|_| ClientError::InvalidResponse.into())
}

async fn authenticate(
    client: &AuthClient,
    store: &SessionStore,
    args: AuthArgs,
    register: bool,
) -> Result<(), CliError> {
    let password = read_password(args.password_stdin)?;
    let (identity, session) = if register {
        client.register(&args.email, &password).await?
    } else {
        client.login(&args.email, &password).await?
    };
    store.save(session.token())?;
    print_identity(&identity, register);
    Ok(())
}

fn print_identity(identity: &Identity, registered: bool) {
    let verb = if registered {
        "registered"
    } else {
        "signed in"
    };
    println!("Successfully {verb} as {}.", identity.email);
    println!("Workspace: {}", identity.tenant_name);
    println!("Role: {}", identity.role);
}

fn read_password(from_stdin: bool) -> Result<String, CliError> {
    if from_stdin {
        let mut password = String::new();
        io::stdin().read_to_string(&mut password)?;
        return Ok(password.trim_end_matches(['\r', '\n']).to_owned());
    }
    if !io::stdin().is_terminal() {
        return Err(CliError::InteractiveInputRequired);
    }
    read_password_without_echo()
}

fn read_password_without_echo() -> Result<String, CliError> {
    rpassword::prompt_password("Password: ").map_err(CliError::Input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::get};
    use clap::CommandFactory;
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::net::TcpListener;
    #[test]
    fn exposes_register_and_login_without_a_password_argument() {
        assert!(Cli::try_parse_from(["mpaka", "register", "--email", "a@example.com"]).is_ok());
        assert!(Cli::try_parse_from(["mpaka", "login", "--email", "a@example.com"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "mpaka",
                "login",
                "--email",
                "a@example.com",
                "--password",
                "secret"
            ])
            .is_err()
        );
    }

    #[test]
    fn exposes_the_mpaka_binary_name_and_stdin_password_escape_hatch() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("mPaka CLI"));
        assert!(help.contains("Usage: mpaka"));
        assert!(help.contains("https://api.mpaka.dev"));
        assert!(help.contains("MPAKA_API_URL"));
        assert!(
            Cli::try_parse_from([
                "mpaka",
                "login",
                "--email",
                "a@example.com",
                "--password-stdin"
            ])
            .is_ok()
        );
    }

    #[test]
    fn exposes_whoami_and_logout_commands() {
        assert!(Cli::try_parse_from(["mpaka", "whoami"]).is_ok());
        assert!(Cli::try_parse_from(["mpaka", "logout"]).is_ok());
    }

    #[test]
    fn exposes_execution_list_get_and_cancel_commands_with_json_output() {
        assert!(Cli::try_parse_from(["mpaka", "executions", "list"]).is_ok());
        assert!(Cli::try_parse_from(["mpaka", "executions", "list", "--json"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "mpaka",
                "executions",
                "get",
                "00000000-0000-0000-0000-000000000001",
                "--json"
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from([
                "mpaka",
                "executions",
                "cancel",
                "00000000-0000-0000-0000-000000000001"
            ])
            .is_ok()
        );
    }

    #[test]
    fn renders_human_execution_list_rows() {
        let output = render_execution_list(&[execution_fixture("completed")]);
        assert!(output.contains("00000000-0000-0000-0000-000000000001"));
        assert!(output.contains("completed"));
        assert!(output.contains("python"));
        assert!(output.contains("2026-10-28T10:00:00Z"));
        assert!(output.contains("2026-10-28T10:00:01Z"));
    }

    #[test]
    fn renders_execution_json_from_typed_response() {
        let output = render_execution_json(&execution_fixture("failed")).expect("json");
        let value: serde_json::Value = serde_json::from_str(&output).expect("valid json");
        assert_eq!(value["status"], "failed");
        assert_eq!(value["stdout"], "partial output\n");
        assert_eq!(value["mode"], "mock_no_code_execution");
        assert!(value.get("source").is_none());
    }

    #[test]
    fn rejects_missing_execution_api_key_before_dispatch() {
        assert!(matches!(
            execution_api_key_from(None),
            Err(CliError::MissingExecutionApiKey)
        ));
    }

    #[test]
    fn terminal_lifecycle_statuses_remain_data_for_inspection_commands() {
        for status in ["failed", "timed_out", "cancelled"] {
            let execution = execution_fixture(status);
            assert!(is_terminal_status(&execution.status));
            assert!(render_execution_json(&execution).is_ok());
        }
    }

    fn execution_fixture(status: &str) -> ExecutionResponse {
        ExecutionResponse {
            id: "00000000-0000-0000-0000-000000000001".into(),
            status: status.into(),
            language: "python".into(),
            timeout_ms: 3000,
            memory_mb: 128,
            created_at: "2026-10-28T10:00:00Z".into(),
            finished_at: Some("2026-10-28T10:00:01Z".into()),
            stdout: "partial output\n".into(),
            stderr: "error output\n".into(),
            exit_code: Some(1),
            mode: "mock_no_code_execution".into(),
        }
    }

    #[test]
    fn exposes_file_and_stdin_execution_inputs() {
        assert!(Cli::try_parse_from(["mpaka", "run", "--file", "script.py"]).is_ok());
        assert!(Cli::try_parse_from(["mpaka", "run", "--stdin"]).is_ok());
    }

    #[test]
    fn exposes_idempotency_and_polling_options_for_run() {
        let cli = Cli::try_parse_from([
            "mpaka",
            "run",
            "--file",
            "script.py",
            "--idempotency-key",
            "retry-1",
            "--max-poll-attempts",
            "12",
        ])
        .expect("run options");
        let Command::Run(args) = cli.command else {
            panic!("expected run command");
        };
        assert_eq!(args.idempotency_key.as_deref(), Some("retry-1"));
        assert_eq!(args.max_poll_attempts, 12);
    }

    #[test]
    fn announces_generated_idempotency_keys_without_changing_explicit_keys() {
        let (generated, was_generated) = resolve_idempotency_key(None);
        assert!(was_generated);
        assert!(Uuid::parse_str(&generated).is_ok());
        assert_eq!(
            generated_idempotency_key_notice(&generated),
            format!("Generated idempotency key: {generated}")
        );

        let (explicit, was_generated) = resolve_idempotency_key(Some("retry-1".into()));
        assert!(!was_generated);
        assert_eq!(explicit, "retry-1");
    }

    #[test]
    fn formats_program_failure_as_the_direct_cause() {
        let error = CliError::ExecutionFailed("program exited with code 1".into());
        assert_eq!(error.to_string(), "program exited with code 1");
    }

    #[test]
    fn classifies_completed_execution_by_process_result() {
        let mut success = execution_fixture("completed");
        success.exit_code = Some(0);
        success.mode = "remote_executor".into();
        assert!(classify_execution_result(&success).is_ok());

        let mut failed = success.clone();
        failed.exit_code = Some(7);
        assert!(matches!(
            classify_execution_result(&failed),
            Err(CliError::ExecutionFailed(message)) if message.contains("exited with code 7")
        ));

        let mut missing = success.clone();
        missing.exit_code = None;
        assert!(matches!(
            classify_execution_result(&missing),
            Err(CliError::ExecutionFailed(message)) if message.contains("no exit code")
        ));
    }

    #[test]
    fn classifies_mock_execution_as_not_executed() {
        let mut mock = execution_fixture("completed");
        mock.exit_code = None;
        mock.mode = "mock_no_code_execution".into();
        assert!(matches!(
            classify_execution_result(&mock),
            Err(CliError::ExecutionFailed(message)) if message.contains("was not executed")
        ));
    }

    #[test]
    fn reads_python_source_from_a_file_or_stdin_mode() {
        let path = std::env::temp_dir().join(format!("mpaka-cli-{}.py", std::process::id()));
        std::fs::write(&path, "print(1)\n").expect("write script");
        assert_eq!(
            read_source(&RunArgs {
                file: Some(path.clone()),
                stdin: false,
                timeout_ms: 3000,
                memory_mb: 128,
                idempotency_key: None,
                max_poll_attempts: DEFAULT_MAX_POLL_ATTEMPTS,
            })
            .expect("source"),
            "print(1)\n"
        );
        std::fs::remove_file(path).expect("remove script");
    }

    #[test]
    fn recognizes_terminal_execution_statuses() {
        assert!(is_terminal_status("completed"));
        assert!(is_terminal_status("failed"));
        assert!(is_terminal_status("timed_out"));
        assert!(is_terminal_status("cancelled"));
        assert!(!is_terminal_status("running"));
    }

    #[tokio::test]
    async fn preserves_execution_identity_when_polling_limit_is_reached() {
        let execution_id = "00000000-0000-0000-0000-000000000001";
        let server = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = server.local_addr().expect("address");
        let app = Router::new().route(
            &format!("/v1/executions/{execution_id}"),
            get(|| async {
                Json(json!({
                    "id":"00000000-0000-0000-0000-000000000001",
                    "status":"running",
                    "language":"python",
                    "timeout_ms":3000,
                    "memory_mb":128,
                    "created_at":"2026-10-28T10:00:00Z",
                    "finished_at":null,
                    "stdout":"",
                    "stderr":"",
                    "exit_code":null,
                    "mode":"remote_executor"
                }))
            }),
        );
        tokio::spawn(async move {
            axum::serve(server, app).await.expect("server");
        });
        let client = AuthClient::new(&format!("http://{address}")).expect("client");

        let result = poll_execution(&client, "api-key", execution_fixture("running"), 2).await;

        assert!(matches!(
            result,
            Err(CliError::PollingTimeout { id, status })
                if id == execution_id && status == "running"
        ));
    }

    #[tokio::test]
    async fn counts_the_initial_execution_as_the_first_polling_attempt() {
        let execution_id = "00000000-0000-0000-0000-000000000001";
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_for_handler = Arc::clone(&requests);
        let server = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = server.local_addr().expect("address");
        let app = Router::new().route(
            &format!("/v1/executions/{execution_id}"),
            get(move || {
                requests_for_handler.fetch_add(1, Ordering::SeqCst);
                async {
                    Json(json!({
                        "id":"00000000-0000-0000-0000-000000000001",
                        "status":"running",
                        "language":"python",
                        "timeout_ms":3000,
                        "memory_mb":128,
                        "created_at":"2026-10-28T10:00:00Z",
                        "finished_at":null,
                        "stdout":"",
                        "stderr":"",
                        "exit_code":null,
                        "mode":"remote_executor"
                    }))
                }
            }),
        );
        tokio::spawn(async move {
            axum::serve(server, app).await.expect("server");
        });
        let client = AuthClient::new(&format!("http://{address}")).expect("client");

        let result = poll_execution(&client, "api-key", execution_fixture("running"), 1).await;

        assert!(matches!(
            result,
            Err(CliError::PollingTimeout { id, status })
                if id == execution_id && status == "running"
        ));
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }
}
