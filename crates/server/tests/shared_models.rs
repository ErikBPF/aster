use std::sync::Arc;

use aster_core::*;
use aster_server::{app, AppState, DestinationPolicy, Keyring, Metrics, SharedModels};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::Engine;
use serde_json::{json, Value};
use tower::ServiceExt;

fn app_with_personal_helpers() -> Router {
    let keyset = json!({"v1": base64::engine::general_purpose::STANDARD.encode([7u8; 32])});
    let shared_models = SharedModels::new(
        Arc::new(InMemorySharedModels::default()),
        DestinationPolicy::parse("https://approved.example.invalid").unwrap(),
        Keyring::parse("v1", &keyset.to_string()).unwrap(),
    );
    app(Arc::new(AppState {
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
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: Some(Arc::new(shared_models)),
        current_identity: None,
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        contracts: Arc::new(vec![]),
        http: reqwest::Client::new(),
        sessions: Arc::new(InMemorySessions::new(3600)),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    }))
}

struct NoNotebooks;

#[async_trait::async_trait]
impl NotebookStore for NoNotebooks {
    async fn get(&self, _: &str) -> Result<Notebook> {
        Err(CoreError::NotFound("no notebooks".into()))
    }
    async fn save(&self, _: &Notebook, _: &str) -> Result<String> {
        Err(CoreError::NotFound("no notebooks".into()))
    }
    async fn list(&self, _: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    subject: &str,
    role: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("x-aster-subject", subject)
                .header("x-aster-roles", role)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn only_admin_can_register_a_shared_model() {
    let app = app_with_personal_helpers();
    let payload = json!({
        "base_url": "https://approved.example.invalid/v1",
        "model": "deepseek-v4.1",
        "api_key": "shared-secret-token"
    });
    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen",
        "alice",
        "editor",
        payload,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_shared_registration_does_not_shadow_personal_name_or_disclose_token() {
    let app = app_with_personal_helpers();
    let personal = json!({
        "base_url": "http://personal.example.invalid/v1",
        "model": "personal-qwen",
        "api_key": "personal-secret-token"
    });
    let (status, _) = request(&app, "PUT", "/api/llm/qwen", "alice", "editor", personal).await;
    assert_eq!(status, StatusCode::OK);

    let shared = json!({
        "base_url": "https://approved.example.invalid/v1",
        "model": "shared-qwen",
        "api_key": "shared-secret-token"
    });
    let (status, body) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen",
        "root",
        "admin",
        shared,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "qwen");
    assert_eq!(body["model"], "shared-qwen");
    assert!(!body.to_string().contains("shared-secret-token"));

    let (status, list) = request(
        &app,
        "GET",
        "/api/admin/shared-models",
        "root",
        "admin",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list[0]["id"], "qwen");
    assert!(!list.to_string().contains("shared-secret-token"));
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/admin/shared-models/rekey",
            "alice",
            "editor",
            json!({}),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/admin/shared-models/rekey",
            "root",
            "admin",
            json!({}),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let form_rekey = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/shared-models/rekey")
                .header("x-aster-subject", "root")
                .header("x-aster-roles", "admin")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("go=1"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(form_rekey.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let (status, personal_list) =
        request(&app, "GET", "/api/llm", "alice", "editor", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(personal_list[0]["model"], "personal-qwen");
    assert!(!personal_list.to_string().contains("shared-qwen"));

    assert_eq!(
        request(
            &app,
            "DELETE",
            "/api/admin/shared-models/qwen",
            "root",
            "admin",
            json!({}),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/admin/shared-models",
            "root",
            "admin",
            json!({})
        )
        .await
        .1,
        json!([])
    );
    assert_eq!(
        request(&app, "GET", "/api/llm", "alice", "editor", json!({}))
            .await
            .1[0]["model"],
        "personal-qwen"
    );
}

#[tokio::test]
async fn admin_registry_is_invisible_to_ordinary_list_selection_and_generation() {
    let app = app_with_personal_helpers();
    let shared = json!({
        "base_url": "https://approved.example.invalid/v1",
        "model": "shared-qwen",
        "api_key": "shared-secret-token"
    });
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen",
            "root",
            "admin",
            shared
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/admin/shared-models",
            "alice",
            "editor",
            json!({})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "GET", "/api/llm", "alice", "editor", json!({}))
            .await
            .1,
        json!([])
    );
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/notebooks/sales/helper",
            "alice",
            "editor",
            json!({"helper":"qwen"}),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/ai",
            "alice",
            "editor",
            json!({"prompt":"SELECT 1", "helper":"qwen"}),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn unapproved_and_credential_bearing_destinations_are_refused() {
    let app = app_with_personal_helpers();
    for destination in [
        "https://other.example.invalid/v1",
        "http://approved.example.invalid/v1",
        "https://user:secret@approved.example.invalid/v1",
        "https://approved.example.invalid/v1?token=secret",
    ] {
        let (status, body) = request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen",
            "root",
            "admin",
            json!({"base_url":destination,"model":"qwen","api_key":"shared-secret-token"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.to_string().contains("shared-secret-token"));
        assert!(!body.to_string().contains("user:secret"));
    }
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/admin/shared-models",
            "root",
            "admin",
            json!({})
        )
        .await
        .1,
        json!([])
    );
}
