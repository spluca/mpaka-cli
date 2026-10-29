# mPaka CLI

## Install the CLI

The supported release channel is published from `https://cli.mpaka.dev`.
It does not require Rust or administrator privileges.

On Linux and macOS:

```bash
curl -fsSL https://cli.mpaka.dev/install | bash
mpaka --version
```

To install an exact immutable release, pass its version to the installer:

```bash
curl -fsSL https://cli.mpaka.dev/install | bash -s -- --version v0.1.0
```

On Windows PowerShell:

```powershell
irm https://cli.mpaka.dev/install.ps1 | iex
mpaka --version
```

The installers download only over HTTPS, verify the signed release manifest
and the SHA-256 hash of the selected archive, reject unexpected archive
entries, and install to a user-owned directory. Unix installs use
`~/.local/bin`; Windows installs use `%LOCALAPPDATA%\mPaka\bin`.

The `curl | bash` form necessarily trusts the installer script delivered over
HTTPS. The embedded public key protects release metadata and binary archives,
not the initial script itself. For a higher-assurance installation, download
`install` or `install.ps1`, inspect it, and run it locally. The embedded key is
also available in the source tree for independent verification.

Supported release targets:

| Platform | Architecture | Target |
| --- | --- | --- |
| Linux | x86_64 | `x86_64-unknown-linux-musl` |
| Linux | ARMv7 | `armv7-unknown-linux-musleabihf` |
| Linux | AArch64 | `aarch64-unknown-linux-musl` |
| Windows | x86_64 | `x86_64-pc-windows-msvc` |
| macOS | x86_64 | `x86_64-apple-darwin` |
| macOS | Apple Silicon | `aarch64-apple-darwin` |

The release channel keeps versioned artifacts immutable. The `latest` pointer
is updated only after all versioned objects have been uploaded and publicly
verified. If a release must be withdrawn, point `latest` back to the last
known-good version; existing versioned releases remain available for pinned
installs and rollback.

The CLI can check and apply updates after installation:

```bash
mpaka self-update --check
mpaka self-update
mpaka self-update --version v0.1.0  # explicit rollback or pin
```

On Windows, self-update uses a short-lived helper process to replace the
executable after the running process exits. An interrupted update leaves the
existing executable in place.

The workspace includes a Rust CLI named `mpaka`. Account commands use the web
session boundary; execution commands use an execution API key. The web console
does not accept source code for execution.

## Configuration

The CLI uses the mPaka API at `https://api.mpaka.dev` by default. Set
`MPAKA_API_URL` or pass `--endpoint` to use another API endpoint.

The opaque web session is stored at `~/.config/mpaka/session`. Set
`RUSTBOX_SESSION_FILE` to choose another path. The file is created with
owner-only permissions on Unix; symlinked or insecure session files are
rejected, and Unix reads use a no-follow file descriptor to avoid following a
symlink introduced concurrently.

## Commands

```bash
mpaka register --email user@example.com
mpaka login --email user@example.com
mpaka whoami
mpaka logout
export MPAKA_API_KEY='...'
mpaka run --file script.py
mpaka run --file script.py --idempotency-key deploy-check-1
mpaka run --file script.py --max-poll-attempts 1200
cat script.py | mpaka run --stdin
mpaka executions list
mpaka executions get <EXECUTION_ID>
mpaka executions cancel <EXECUTION_ID>
```

Passwords are read from the terminal without echoing on Unix and Windows.
Automation can use `--password-stdin`; passwords are never accepted as
command-line arguments:

```bash
printf '%s\n' "$MPAKA_PASSWORD" | mpaka login \
  --email user@example.com --password-stdin
```

`whoami` uses the stored session to call `GET /v1/me`. `logout` calls
`POST /v1/logout` and removes the local session after the server accepts the
logout, including an already-expired session.

`run` requires `MPAKA_API_KEY` and submits Python through
`POST /v1/executions`, then polls `GET /v1/executions/{id}` until the execution
is terminal. By default it makes up to 240 polling attempts at 250 ms
intervals, counting the response returned by the initial submission as the
first attempt. Use `--max-poll-attempts <N>` to choose another positive
bounded polling budget. If that budget is exhausted, the command returns a
non-zero status and reports the execution ID and last known lifecycle status
so the same execution can be inspected with
`mpaka executions get <EXECUTION_ID>`.

The CLI generates an `Idempotency-Key` when `--idempotency-key` is omitted and
prints that generated key to `stderr` before submission. If submission is
ambiguous because the connection fails, rerun the same request with that key
to recover the original submission instead of creating a duplicate. The key
is not an API credential. Create the execution
API key in the API keys section of the web console and keep it in the
environment or a server-side secret store; do not pass the API key as a
command-line argument. Optional execution limits are `--timeout-ms` (default
`3000`) and `--memory-mb` (default `128`). Use either `--file <PATH>` or
`--stdin`.

The command prints status, mode, exit code, stdout, and stderr. A zero process
exit code means the API reports `completed`, the result is not
`mock_no_code_execution`, and the program returned exit code `0`. A non-zero
program exit code, a missing exit code, `failed`, `timed_out`, or `cancelled`
status all produce a non-zero CLI exit code. Local control-plane runs may
report `mock_no_code_execution`; that is an explicit not-executed result and
not evidence of production Python sandbox execution.

Execution inspection commands use the same API key:

```bash
mpaka executions list
mpaka executions list --json
mpaka executions get "$EXECUTION_ID"
mpaka executions get "$EXECUTION_ID" --json
mpaka executions cancel "$EXECUTION_ID"
```

`list` uses the server-bounded execution history and does not accept a client
side pagination flag. `get` returns lifecycle metadata plus stdout, stderr,
exit code, and mode. `cancel` requests cancellation and prints the state
returned by the API; completion can win a race with cancellation, so the
returned terminal status is authoritative. Use `--json` when another program
will consume the result.

Missing credentials, transport failures, invalid IDs, and API error responses
return a non-zero exit code. Inspection commands return zero when the API
request succeeds, even when the execution itself is `failed`, `timed_out`, or
`cancelled`.

The CLI does not print session tokens, API keys, or passwords. The same
API-key execution contract is intended for the future TypeScript and Python
SDKs.
