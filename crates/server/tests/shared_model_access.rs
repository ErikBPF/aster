//! V8b2 assertion RED: admin grants and scoped listing must exist before
//! current-membership checks can authorize ordinary shared-model use.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aster_core::*;
use aster_server::{app, AppState, DestinationPolicy, Keyring, Metrics, SharedModels};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use base64::Engine;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;

struct NoNotebooks;

#[derive(Default)]
struct CountingConversations {
    rows: InMemoryConversations,
    reads: AtomicUsize,
}

#[async_trait::async_trait]
impl ConversationStore for CountingConversations {
    async fn get(&self, subject: &str, notebook: &str) -> Result<Conversation> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.rows.get(subject, notebook).await
    }

    async fn append(
        &self,
        subject: &str,
        notebook: &str,
        expected: i64,
        user: ChatMessage,
        assistant: ChatMessage,
    ) -> Result<Conversation> {
        self.rows
            .append(subject, notebook, expected, user, assistant)
            .await
    }
}

struct FakeCurrentIdentity {
    value: Mutex<Option<CurrentIdentity>>,
}

impl FakeCurrentIdentity {
    fn set(&self, value: Option<CurrentIdentity>) {
        *self.value.lock().unwrap() = value;
    }
}

#[async_trait::async_trait]
impl CurrentIdentityProvider for FakeCurrentIdentity {
    async fn current(&self, _: &str) -> Result<CurrentIdentity> {
        self.value
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| CoreError::Storage("fake authority unavailable".into()))
    }

    async fn group_exists(&self, group_uuid: &str) -> Result<bool> {
        Ok(group_uuid == "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
    }
}

#[async_trait::async_trait]
impl NotebookStore for NoNotebooks {
    async fn get(&self, id: &str) -> Result<Notebook> {
        if id != "report" {
            return Err(CoreError::NotFound("no notebooks".into()));
        }
        Ok(Notebook {
            id: id.into(),
            title: "Report".into(),
            cells: vec![],
        })
    }

    async fn save(&self, _: &Notebook, _: &str) -> Result<String> {
        Err(CoreError::NotFound("no notebooks".into()))
    }

    async fn list(&self, _: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }

    async fn snapshot(&self, id: &str) -> Result<NotebookSnapshot> {
        Ok(NotebookSnapshot {
            notebook: self.get(id).await?,
            content_revision: "1".repeat(40),
        })
    }
}

async fn fixture() -> (Router, Arc<InMemorySessions>) {
    fixture_with_authority(None).await
}

async fn fixture_with_authority(
    authority: Option<Arc<dyn CurrentIdentityProvider>>,
) -> (Router, Arc<InMemorySessions>) {
    fixture_with_endpoint(
        authority,
        "https://approved.example.invalid",
        reqwest::Client::new(),
    )
    .await
}

async fn fixture_with_endpoint(
    authority: Option<Arc<dyn CurrentIdentityProvider>>,
    approved_origin: &str,
    http: reqwest::Client,
) -> (Router, Arc<InMemorySessions>) {
    fixture_with_conversations(
        authority,
        approved_origin,
        http,
        Arc::new(InMemoryConversations::default()),
    )
    .await
}

async fn fixture_with_conversations(
    authority: Option<Arc<dyn CurrentIdentityProvider>>,
    approved_origin: &str,
    http: reqwest::Client,
    conversations: Arc<dyn ConversationStore>,
) -> (Router, Arc<InMemorySessions>) {
    let use_enabled = authority.is_some();
    let sessions = Arc::new(InMemorySessions::new(3600));
    let keyset = json!({"v1": base64::engine::general_purpose::STANDARD.encode([7u8; 32])});
    let shared_models = SharedModels::new(
        Arc::new(InMemorySharedModels::default()),
        DestinationPolicy::parse(approved_origin).unwrap(),
        Keyring::parse("v1", &keyset.to_string()).unwrap(),
    );
    let notebooks = Arc::new(NoNotebooks);
    let notebook_owners = Arc::new(InMemoryNotebookOwners::default());
    notebook_owners
        .change(NotebookOwnerChange {
            source: notebooks.source_key(),
            id: "report".into(),
            expected_owner: None,
            owner: "alice".into(),
            source_blob: notebooks.snapshot("report").await.unwrap().content_revision,
            actor: "alice".into(),
            reason: Some("shared-model fixture notebook".into()),
        })
        .await
        .unwrap();
    let state = AppState {
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
        notebooks,
        notebook_owners,
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: Some(Arc::new(shared_models)),
        current_identity: authority,
        shared_model_use_enabled: use_enabled,
        team_workspaces: None,
        team_git_targets: None,
        conversations,
        exchanges: Arc::new(InMemoryExchanges::default()),
        compiled_contracts: None,
        contracts: Arc::new(vec![]),
        http,
        sessions: sessions.clone(),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    };
    (app(Arc::new(state)), sessions)
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    } else {
        builder = builder
            .header("x-aster-subject", "root")
            .header("x-aster-roles", "admin");
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn conversation(app: &Router, cookie: &str) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/aster.v1.Aster/GetConversation")
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .header("cookie", cookie)
                .body(Body::from(json!({"notebook":"report"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn https_model_fixture() -> (
    String,
    Arc<Mutex<Vec<(String, String)>>>,
    Arc<AtomicUsize>,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
) {
    let temporary = tempfile::tempdir().unwrap();
    let cert = temporary.path().join("cert.pem");
    let key = temporary.path().join("key.pem");
    let cert_der = temporary.path().join("cert.der");
    let key_der = temporary.path().join("key.der");
    let quiet = || std::process::Stdio::null();
    assert!(std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=IP:127.0.0.1",
            "-keyout"
        ])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .stdout(quiet())
        .stderr(quiet())
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("openssl")
        .args(["x509", "-in"])
        .arg(&cert)
        .args(["-outform", "DER", "-out"])
        .arg(&cert_der)
        .stdout(quiet())
        .stderr(quiet())
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("openssl")
        .args(["pkcs8", "-topk8", "-nocrypt", "-in"])
        .arg(&key)
        .args(["-outform", "DER", "-out"])
        .arg(&key_der)
        .stdout(quiet())
        .stderr(quiet())
        .status()
        .unwrap()
        .success());
    let tls = ServerConfig::builder_with_provider(Arc::new(
        tokio_rustls::rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from(std::fs::read(cert_der).unwrap())],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(std::fs::read(key_der).unwrap())),
    )
    .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("https://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let redirected = Arc::new(AtomicUsize::new(0));
    let captured = seen.clone();
    let stolen = redirected.clone();
    let redirect_url = format!("{origin}/steal");
    let task = tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let mut stream = acceptor.accept(socket).await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let (head_end, length) = loop {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0 && bytes.len() + count < 65_536);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    let head = std::str::from_utf8(&bytes[..end]).unwrap();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                                .and_then(|(_, size)| size.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break (end, length);
                    }
                }
            };
            let head = std::str::from_utf8(&bytes[..head_end]).unwrap();
            let path = head
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let authorization = head
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                        .map(|(_, value)| value.trim())
                })
                .unwrap_or("")
                .to_string();
            let body: Value = serde_json::from_slice(&bytes[head_end + 4..head_end + 4 + length])
                .unwrap_or(Value::Null);
            let response = if path == "/steal" {
                stolen.fetch_add(1, Ordering::SeqCst);
                "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
            } else if path == "/bounce/v1/chat/completions" {
                format!("HTTP/1.1 302 Found\r\nLocation: {redirect_url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            } else {
                captured.lock().unwrap().push((
                    authorization,
                    body["model"].as_str().unwrap_or("").to_string(),
                ));
                let payload = r#"{"choices":[{"message":{"content":"SELECT 77"}}]}"#;
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len())
            };
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        }
    });
    (origin, seen, redirected, task, temporary)
}

#[tokio::test]
async fn grant_and_scoped_list_keep_shared_qwen_distinct_from_personal_qwen() {
    let authority = Arc::new(FakeCurrentIdentity {
        value: Mutex::new(Some(CurrentIdentity {
            user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            active: true,
            roles: vec![Role::Editor],
            groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
        })),
    });
    let (app, sessions) = fixture_with_authority(Some(authority.clone())).await;
    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen",
        None,
        json!({"base_url":"https://approved.example.invalid/v1", "model":"shared-qwen", "api_key":"shared-secret"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let alice = sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
                user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let cookie = format!("aster_session={}", alice.sid);
    let (status, _) = request(
        &app,
        "PUT",
        "/api/llm/qwen",
        Some(&cookie),
        json!({"base_url":"http://personal.example.invalid/v1", "model":"personal-qwen", "api_key":"personal-secret"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen/grants",
        None,
        json!({"expected_revision":0,"grants":[{"group":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}]}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "admin grants must be durable before shared use"
    );

    let (status, list) = request(&app, "GET", "/api/llm", Some(&cookie), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .any(|item| { item["ref"] == "personal/qwen" && item["model"] == "personal-qwen" }));
    assert!(list
        .as_array()
        .unwrap()
        .iter()
        .any(|item| { item["ref"] == "shared/qwen" && item["model"] == "shared-qwen" }));
    assert!(!list.to_string().contains("shared-secret"));
    assert!(!list.to_string().contains("approved.example.invalid"));

    let (status, choice) = request(
        &app,
        "PUT",
        "/api/notebooks/report/helper",
        Some(&cookie),
        json!({"helper":"shared/qwen"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "current grantee can select shared/qwen"
    );
    assert_eq!(choice["helper"], "shared/qwen");
    let (status, saved) = request(
        &app,
        "GET",
        "/api/notebooks/report/helper",
        Some(&cookie),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["helper"], "shared/qwen");

    // A current group grant does not keep RunQuery after the IdP removes the
    // editor role; the session still says Editor throughout this check.
    authority.set(Some(CurrentIdentity {
        user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
        active: true,
        roles: vec![Role::Viewer],
        groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
    }));
    let (status, downgraded) = request(&app, "GET", "/api/llm", Some(&cookie), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!downgraded.to_string().contains("shared/qwen"));
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/ai",
            Some(&cookie),
            json!({"helper":"shared/qwen","prompt":"write one query"}),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    authority.set(Some(CurrentIdentity {
        user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
        active: true,
        roles: vec![Role::Viewer],
        groups: vec![],
    }));
    let (status, revoked) = request(&app, "GET", "/api/llm", Some(&cookie), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!revoked.to_string().contains("shared/qwen"));
    assert!(revoked.to_string().contains("personal/qwen"));
    let (status, _) = request(
        &app,
        "PUT",
        "/api/notebooks/report/helper",
        Some(&cookie),
        json!({"helper":"shared/qwen"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "revoked selection is denied");
    let (status, _) = request(
        &app,
        "POST",
        "/api/ai",
        Some(&cookie),
        json!({"helper":"shared/qwen","prompt":"write one query"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "revoked generation is denied"
    );
    let before = conversation(&app, &cookie).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/aster.v1.Aster/SendMessage")
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({"notebook":"report","helper":"shared/qwen","prompt":"explain","expectedRevision":"0"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "revoked conversation is denied"
    );
    assert_eq!(conversation(&app, &cookie).await, before);
}

#[tokio::test]
async fn grant_replacement_conflicts_on_stale_revision_and_keeps_audit_history() {
    let (app, sessions) = fixture().await;
    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen",
        None,
        json!({"base_url":"https://approved.example.invalid/v1", "model":"shared-qwen", "api_key":"shared-secret"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let editor = sessions
        .create("alice", vec![Role::Editor], vec![], None, now)
        .await
        .unwrap();
    let editor_cookie = format!("aster_session={}", editor.sid);
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen/grants",
            Some(&editor_cookie),
            json!({"expected_revision":0,"grants":[{"role":"admin"}]}),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen/grants",
            None,
            json!({"expected_revision":0,"grants":[{"group":""}]}),
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen/grants",
            None,
            json!({"expected_revision":0,"grants":[{"group":"analysts"}]}),
        )
        .await
        .0,
        StatusCode::BAD_REQUEST,
        "shared group grants require stable provider UUIDs"
    );

    let initial =
        json!({"expected_revision":0,"grants":[{"group":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}]});
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen/grants",
            None,
            initial,
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let stale = json!({"expected_revision":0,"grants":[{"role":"admin"}]});
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen/grants",
            None,
            stale,
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    let (status, current) = request(
        &app,
        "GET",
        "/api/admin/shared-models/qwen/grants",
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["revision"], 1);
    assert_eq!(
        current["grants"],
        json!([{"group":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}])
    );

    let (status, events) = request(
        &app,
        "GET",
        "/api/admin/shared-models/qwen/grants/events",
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events.as_array().unwrap().len(), 1);
    assert_eq!(events[0]["actor"], "root");
    assert_eq!(events[0]["revision"], 1);

    let fresh = json!({"expected_revision":1,"grants":[{"role":"editor"}]});
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen/grants",
            None,
            fresh,
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (status, current) = request(
        &app,
        "GET",
        "/api/admin/shared-models/qwen/grants",
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["revision"], 2);
    assert_eq!(current["grants"], json!([{"role":"editor"}]));
    let (status, events) = request(
        &app,
        "GET",
        "/api/admin/shared-models/qwen/grants/events",
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events.as_array().unwrap().len(), 2);
    assert_eq!(events[1]["revision"], 2);
}

#[tokio::test]
async fn grant_replacement_rejects_group_absent_from_current_provider() {
    let authority = Arc::new(FakeCurrentIdentity {
        value: Mutex::new(Some(CurrentIdentity {
            user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            active: true,
            roles: vec![Role::Admin],
            groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
        })),
    });
    let (app, _) = fixture_with_authority(Some(authority)).await;
    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen",
        None,
        json!({"base_url":"https://approved.example.invalid/v1", "model":"shared-qwen", "api_key":"shared-secret"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen/grants",
        None,
        json!({"expected_revision":0,"grants":[{"group":"dddddddd-dddd-4ddd-8ddd-dddddddddddd"}]}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "unknown group cannot receive a grant"
    );
    let (status, grants) = request(
        &app,
        "GET",
        "/api/admin/shared-models/qwen/grants",
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(grants["revision"], 0);
    assert_eq!(grants["grants"], json!([]));
}

#[tokio::test]
async fn approved_https_routes_exact_scoped_credential_and_never_follows_redirect() {
    let (origin, seen, redirected, upstream, _temporary) = https_model_fixture().await;
    let authority = Arc::new(FakeCurrentIdentity {
        value: Mutex::new(Some(CurrentIdentity {
            user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            active: true,
            roles: vec![Role::Editor],
            groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
        })),
    });
    let http = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (app, sessions) = fixture_with_endpoint(Some(authority.clone()), &origin, http).await;
    for (id, path) in [("qwen", "/v1"), ("bounce", "/bounce")] {
        assert_eq!(
            request(
                &app,
                "PUT",
                &format!("/api/admin/shared-models/{id}"),
                None,
                json!({"base_url":format!("{origin}{path}"),"model":"shared-qwen","api_key":"shared-secret"}),
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                &app,
                "PUT",
                &format!("/api/admin/shared-models/{id}/grants"),
                None,
                json!({"expected_revision":0,"grants":[{"group":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}]}),
            )
            .await
            .0,
            StatusCode::NO_CONTENT
        );
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let alice = sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
                user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let cookie = format!("aster_session={}", alice.sid);
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/llm/qwen",
            Some(&cookie),
            json!({"base_url":format!("{origin}/v1"),"model":"personal-qwen","api_key":"personal-secret"}),
        )
        .await
        .0,
        StatusCode::OK
    );
    for (reference, expected_model) in [
        ("personal/qwen", "personal-qwen"),
        ("shared/qwen", "shared-qwen"),
    ] {
        let (status, completion) = request(
            &app,
            "POST",
            "/api/ai",
            Some(&cookie),
            json!({"helper":reference,"prompt":"write a query"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{completion}");
        assert_eq!(completion["sql"], "SELECT 77");
        assert_eq!(seen.lock().unwrap().last().unwrap().1, expected_model);
    }
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        &[
            ("Bearer personal-secret".into(), "personal-qwen".into()),
            ("Bearer shared-secret".into(), "shared-qwen".into()),
        ]
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/aster.v1.Aster/SendMessage")
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({"notebook":"report","helper":"shared/qwen","prompt":"explain","expectedRevision":"0"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &("Bearer shared-secret".into(), "shared-qwen".into())
    );
    let saved_conversation = conversation(&app, &cookie).await;
    assert_eq!(saved_conversation["messages"].as_array().unwrap().len(), 2);
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/ai",
            Some(&cookie),
            json!({"helper":"shared/bounce","prompt":"write a query"}),
        )
        .await
        .0,
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(redirected.load(Ordering::SeqCst), 0);
    authority.set(Some(CurrentIdentity {
        user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
        active: true,
        roles: vec![Role::Editor],
        groups: vec![],
    }));
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/ai",
            Some(&cookie),
            json!({"helper":"shared/qwen","prompt":"write a query"}),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/aster.v1.Aster/SendMessage")
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({"notebook":"report","helper":"shared/qwen","prompt":"another","expectedRevision":"1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(conversation(&app, &cookie).await, saved_conversation);
    assert_eq!(seen.lock().unwrap().len(), 3);
    upstream.abort();
}

#[tokio::test]
async fn revoked_first_send_does_not_create_conversation_or_call_upstream() {
    let (origin, seen, _redirected, upstream, _temporary) = https_model_fixture().await;
    let authority = Arc::new(FakeCurrentIdentity {
        value: Mutex::new(Some(CurrentIdentity {
            user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            active: true,
            roles: vec![Role::Editor],
            groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
        })),
    });
    let conversations = Arc::new(CountingConversations::default());
    let http = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (app, sessions) =
        fixture_with_conversations(Some(authority), &origin, http, conversations.clone()).await;
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/admin/shared-models/qwen",
            None,
            json!({"base_url":format!("{origin}/v1"),"model":"shared-qwen","api_key":"shared-secret"}),
        )
        .await
        .0,
        StatusCode::OK
    );
    let grants = "/api/admin/shared-models/qwen/grants";
    assert_eq!(
        request(&app, "PUT", grants, None, json!({"expected_revision":0,"grants":[{"group":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}]})).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        request(
            &app,
            "PUT",
            grants,
            None,
            json!({"expected_revision":1,"grants":[]})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let alice = sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
                user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/aster.v1.Aster/SendMessage")
                .header("content-type", "application/json")
                .header("connect-protocol-version", "1")
                .header("cookie", format!("aster_session={}", alice.sid))
                .body(Body::from(
                    json!({"notebook":"report","helper":"shared/qwen","prompt":"explain","expectedRevision":"0"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(conversations.reads.load(Ordering::SeqCst), 0);
    assert!(seen.lock().unwrap().is_empty());
    upstream.abort();
}

#[tokio::test]
async fn cached_admin_session_without_current_authority_cannot_manage_grants() {
    let (app, sessions) = fixture().await;
    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen",
        None,
        json!({"base_url":"https://approved.example.invalid/v1", "model":"shared-qwen", "api_key":"shared-secret"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let stale = sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Admin],
                groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
                user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
            },
            None,
            now,
        )
        .await
        .unwrap();
    let cookie = format!("aster_session={}", stale.sid);
    let (status, _) = request(
        &app,
        "PUT",
        "/api/admin/shared-models/qwen/grants",
        Some(&cookie),
        json!({"expected_revision":0,"grants":[{"role":"editor"}]}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, grants) = request(
        &app,
        "GET",
        "/api/admin/shared-models/qwen/grants",
        None,
        json!({}),
    )
    .await;
    assert_eq!(grants["revision"], 0);
    let (_, events) = request(
        &app,
        "GET",
        "/api/admin/shared-models/qwen/grants/events",
        None,
        json!({}),
    )
    .await;
    assert_eq!(events, json!([]));
}
