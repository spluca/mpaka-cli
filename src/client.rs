use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};

use crate::session::Session;

#[derive(Clone)]
pub struct AuthClient {
    client: reqwest::Client,
    endpoint: Url,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub user_id: String,
    pub email: String,
    pub tenant_id: String,
    pub tenant_name: String,
    pub role: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClientError {
    InvalidEndpoint,
    RequestFailed,
    InvalidCredentials,
    RegistrationConflict,
    InvalidRegistration,
    InvalidSession,
    ServiceUnavailable,
    InvalidResponse,
    MissingSession,
    Api { code: String },
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidEndpoint => "the API endpoint is invalid",
            Self::RequestFailed => "the authentication service could not be reached",
            Self::InvalidCredentials => "the email or password is invalid",
            Self::RegistrationConflict => "that email is already registered",
            Self::InvalidRegistration => "the email or password is invalid for registration",
            Self::InvalidSession => "the stored session is no longer valid",
            Self::ServiceUnavailable => "the authentication service is unavailable",
            Self::InvalidResponse => "the authentication service returned an invalid response",
            Self::MissingSession => "the authentication service did not return a session",
            Self::Api { code } => return write!(formatter, "the API request failed ({code})"),
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ClientError {}

#[derive(Serialize)]
struct Credentials<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Debug, Serialize)]
pub struct ExecutionRequest {
    pub language: String,
    pub code: String,
    pub timeout_ms: u64,
    pub memory_mb: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExecutionResponse {
    pub id: String,
    pub status: String,
    pub language: String,
    pub timeout_ms: u64,
    pub memory_mb: u64,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub mode: String,
}

#[derive(Deserialize)]
struct ApiErrorEnvelope {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    code: String,
}

impl AuthClient {
    pub fn new(endpoint: &str) -> Result<Self, ClientError> {
        let endpoint =
            Url::parse(endpoint.trim_end_matches('/')).map_err(|_| ClientError::InvalidEndpoint)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| ClientError::RequestFailed)?;
        Ok(Self { client, endpoint })
    }

    pub async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> Result<(Identity, Session), ClientError> {
        self.authenticate("/v1/login", email, password).await
    }

    pub async fn register(
        &self,
        email: &str,
        password: &str,
    ) -> Result<(Identity, Session), ClientError> {
        self.authenticate("/v1/register", email, password).await
    }

    pub async fn me(&self, session: &Session) -> Result<Identity, ClientError> {
        let response = self
            .request_with_session(reqwest::Method::GET, "/v1/me", session)
            .send()
            .await
            .map_err(|_| ClientError::RequestFailed)?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(ClientError::InvalidSession);
        }
        if !response.status().is_success() {
            return Err(ClientError::InvalidResponse);
        }
        response
            .json::<Identity>()
            .await
            .map_err(|_| ClientError::InvalidResponse)
    }

    pub async fn logout(&self, session: &Session) -> Result<(), ClientError> {
        let response = self
            .request_with_session(reqwest::Method::POST, "/v1/logout", session)
            .send()
            .await
            .map_err(|_| ClientError::RequestFailed)?;
        if response.status() == StatusCode::UNAUTHORIZED || response.status().is_success() {
            return Ok(());
        }
        Err(ClientError::InvalidResponse)
    }

    pub async fn create_execution(
        &self,
        api_key: &str,
        request: &ExecutionRequest,
        idempotency_key: &str,
    ) -> Result<ExecutionResponse, ClientError> {
        let response = self
            .request_with_api_key(reqwest::Method::POST, "/v1/executions", api_key)
            .header("idempotency-key", idempotency_key)
            .json(request)
            .send()
            .await
            .map_err(|_| ClientError::RequestFailed)?;
        parse_execution_response(response).await
    }

    pub async fn get_execution(
        &self,
        api_key: &str,
        id: &str,
    ) -> Result<ExecutionResponse, ClientError> {
        let path = format!("/v1/executions/{id}");
        let response = self
            .request_with_api_key(reqwest::Method::GET, &path, api_key)
            .send()
            .await
            .map_err(|_| ClientError::RequestFailed)?;
        parse_execution_response(response).await
    }

    pub async fn list_executions(
        &self,
        api_key: &str,
    ) -> Result<Vec<ExecutionResponse>, ClientError> {
        let response = self
            .request_with_api_key(reqwest::Method::GET, "/v1/executions", api_key)
            .send()
            .await
            .map_err(|_| ClientError::RequestFailed)?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_error("/v1/executions", status, response).await);
        }
        response
            .json::<Vec<ExecutionResponse>>()
            .await
            .map_err(|_| ClientError::InvalidResponse)
    }

    pub async fn cancel_execution(
        &self,
        api_key: &str,
        id: &str,
    ) -> Result<ExecutionResponse, ClientError> {
        let path = format!("/v1/executions/{id}/cancel");
        let response = self
            .request_with_api_key(reqwest::Method::POST, &path, api_key)
            .send()
            .await
            .map_err(|_| ClientError::RequestFailed)?;
        parse_execution_response(response).await
    }

    fn request_with_session(
        &self,
        method: reqwest::Method,
        path: &str,
        session: &Session,
    ) -> reqwest::RequestBuilder {
        let url = self
            .endpoint
            .join(path)
            .expect("authenticated API path must be valid");
        self.client
            .request(method, url)
            .header("cookie", format!("rustbox_session={}", session.token()))
    }

    fn request_with_api_key(
        &self,
        method: reqwest::Method,
        path: &str,
        api_key: &str,
    ) -> reqwest::RequestBuilder {
        let url = self
            .endpoint
            .join(path)
            .expect("authenticated API path must be valid");
        self.client
            .request(method, url)
            .header("authorization", format!("Bearer {api_key}"))
    }

    async fn authenticate(
        &self,
        path: &str,
        email: &str,
        password: &str,
    ) -> Result<(Identity, Session), ClientError> {
        let url = self
            .endpoint
            .join(path)
            .map_err(|_| ClientError::InvalidEndpoint)?;
        let response = self
            .client
            .post(url)
            .json(&Credentials { email, password })
            .send()
            .await
            .map_err(|_| ClientError::RequestFailed)?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_error(path, status, response).await);
        }
        let token = response
            .headers()
            .get("set-cookie")
            .and_then(|value| extract_session_token(value.to_str().ok()?))
            .ok_or(ClientError::MissingSession)?
            .to_owned();
        let identity = response
            .json::<Identity>()
            .await
            .map_err(|_| ClientError::InvalidResponse)?;
        Ok((identity, Session::from_token(token)))
    }
}

async fn map_error(path: &str, status: StatusCode, response: reqwest::Response) -> ClientError {
    let code = response
        .json::<ApiErrorEnvelope>()
        .await
        .ok()
        .map(|envelope| envelope.error.code);
    match (path, status, code.as_deref()) {
        ("/v1/login", StatusCode::UNAUTHORIZED, Some("invalid_credentials")) => {
            ClientError::InvalidCredentials
        }
        ("/v1/register", StatusCode::CONFLICT, Some("registration_conflict")) => {
            ClientError::RegistrationConflict
        }
        ("/v1/register", StatusCode::UNPROCESSABLE_ENTITY, Some("invalid_registration")) => {
            ClientError::InvalidRegistration
        }
        (_, StatusCode::SERVICE_UNAVAILABLE, _) => ClientError::ServiceUnavailable,
        _ => ClientError::Api {
            code: code.unwrap_or_else(|| format!("http_{}", status.as_u16())),
        },
    }
}

async fn parse_execution_response(
    response: reqwest::Response,
) -> Result<ExecutionResponse, ClientError> {
    let status = response.status();
    if !status.is_success() {
        return Err(map_error("/v1/executions", status, response).await);
    }
    response
        .json::<ExecutionResponse>()
        .await
        .map_err(|_| ClientError::InvalidResponse)
}

fn extract_session_token(set_cookie: &str) -> Option<&str> {
    let value = set_cookie.strip_prefix("rustbox_session=")?;
    let end = value.find(';').unwrap_or(value.len());
    let token = &value[..end];
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        http::{HeaderMap, HeaderValue, StatusCode},
        routing::{get, post},
    };
    use serde_json::json;
    use tokio::net::TcpListener;

    #[test]
    fn extracts_only_the_session_cookie_value() {
        assert_eq!(
            extract_session_token("rustbox_session=opaque; Path=/; HttpOnly"),
            Some("opaque")
        );
        assert_eq!(extract_session_token("other=value"), None);
    }

    #[tokio::test]
    async fn login_posts_to_the_v1_login_contract_and_stores_cookie() {
        let server = spawn_server(Router::new().route(
            "/v1/login",
            post(|Json(payload): Json<serde_json::Value>| async move {
                assert_eq!(
                    payload,
                    json!({"email":"alice@example.com","password":"secret"})
                );
                let mut headers = HeaderMap::new();
                headers.insert(
                    "set-cookie",
                    HeaderValue::from_static("rustbox_session=opaque; Path=/; HttpOnly"),
                );
                (
                    StatusCode::OK,
                    headers,
                    Json(json!({
                        "user_id":"user-1",
                        "email":"alice@example.com",
                        "tenant_id":"tenant-1",
                        "tenant_name":"Alice workspace",
                        "role":"owner"
                    })),
                )
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        let (identity, session) = client
            .login("alice@example.com", "secret")
            .await
            .expect("login");
        assert_eq!(identity.email, "alice@example.com");
        assert_eq!(session.token(), "opaque");
    }

    #[tokio::test]
    async fn maps_safe_login_and_registration_errors() {
        let server = spawn_server(
            Router::new()
                .route(
                    "/v1/login",
                    post(|| async {
                        (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({"error":{"code":"invalid_credentials"}})),
                        )
                    }),
                )
                .route(
                    "/v1/register",
                    post(|| async {
                        (
                            StatusCode::CONFLICT,
                            Json(json!({"error":{"code":"registration_conflict"}})),
                        )
                    }),
                ),
        )
        .await;
        let client = AuthClient::new(&server).expect("client");
        assert!(matches!(
            client.login("alice@example.com", "wrong").await,
            Err(ClientError::InvalidCredentials)
        ));
        assert!(matches!(
            client.register("alice@example.com", "secret").await,
            Err(ClientError::RegistrationConflict)
        ));
    }

    #[tokio::test]
    async fn me_sends_the_stored_session_cookie() {
        let server = spawn_server(Router::new().route(
            "/v1/me",
            get(|request: axum::extract::Request| async move {
                assert_eq!(
                    request
                        .headers()
                        .get("cookie")
                        .and_then(|value| value.to_str().ok()),
                    Some("rustbox_session=opaque")
                );
                Json(json!({
                    "user_id":"user-1",
                    "email":"alice@example.com",
                    "tenant_id":"tenant-1",
                    "tenant_name":"Alice workspace",
                    "role":"owner"
                }))
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        let identity = client
            .me(&Session::from_token("opaque"))
            .await
            .expect("identity");
        assert_eq!(identity.email, "alice@example.com");
    }

    #[tokio::test]
    async fn logout_posts_the_stored_session_cookie() {
        let server = spawn_server(Router::new().route(
            "/v1/logout",
            post(|request: axum::extract::Request| async move {
                assert_eq!(
                    request
                        .headers()
                        .get("cookie")
                        .and_then(|value| value.to_str().ok()),
                    Some("rustbox_session=opaque")
                );
                StatusCode::NO_CONTENT
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        client
            .logout(&Session::from_token("opaque"))
            .await
            .expect("logout");
    }

    #[tokio::test]
    async fn maps_expired_session_to_a_safe_error() {
        let server = spawn_server(Router::new().route(
            "/v1/me",
            get(|| async {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error":{"code":"unauthorized"}})),
                )
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        assert!(matches!(
            client.me(&Session::from_token("expired")).await,
            Err(ClientError::InvalidSession)
        ));
    }

    #[tokio::test]
    async fn creates_an_authenticated_python_execution_with_idempotency() {
        let server = spawn_server(Router::new().route(
            "/v1/executions",
            post(|request: axum::extract::Request| async move {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok()),
                    Some("Bearer api-key")
                );
                assert_eq!(
                    request
                        .headers()
                        .get("idempotency-key")
                        .and_then(|value| value.to_str().ok()),
                    Some("run-1")
                );
                let body = axum::body::to_bytes(request.into_body(), usize::MAX)
                    .await
                    .expect("body");
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&body).expect("json"),
                    json!({"language":"python","code":"print(1)","timeout_ms":3000,"memory_mb":128})
                );
                (
                    StatusCode::ACCEPTED,
                    Json(json!({
                        "id":"00000000-0000-0000-0000-000000000001",
                        "status":"queued",
                        "language":"python",
                        "timeout_ms":3000,
                        "memory_mb":128,
                        "created_at":"2026-10-28T10:00:00Z",
                        "finished_at":null,
                        "stdout":"",
                        "stderr":"",
                        "exit_code":null,
                        "mode":"pending"
                    })),
                )
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        let execution = client
            .create_execution(
                "api-key",
                &ExecutionRequest {
                    language: "python".into(),
                    code: "print(1)".into(),
                    timeout_ms: 3000,
                    memory_mb: 128,
                },
                "run-1",
            )
            .await
            .expect("execution");
        assert_eq!(execution.status, "queued");
        assert_eq!(execution.mode, "pending");
    }

    #[tokio::test]
    async fn gets_an_execution_result_and_maps_api_errors() {
        let server = spawn_server(
            Router::new()
                .route(
                    "/v1/executions/00000000-0000-0000-0000-000000000001",
                    get(|| async {
                        Json(json!({
                            "id":"00000000-0000-0000-0000-000000000001",
                            "status":"completed",
                            "language":"python",
                            "timeout_ms":3000,
                            "memory_mb":128,
                            "created_at":"2026-10-28T10:00:00Z",
                            "finished_at":"2026-10-28T10:00:01Z",
                            "stdout":"2\n",
                            "stderr":"",
                            "exit_code":0,
                            "mode":"mock_no_code_execution"
                        }))
                    }),
                )
                .route(
                    "/v1/executions/00000000-0000-0000-0000-000000000002",
                    get(|| async {
                        (
                            StatusCode::TOO_MANY_REQUESTS,
                            Json(json!({"error":{"code":"rate_limit_exceeded","message":"try later"}})),
                        )
                    }),
                ),
        )
        .await;
        let client = AuthClient::new(&server).expect("client");
        let execution = client
            .get_execution("api-key", "00000000-0000-0000-0000-000000000001")
            .await
            .expect("result");
        assert_eq!(execution.stdout, "2\n");
        assert_eq!(execution.exit_code, Some(0));
        assert_eq!(
            client
                .get_execution("api-key", "00000000-0000-0000-0000-000000000002",)
                .await,
            Err(ClientError::Api {
                code: "rate_limit_exceeded".into()
            })
        );
    }

    #[tokio::test]
    async fn lists_authenticated_executions() {
        let server = spawn_server(Router::new().route(
            "/v1/executions",
            get(|request: axum::extract::Request| async move {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok()),
                    Some("Bearer api-key")
                );
                Json(json!([
                    {
                        "id":"00000000-0000-0000-0000-000000000001",
                        "status":"completed",
                        "language":"python",
                        "timeout_ms":3000,
                        "memory_mb":128,
                        "created_at":"2026-10-28T10:00:00Z",
                        "finished_at":"2026-10-28T10:00:01Z",
                        "stdout":"2\n",
                        "stderr":"",
                        "exit_code":0,
                        "mode":"mock_no_code_execution"
                    }
                ]))
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        let executions = client.list_executions("api-key").await.expect("executions");
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].status, "completed");
        assert_eq!(executions[0].stdout, "2\n");
        assert_eq!(executions[0].mode, "mock_no_code_execution");
    }

    #[tokio::test]
    async fn cancels_an_execution_with_bearer_auth() {
        let server = spawn_server(Router::new().route(
            "/v1/executions/00000000-0000-0000-0000-000000000001/cancel",
            post(|request: axum::extract::Request| async move {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok()),
                    Some("Bearer api-key")
                );
                Json(json!({
                    "id":"00000000-0000-0000-0000-000000000001",
                    "status":"cancelled",
                    "language":"python",
                    "timeout_ms":3000,
                    "memory_mb":128,
                    "created_at":"2026-10-28T10:00:00Z",
                    "finished_at":"2026-10-28T10:00:01Z",
                    "stdout":"",
                    "stderr":"execution cancelled by request",
                    "exit_code":null,
                    "mode":"cancelled_by_request"
                }))
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        let execution = client
            .cancel_execution("api-key", "00000000-0000-0000-0000-000000000001")
            .await
            .expect("execution");
        assert_eq!(execution.status, "cancelled");
        assert_eq!(execution.mode, "cancelled_by_request");
    }

    #[tokio::test]
    async fn maps_list_api_errors_to_safe_codes() {
        let server = spawn_server(Router::new().route(
            "/v1/executions",
            get(|| async {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error":{"code":"rate_limit_exceeded"}})),
                )
            }),
        ))
        .await;
        let client = AuthClient::new(&server).expect("client");
        assert_eq!(
            client.list_executions("api-key").await,
            Err(ClientError::Api {
                code: "rate_limit_exceeded".into()
            })
        );
    }

    async fn spawn_server(router: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("server");
        });
        format!("http://{address}")
    }
}
