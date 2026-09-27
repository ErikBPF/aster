use std::sync::{Arc, Mutex};

use aster_core::*;
use aster_server::{app, AppState, Metrics};
use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request, StatusCode},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

struct NoNotebooks;

#[async_trait::async_trait]
impl NotebookStore for NoNotebooks {
    async fn get(&self, _: &str) -> Result<Notebook> {
        unreachable!()
    }
    async fn list(&self, _: &str) -> Result<Vec<String>> {
        unreachable!()
    }
    async fn save(&self, _: &Notebook, _: &str) -> Result<String> {
        unreachable!()
    }
}

async fn fake_model(
    sql: &'static str,
) -> (
    String,
    Arc<Mutex<Vec<(String, String)>>>,
    tokio::task::JoinHandle<()>,
) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&calls);
    let mock = Router::new().route(
        "/v1/chat/completions",
        post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let captured = Arc::clone(&captured);
            async move {
                let model = body["model"].as_str().unwrap().to_owned();
                let auth = headers["authorization"].to_str().unwrap().to_owned();
                captured.lock().unwrap().push((model, auth));
                Json(json!({"choices":[{"message":{"content":sql}}]}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    (url, calls, task)
}

fn test_app() -> Router {
    test_app_with_llm(Arc::new(InMemoryLlm::new()))
}

fn test_app_with_llm(llm: Arc<InMemoryLlm>) -> Router {
    app(Arc::new(test_state_with_llm(llm)))
}

fn test_state_with_llm(llm: Arc<InMemoryLlm>) -> AppState {
    AppState {
        config: AppConfig {
            bind: String::new(),
            engines: vec![],
            catalogs: vec![],
            catalog_bindings: vec![],
            default_engine: None,
            default_catalog: None,
        },
        engines: EngineRegistry::new(),
        catalogs: CatalogRegistry::new(),
        grants: Arc::new(InMemoryGrants::new()),
        audit: Arc::new(InMemoryAudit::new()),
        notebooks: Arc::new(NoNotebooks),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        team_git_targets: None,
        llm,
        shared_models: None,
        current_identity: None,
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        contracts: Arc::new(vec![]),
        http: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap(),
        sessions: Arc::new(InMemorySessions::new(3600)),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    }
}

struct TrustedIdentity;

#[async_trait::async_trait]
impl IdentityProvider for TrustedIdentity {
    fn kind(&self) -> &'static str {
        "test"
    }

    async fn begin(&self) -> Result<IdentityHandshake> {
        unreachable!()
    }

    async fn complete(&self, _: &str, _: &str, _: &str) -> Result<Identity> {
        Ok(Identity {
            subject: "alice".into(),
            roles: vec![Role::Editor],
            groups: vec!["/org/analysts".into()],
            user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
        })
    }
}

#[tokio::test]
async fn callback_keeps_trusted_groups_and_ignores_forged_request_claims() {
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    state.identity = Some(Arc::new(TrustedIdentity));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    state
        .handshakes
        .put("challenge", "verifier:nonce", now)
        .await
        .unwrap();
    let sessions = Arc::clone(&state.sessions);
    let app = app(Arc::new(state));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/callback?code=valid&state=challenge")
                .header("x-aster-subject", "mallory")
                .header("x-aster-roles", "admin")
                .header("x-aster-groups", "/org/forged")
                .header("x-aster-user-uuid", "cccccccc-cccc-4ccc-8ccc-cccccccccccc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    let sid = cookie
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("aster_session=")
        .unwrap();
    let record = sessions.get(sid, now).await.unwrap().unwrap();
    assert_eq!(record.subject, "alice");
    assert_eq!(record.roles, vec![Role::Editor]);
    assert_eq!(record.groups, vec!["/org/analysts"]);
    assert_eq!(
        record.user_uuid.as_deref(),
        Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")
    );
    assert!(!record.groups.iter().any(|group| group == "/org/forged"));

    let denied = app
        .oneshot(
            Request::builder()
                .uri("/api/audit")
                .header("cookie", format!("aster_session={sid}"))
                .header("x-aster-subject", "mallory")
                .header("x-aster-roles", "admin")
                .header("x-aster-groups", "/org/forged")
                .header("x-aster-user-uuid", "cccccccc-cccc-4ccc-8ccc-cccccccccccc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn legacy_url_credentials_do_not_appear_in_settings_page() {
    let llm = Arc::new(InMemoryLlm::new());
    llm.put(LlmConfig {
        subject: "alice".into(),
        id: "legacy".into(),
        base_url: "http://url-user:url-secret@example.invalid/v1".into(),
        model: "test".into(),
        api_key: "separate-token".into(),
    })
    .await
    .unwrap();
    let app = test_app_with_llm(llm);
    let (status, _, body) = call(&app, "GET", "/settings/llm", Some("alice"), "editor", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("url-user"));
    assert!(!body.contains("url-secret"));
    assert!(!body.contains("separate-token"));
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    subject: Option<&str>,
    role: &str,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(subject) = subject {
        request = request
            .header("x-aster-subject", subject)
            .header("x-aster-roles", role);
    }
    if path.starts_with("/aster.v1.Aster/") {
        request = request.header("connect-protocol-version", "1");
    }
    let payload = body.map_or_else(Vec::new, |body| body.to_string().into_bytes());
    if !payload.is_empty() {
        request = request.header("content-type", "application/json");
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(payload)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, text)
}

#[tokio::test]
async fn helper_choice_is_private_to_subject_and_notebook_across_rest_and_connect() {
    let app = test_app();
    for subject in ["alice", "bob"] {
        for id in ["alpha", "beta"] {
            let (status, _, _) = call(
                &app,
                "PUT",
                &format!("/api/llm/{id}"),
                Some(subject),
                "editor",
                Some(json!({"base_url":"http://127.0.0.1:9/v1","model":id,"api_key":"fake"})),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
        }
    }

    for (subject, notebook, helper) in [
        ("alice", "sales", "alpha"),
        ("alice", "forecast", "beta"),
        ("bob", "sales", "beta"),
    ] {
        let (status, body, _) = call(
            &app,
            "PUT",
            &format!("/api/notebooks/{notebook}/helper"),
            Some(subject),
            "editor",
            Some(json!({"helper":helper})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["helper"], helper);
    }
    let (status, body, _) = call(
        &app,
        "POST",
        "/aster.v1.Aster/GetNotebookHelper",
        Some("alice"),
        "editor",
        Some(json!({"notebook":"sales"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["helper"], "alpha");
    let (status, body, _) = call(
        &app,
        "POST",
        "/aster.v1.Aster/PutNotebookHelper",
        Some("alice"),
        "editor",
        Some(json!({"notebook":"forecast","helper":"alpha"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["helper"], "alpha");

    for (subject, notebook, expected) in [
        ("alice", "sales", "alpha"),
        ("alice", "forecast", "alpha"),
        ("bob", "sales", "beta"),
    ] {
        let (status, body, _) = call(
            &app,
            "GET",
            &format!("/api/notebooks/{notebook}/helper"),
            Some(subject),
            "editor",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["helper"], expected);
    }

    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/state",
        Some("alice"),
        "editor",
        Some(json!({"notebook":"sales","helper":"beta"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body, _) = call(
        &app,
        "GET",
        "/api/notebooks/sales/helper",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(
        body["helper"], "alpha",
        "legacy state must not overwrite scoped preference"
    );

    let (status, _, _) = call(
        &app,
        "DELETE",
        "/api/llm/alpha",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body, _) = call(
        &app,
        "GET",
        "/api/notebooks/sales/helper",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(body["helper"], "alpha", "deleted choice remains visible");
}

#[tokio::test]
async fn legacy_choice_migrates_once_without_changing_other_notebooks() {
    let app = test_app();
    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/state",
        Some("alice"),
        "editor",
        Some(json!({"notebook":"sales","helper":"alpha"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, other, _) = call(
        &app,
        "GET",
        "/api/notebooks/forecast/helper",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(other["helper"], Value::Null);
    let (status, selected, _) = call(
        &app,
        "GET",
        "/api/notebooks/sales/helper",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(selected["helper"], "alpha");

    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/state",
        Some("alice"),
        "editor",
        Some(json!({"notebook":"sales","helper":"beta"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, selected, _) = call(
        &app,
        "GET",
        "/api/notebooks/sales/helper",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(selected["helper"], "alpha");
}

#[tokio::test]
async fn helper_choice_refuses_invalid_or_unregistered_names() {
    let app = test_app();
    for (notebook, helper, expected) in [
        ("bad!", "alpha", StatusCode::BAD_REQUEST),
        ("sales", "bad helper", StatusCode::BAD_REQUEST),
        ("sales", "alpha", StatusCode::NOT_FOUND),
    ] {
        let (status, _, _) = call(
            &app,
            "PUT",
            &format!("/api/notebooks/{notebook}/helper"),
            Some("alice"),
            "editor",
            Some(json!({"helper":helper})),
        )
        .await;
        assert_eq!(status, expected);
    }
    let (status, _, _) = call(
        &app,
        "GET",
        "/api/notebooks/sales/helper",
        None,
        "editor",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, selected, _) = call(
        &app,
        "GET",
        "/api/notebooks/sales/helper",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(selected["helper"], Value::Null);
}

#[tokio::test]
async fn helper_choice_read_requires_notebook_role_on_rest_and_connect() {
    let app = test_app();
    for (method, path, body) in [
        ("GET", "/api/notebooks/sales/helper", None),
        (
            "POST",
            "/aster.v1.Aster/GetNotebookHelper",
            Some(json!({"notebook":"sales"})),
        ),
    ] {
        let (status, _, _) = call(&app, method, path, Some("alice"), "unrecognized", body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
}

#[tokio::test]
async fn personal_helpers_route_by_subject_and_name_without_exposing_tokens() {
    let (first_url, first_calls, first_task) = fake_model("SELECT 11").await;
    let (second_url, second_calls, second_task) = fake_model("SELECT 22").await;
    let app = test_app();
    for (subject, id, url, model, token) in [
        ("alice", "alpha", &first_url, "model-11", "token-11"),
        ("alice", "beta", &second_url, "model-22", "token-22"),
        ("bob", "alpha", &second_url, "model-bob", "token-bob"),
    ] {
        let (status, _, response) = call(
            &app,
            "PUT",
            &format!("/api/llm/{id}"),
            Some(subject),
            "editor",
            Some(json!({"base_url":url,"model":model,"api_key":token})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!response.contains(token));
    }

    let (status, listed, response) =
        call(&app, "GET", "/api/llm", Some("alice"), "editor", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 2);
    assert_eq!(listed[0]["id"], "alpha");
    assert_eq!(listed[1]["id"], "beta");
    assert!(!response.contains("token-"));
    assert_eq!(first_calls.lock().unwrap().len(), 0);
    assert_eq!(second_calls.lock().unwrap().len(), 0);

    let ask = |helper: &str| json!({"prompt":"one query", "helper":helper});
    let (status, answer, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("alice"),
        "editor",
        Some(ask("alpha")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["sql"], "SELECT 11");
    assert_eq!(first_calls.lock().unwrap().len(), 1);
    assert_eq!(second_calls.lock().unwrap().len(), 0);

    let (status, answer, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("alice"),
        "editor",
        Some(ask("beta")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["sql"], "SELECT 22");
    let (status, answer, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("bob"),
        "editor",
        Some(ask("alpha")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["sql"], "SELECT 22");
    assert_eq!(first_calls.lock().unwrap().len(), 1);
    {
        let calls = second_calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(calls[0].0 == "model-22" && calls[0].1 == "Bearer token-22");
        assert!(calls[1].0 == "model-bob" && calls[1].1 == "Bearer token-bob");
    }

    for (subject, role, expected) in [
        (Some("alice"), "editor", StatusCode::NOT_FOUND),
        (Some("alice"), "viewer", StatusCode::FORBIDDEN),
        (None, "editor", StatusCode::FORBIDDEN),
    ] {
        let (status, _, _) =
            call(&app, "POST", "/api/ai", subject, role, Some(ask("missing"))).await;
        assert_eq!(status, expected);
    }
    assert_eq!(first_calls.lock().unwrap().len(), 1);
    assert_eq!(second_calls.lock().unwrap().len(), 2);

    let (status, _, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("alice"),
        "editor",
        Some(ask("  ")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(first_calls.lock().unwrap().len(), 1);
    assert_eq!(second_calls.lock().unwrap().len(), 2);

    let bad = format!(
        "http://url-user:url-secret@{}/v1",
        first_url.trim_start_matches("http://")
    );
    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/llm/unsafe",
        Some("alice"),
        "editor",
        Some(json!({"base_url":bad,"model":"x","api_key":"separate"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, listed, _) = call(&app, "GET", "/api/llm", Some("alice"), "editor", None).await;
    assert_eq!(listed.as_array().unwrap().len(), 2);

    let (status, _, response) = call(
        &app,
        "PUT",
        "/api/llm/alpha",
        Some("alice"),
        "editor",
        Some(json!({"base_url":second_url,"model":"replacement","api_key":"replacement-token"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!response.contains("replacement-token"));
    let (status, answer, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("alice"),
        "editor",
        Some(ask("alpha")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["sql"], "SELECT 22");
    let (status, answer, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("bob"),
        "editor",
        Some(ask("alpha")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["sql"], "SELECT 22");
    {
        let calls = second_calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert!(calls[2].0 == "replacement" && calls[2].1 == "Bearer replacement-token");
        assert!(calls[3].0 == "model-bob" && calls[3].1 == "Bearer token-bob");
    }

    let (status, _, _) = call(
        &app,
        "DELETE",
        "/api/llm/alpha",
        Some("alice"),
        "editor",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("alice"),
        "editor",
        Some(ask("alpha")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, answer, _) = call(
        &app,
        "POST",
        "/api/ai",
        Some("bob"),
        "editor",
        Some(ask("alpha")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["sql"], "SELECT 22");
    assert_eq!(second_calls.lock().unwrap().len(), 5);

    first_task.abort();
    second_task.abort();
}
