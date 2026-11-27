use axum::{Json, Router, extract::OriginalUri, routing::get};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::{net::TcpListener, process::Command};

async fn spawn_api(router: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, router).await.expect("server") });
    format!("http://{address}")
}

async fn run_cli(endpoint: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mpaka"))
        .arg("--endpoint")
        .arg(endpoint)
        .args(args)
        .env("MPAKA_API_KEY", "test-api-key")
        .env_remove("MPAKA_API_TIMEOUT_SECONDS")
        .output()
        .await
        .expect("run mpaka")
}

#[tokio::test]
async fn failed_environment_wait_prints_json_and_exits_nonzero() {
    let id = "00000000-0000-0000-0000-000000000001";
    let router = Router::new().route(
        &format!("/v1/environments/{id}"),
        get(|| async {
            Json(json!({
                "id": "00000000-0000-0000-0000-000000000001",
                "status": "failed",
                "error": "package installation failed"
            }))
        }),
    );
    let endpoint = spawn_api(router).await;

    let output = run_cli(
        &endpoint,
        &[
            "environments",
            "wait",
            id,
            "--json",
            "--wait-timeout-ms",
            "1000",
        ],
    )
    .await;

    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).expect("structured stdout");
    assert_eq!(response["status"], "failed");
    assert_eq!(response["error"], "package installation failed");
    assert!(String::from_utf8_lossy(&output.stderr).contains("environment build failed"));
}

#[tokio::test]
async fn environment_list_sends_explicit_pagination_parameters() {
    let request_uri = Arc::new(Mutex::new(None));
    let handler_uri = Arc::clone(&request_uri);
    let router = Router::new().route(
        "/v1/environments",
        get(move |uri: OriginalUri| {
            *handler_uri.lock().expect("request uri lock") = Some(uri.0.to_string());
            async { Json(json!({"items": [], "page": 3, "page_size": 40})) }
        }),
    );
    let endpoint = spawn_api(router).await;

    let output = run_cli(
        &endpoint,
        &[
            "environments",
            "list",
            "--json",
            "--page",
            "3",
            "--page-size",
            "40",
        ],
    )
    .await;

    assert!(output.status.success());
    assert_eq!(
        request_uri.lock().expect("request uri lock").as_deref(),
        Some("/v1/environments?page=3&page_size=40")
    );
    let response: Value = serde_json::from_slice(&output.stdout).expect("structured stdout");
    assert_eq!(response["page"], 3);
    assert_eq!(response["page_size"], 40);
}

#[tokio::test]
async fn environment_list_sends_api_pagination_defaults() {
    let request_uri = Arc::new(Mutex::new(None));
    let handler_uri = Arc::clone(&request_uri);
    let router = Router::new().route(
        "/v1/environments",
        get(move |uri: OriginalUri| {
            *handler_uri.lock().expect("request uri lock") = Some(uri.0.to_string());
            async { Json(json!({"items": [], "page": 1, "page_size": 25})) }
        }),
    );
    let endpoint = spawn_api(router).await;

    let output = run_cli(&endpoint, &["environments", "list", "--json"]).await;

    assert!(output.status.success());
    assert_eq!(
        request_uri.lock().expect("request uri lock").as_deref(),
        Some("/v1/environments?page=1&page_size=25")
    );
}
