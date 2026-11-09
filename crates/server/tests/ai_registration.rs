use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

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

async fn notebook_helper_app() -> Router {
    let state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    for (id, owner) in [
        ("sales", "alice"),
        ("forecast", "alice"),
        ("bob-sales", "bob"),
    ] {
        state
            .notebook_owners
            .change(NotebookOwnerChange {
                source: state.notebooks.source_key(),
                id: id.into(),
                expected_owner: None,
                owner: owner.into(),
                source_blob: "fixture".into(),
                actor: owner.into(),
                reason: None,
            })
            .await
            .unwrap();
    }
    app(Arc::new(state))
}

fn test_app() -> Router {
    test_app_with_llm(Arc::new(InMemoryLlm::new()))
}

fn test_app_with_llm(llm: Arc<InMemoryLlm>) -> Router {
    app(Arc::new(test_state_with_llm(llm)))
}

fn test_state_with_llm(llm: Arc<InMemoryLlm>) -> AppState {
    AppState {
        catalog_assets: None,
        skills: Arc::new(aster_core::skills::InMemorySkills::default()),
        mcp: None,
        personal_model_policy: Arc::new(aster_core::InMemoryPersonalModelPolicy::with_subjects(&[
            "alice", "bob", "admin", "viewer", "editor", "carol",
        ])),
        personal_model_destinations: None,
        personal_models_enabled: true,
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
        compiled_contracts: None,
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

#[derive(Default)]
struct TrustedIdentity {
    exchanges: AtomicUsize,
    callback_uri: Option<&'static str>,
}

#[async_trait::async_trait]
impl IdentityProvider for TrustedIdentity {
    fn kind(&self) -> &'static str {
        "test"
    }

    fn callback_uri(&self) -> Option<&str> {
        self.callback_uri
    }

    async fn begin(&self) -> Result<IdentityHandshake> {
        let state = new_sid()?;
        Ok(IdentityHandshake {
            url: format!("https://id.example.invalid/authorize?state={state}"),
            state,
            verifier: "verifier".into(),
            nonce: "nonce".into(),
        })
    }

    async fn complete(&self, code: &str, verifier: &str, nonce: &str) -> Result<Identity> {
        self.exchanges.fetch_add(1, Ordering::SeqCst);
        assert_eq!((verifier, nonce), ("verifier", "nonce"));
        if code != "valid" {
            return Err(CoreError::Unauthorized("exchange refused".into()));
        }
        Ok(Identity {
            subject: "alice".into(),
            roles: vec![Role::Editor],
            groups: vec!["/org/analysts".into()],
            user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
        })
    }
}

struct ObservedHandshakes {
    inner: InMemoryHandshakes,
    takes: AtomicUsize,
}

#[async_trait::async_trait]
impl HandshakeStore for ObservedHandshakes {
    async fn put(&self, state: &str, payload: &str, now: i64) -> Result<()> {
        self.inner.put(state, payload, now).await
    }
    async fn take(&self, state: &str, now: i64) -> Result<Option<String>> {
        self.takes.fetch_add(1, Ordering::SeqCst);
        self.inner.take(state, now).await
    }
}

fn oidc_app(ttl: i64) -> (Router, Arc<TrustedIdentity>, Arc<ObservedHandshakes>) {
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    let identity = Arc::new(TrustedIdentity::default());
    let handshakes = Arc::new(ObservedHandshakes {
        inner: InMemoryHandshakes::new(ttl),
        takes: AtomicUsize::new(0),
    });
    state.identity = Some(identity.clone());
    state.handshakes = handshakes.clone();
    state.dev_login = false;
    (app(Arc::new(state)), identity, handshakes)
}

async fn start_oidc(app: &Router) -> (String, HeaderMap) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/login")
                .header("x-forwarded-proto", "http")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let url = reqwest::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    (state, response.headers().clone())
}

fn binding(headers: &HeaderMap) -> String {
    let cookie = headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .find(|v| v.starts_with("__Host-aster_oidc_state="))
        .expect("browser binding cookie");
    assert!(cookie.contains("Path=/; HttpOnly; SameSite=Lax; Secure; Max-Age=300"));
    assert!(!cookie.contains("Domain="));
    cookie.split(';').next().unwrap().to_string()
}

async fn oidc_callback(
    app: &Router,
    state: &str,
    cookies: Option<&str>,
    suffix: &str,
) -> axum::response::Response {
    let mut request = Request::builder().uri(format!("/callback?state={state}&{suffix}"));
    if let Some(cookies) = cookies {
        request = request.header("cookie", cookies);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn binding_cleared(response: &axum::response::Response) {
    assert!(response.headers().get_all("set-cookie").iter().any(
        |v| v == "__Host-aster_oidc_state=; Path=/; HttpOnly; SameSite=Lax; Secure; Max-Age=0"
    ));
}

#[tokio::test]
async fn oidc_callback_missing_binding_cannot_consume_another_browsers_handshake() {
    rejects_foreign_callback(None).await;
}

#[tokio::test]
async fn oidc_callback_wrong_binding_cannot_consume_another_browsers_handshake() {
    rejects_foreign_callback(Some("__Host-aster_oidc_state=another-browser")).await;
}

async fn rejects_foreign_callback(bad_cookie: Option<&str>) {
    let (app, identity, store) = oidc_app(300);
    let (state, headers) = start_oidc(&app).await;
    let denied = oidc_callback(&app, &state, bad_cookie, "code=valid").await;
    assert_eq!(
        denied.status(),
        StatusCode::FORBIDDEN,
        "callback must be browser-bound"
    );
    assert_eq!(identity.exchanges.load(Ordering::SeqCst), 0);
    assert_eq!(
        store.takes.load(Ordering::SeqCst),
        0,
        "reject before store consumption"
    );
    assert!(
        !denied.headers().contains_key("set-cookie"),
        "foreign callbacks must not clear this browser's login"
    );
    let cookie = binding(&headers);
    let accepted = oidc_callback(&app, &state, Some(&cookie), "code=valid").await;
    assert_eq!(accepted.status(), StatusCode::SEE_OTHER);
    binding_cleared(&accepted);
    assert_eq!(identity.exchanges.load(Ordering::SeqCst), 1);
    let replay = oidc_callback(&app, &state, Some(&cookie), "code=valid").await;
    assert_eq!(replay.status(), StatusCode::FORBIDDEN);
    binding_cleared(&replay);
    assert_eq!(identity.exchanges.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn oidc_callback_expired_and_failed_attempts_clear_only_matching_binding() {
    for (ttl, suffix, expected_exchanges) in [
        (0, "code=valid", 0),
        (300, "code=bad", 1),
        (300, "error=access_denied", 0),
    ] {
        let (app, identity, store) = oidc_app(ttl);
        let (state, headers) = start_oidc(&app).await;
        let cookie = binding(&headers);
        let response = oidc_callback(&app, &state, Some(&cookie), suffix).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        binding_cleared(&response);
        assert_eq!(
            identity.exchanges.load(Ordering::SeqCst),
            expected_exchanges
        );
        assert_eq!(store.takes.load(Ordering::SeqCst), 1);
        let replay = oidc_callback(&app, &state, Some(&cookie), "code=valid").await;
        assert_eq!(replay.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            identity.exchanges.load(Ordering::SeqCst),
            expected_exchanges
        );
    }
}

#[tokio::test]
async fn oidc_callback_duplicate_cookie_and_foreign_error_preserve_pending_login() {
    let (app, identity, store) = oidc_app(300);
    let (state, headers) = start_oidc(&app).await;
    let cookie = binding(&headers);
    for (returned, cookies, suffix) in [
        (state.as_str(), format!("{cookie}; {cookie}"), "code=valid"),
        ("foreign-state", cookie.clone(), "error=access_denied"),
    ] {
        let denied = oidc_callback(&app, returned, Some(&cookies), suffix).await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        assert!(!denied.headers().contains_key("set-cookie"));
        assert_eq!(store.takes.load(Ordering::SeqCst), 0);
        assert_eq!(identity.exchanges.load(Ordering::SeqCst), 0);
    }
    let malformed = oidc_callback(&app, &state, Some(&cookie), "").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    binding_cleared(&malformed);
    assert_eq!(store.takes.load(Ordering::SeqCst), 1);
    assert_eq!(identity.exchanges.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn oidc_callback_cookie_security_uses_trusted_callback_not_forwarded_headers() {
    for (uri, name, secure, forwarded) in [
        (
            "http://localhost:8080/callback",
            "aster_oidc_state",
            false,
            "https",
        ),
        (
            "https://aster.example.invalid/callback",
            "__Host-aster_oidc_state",
            true,
            "http",
        ),
    ] {
        let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
        state.identity = Some(Arc::new(TrustedIdentity {
            callback_uri: Some(uri),
            ..Default::default()
        }));
        let app = app(Arc::new(state));
        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/login")
                    .header("x-forwarded-proto", forwarded)
                    .header("forwarded", format!("proto={forwarded}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let set = login.headers()["set-cookie"].to_str().unwrap();
        assert!(set.starts_with(&format!("{name}=")));
        assert_eq!(set.contains("; Secure"), secure);
        assert!(set.contains("Path=/; HttpOnly; SameSite=Lax"));
        assert!(set.ends_with("Max-Age=300"));
        let cookie = set.split(';').next().unwrap();
        let returned = cookie.split_once('=').unwrap().1;
        let complete = oidc_callback(&app, returned, Some(cookie), "code=valid").await;
        assert_eq!(complete.status(), StatusCode::SEE_OTHER);
        let clear = complete
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap())
            .find(|v| v.starts_with(&format!("{name}=")))
            .unwrap();
        assert_eq!(clear.contains("; Secure"), secure);
        assert!(clear.ends_with("Max-Age=0"));
        let session = complete
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap())
            .find(|v| v.starts_with("aster_session="))
            .unwrap();
        assert_eq!(session.contains("; Secure"), secure);
    }
}

#[tokio::test]
async fn callback_keeps_trusted_groups_and_ignores_forged_request_claims() {
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    state.identity = Some(Arc::new(TrustedIdentity::default()));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let sessions = Arc::clone(&state.sessions);
    let app = app(Arc::new(state));
    let (challenge, headers) = start_oidc(&app).await;
    let browser_cookie = binding(&headers);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/callback?code=valid&state={challenge}"))
                .header("cookie", browser_cookie)
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
    let app = notebook_helper_app().await;
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
        ("bob", "bob-sales", "beta"),
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
        ("bob", "bob-sales", "beta"),
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
    let app = notebook_helper_app().await;
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
    let app = notebook_helper_app().await;
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

#[tokio::test]
async fn personal_registration_without_admin_grant_is_refused() {
    let response = test_app().oneshot(Request::builder()
        .method("PUT").uri("/api/llm/private")
        .header("x-aster-subject", "not-granted")
        .header("x-aster-roles", "editor")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"base_url":"https://models.example.invalid","model":"sql","api_key":"disposable"}"#)).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn personal_capability_revocation_blocks_saved_use_but_allows_owner_cleanup() {
    let (url, calls, task) = fake_model("SELECT 1").await;
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    state.personal_model_policy = Arc::new(InMemoryPersonalModelPolicy::default());
    state
        .notebook_owners
        .change(NotebookOwnerChange {
            source: state.notebooks.source_key(),
            id: "sales".into(),
            expected_owner: None,
            owner: "alice".into(),
            source_blob: "fixture".into(),
            actor: "alice".into(),
            reason: None,
        })
        .await
        .unwrap();
    let app = app(Arc::new(state));
    let grant = json!({"expectedRevision":"0","subjects":["alice"],"teams":[]});
    let (status, _, _) = call(
        &app,
        "POST",
        "/aster.v1.Aster/PutPersonalModelPolicy",
        Some("alice"),
        "editor",
        Some(grant.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body, _) = call(
        &app,
        "POST",
        "/aster.v1.Aster/PutPersonalModelPolicy",
        Some("admin"),
        "admin",
        Some(grant),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revision"], "1");
    let registration = json!({"id":"private","baseUrl":url,"model":"qwen","token":"fixture-token"});
    let (status, body, _) = call(
        &app,
        "POST",
        "/aster.v1.Aster/PutPersonalModel",
        Some("alice"),
        "editor",
        Some(registration.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ref"], "personal/private");
    assert!(!body.to_string().contains("fixture-token"));
    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/notebooks/sales/helper",
        Some("alice"),
        "editor",
        Some(json!({"helper":"personal/private"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = call(
        &app,
        "POST",
        "/aster.v1.Aster/PutPersonalModelPolicy",
        Some("admin"),
        "admin",
        Some(json!({"expectedRevision":"1","subjects":[],"teams":[]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for (path, body) in [
        ("/aster.v1.Aster/PutPersonalModel", registration),
        (
            "/api/ai",
            json!({"prompt":"select one","helper":"personal/private"}),
        ),
    ] {
        let (status, _, _) = call(&app, "POST", path, Some("alice"), "editor", Some(body)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/notebooks/sales/helper",
        Some("alice"),
        "editor",
        Some(json!({"helper":"personal/private"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(calls.lock().unwrap().is_empty());
    let (status, _, _) = call(
        &app,
        "POST",
        "/aster.v1.Aster/DeletePersonalModel",
        Some("alice"),
        "editor",
        Some(json!({"id":"private"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    task.abort();
}

#[tokio::test]
async fn personal_models_require_approved_destinations_and_enabled_credentials() {
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    state.personal_model_destinations =
        Some(aster_server::DestinationPolicy::parse("https://models.example.invalid").unwrap());
    let app = app(Arc::new(state));
    for url in [
        "http://models.example.invalid",
        "https://unapproved.invalid",
    ] {
        let (status, _, _) = call(
            &app,
            "PUT",
            "/api/llm/private",
            Some("alice"),
            "editor",
            Some(json!({"base_url":url,"model":"qwen","api_key":"fixture-token"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    state.personal_models_enabled = false;
    let app = aster_server::app(Arc::new(state));
    let (status, _, _) = call(
        &app,
        "PUT",
        "/api/llm/private",
        Some("alice"),
        "editor",
        Some(json!({"base_url":"https://models.example.invalid","model":"qwen"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

struct PersonalAuthority(Mutex<CurrentIdentity>);
#[async_trait::async_trait]
impl CurrentIdentityProvider for PersonalAuthority {
    async fn current(&self, _: &str) -> Result<CurrentIdentity> {
        Ok(self.0.lock().unwrap().clone())
    }
    async fn group_exists(&self, _: &str) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn personal_team_permission_refreshes_membership_without_shared_registry() {
    let directory = tempfile::tempdir().unwrap();
    let authority = Arc::new(PersonalAuthority(Mutex::new(CurrentIdentity {
        user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
        active: true,
        roles: vec![Role::Editor],
        groups: vec!["analysts".into()],
    })));
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    state.current_identity = Some(authority.clone());
    state.personal_model_destinations =
        Some(aster_server::DestinationPolicy::parse("https://models.example.invalid").unwrap());
    state.personal_model_policy = Arc::new(InMemoryPersonalModelPolicy::default());
    state
        .personal_model_policy
        .replace(0, vec![], vec!["data".into()], "admin")
        .await
        .unwrap();
    state.team_workspaces = Some(Arc::new(
        aster_server::TeamWorkspaces::new(
            directory.path(),
            std::collections::HashMap::from([(
                "data".into(),
                aster_server::TeamPolicy {
                    member_claim: "analysts".into(),
                    maintainer_claim: "maintainers".into(),
                    allowed_repositories: std::collections::HashSet::from(["org/repo".into()]),
                    local_repositories: Default::default(),
                },
            )]),
        )
        .unwrap(),
    ));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let session = state
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec!["analysts".into()],
                user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let app = app(Arc::new(state));
    for expected in [StatusCode::OK, StatusCode::FORBIDDEN] {
        let response=app.clone().oneshot(Request::builder().method("PUT").uri("/api/llm/private")
            .header("cookie",format!("aster_session={}",session.sid)).header("content-type","application/json")
            .header("x-aster-groups","analysts")
            .body(Body::from(json!({"base_url":"https://models.example.invalid","model":"qwen","api_key":"fixture-token"}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), expected);
        authority.0.lock().unwrap().groups.clear();
    }
}

#[tokio::test]
async fn personal_generation_rechecks_current_role_after_session_downgrade() {
    let authority = Arc::new(PersonalAuthority(Mutex::new(CurrentIdentity {
        user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
        active: true,
        roles: vec![Role::Viewer],
        groups: vec![],
    })));
    let llm = Arc::new(InMemoryLlm::new());
    let (url, calls, task) = fake_model("SELECT 1").await;
    llm.put(LlmConfig {
        subject: "alice".into(),
        id: "private".into(),
        base_url: url,
        model: "qwen".into(),
        api_key: "fixture-token".into(),
    })
    .await
    .unwrap();
    let mut state = test_state_with_llm(llm);
    state.current_identity = Some(authority);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let session = state
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec![],
                user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let app = app(Arc::new(state));
    for (method, path, body) in [
        (
            "POST",
            "/api/ai",
            json!({"prompt":"select one","helper":"personal/private"}),
        ),
        (
            "PUT",
            "/api/llm/private",
            json!({"base_url":"https://models.example.invalid","model":"qwen"}),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("cookie", format!("aster_session={}", session.sid))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
    }
    assert!(calls.lock().unwrap().is_empty());
    task.abort();
}

struct NoPersonalGitCalls;
#[async_trait::async_trait]
impl aster_server::TeamGitVerifier for NoPersonalGitCalls {
    async fn verified_default_commit(&self, _: &str, _: u64, _: u64, _: &str) -> Result<String> {
        panic!("personal capability checks must not call Git")
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn personal_team_capability_does_not_override_git_policy_denial() {
    let database = std::env::var("ASTER_TEST_PERSONAL_MODEL_URL").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let user = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let local_group = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let git_group = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    let maintainer = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    let mut state = test_state_with_llm(Arc::new(InMemoryLlm::new()));
    state.current_identity = Some(Arc::new(PersonalAuthority(Mutex::new(CurrentIdentity {
        user_uuid: user.into(),
        active: true,
        roles: vec![Role::Editor],
        groups: vec![local_group.into()],
    }))));
    state.personal_model_destinations =
        Some(aster_server::DestinationPolicy::parse("https://models.example.invalid").unwrap());
    state.personal_model_policy = Arc::new(InMemoryPersonalModelPolicy::default());
    state
        .personal_model_policy
        .replace(0, vec![], vec!["data".into()], "admin")
        .await
        .unwrap();
    state.team_workspaces = Some(Arc::new(
        aster_server::TeamWorkspaces::new(
            directory.path(),
            std::collections::HashMap::from([(
                "data".into(),
                aster_server::TeamPolicy {
                    member_claim: local_group.into(),
                    maintainer_claim: maintainer.into(),
                    allowed_repositories: std::collections::HashSet::from(["org/repo".into()]),
                    local_repositories: Default::default(),
                },
            )]),
        )
        .unwrap(),
    ));
    state.team_git_targets = Some(Arc::new(
        aster_server::TeamGitTargets::connect(
            &database,
            std::collections::HashMap::from([(
                "data".into(),
                aster_server::TeamGitPolicy {
                    member_group_uuid: git_group.into(),
                    maintainer_group_uuid: maintainer.into(),
                    installation_id: 1,
                    allowed_repositories: std::collections::HashMap::from([("org/repo".into(), 1)]),
                },
            )]),
            Arc::new(NoPersonalGitCalls),
        )
        .await
        .unwrap(),
    ));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let session = state
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec![local_group.into()],
                user_uuid: Some(user.into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let app = app(Arc::new(state));
    let response = app.oneshot(Request::builder().method("PUT").uri("/api/llm/private")
        .header("cookie", format!("aster_session={}", session.sid)).header("content-type", "application/json")
        .body(Body::from(json!({"base_url":"https://models.example.invalid","model":"qwen","api_key":"fixture-token"}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
