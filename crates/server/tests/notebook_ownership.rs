use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aster_core::*;
use aster_server::{app, AppState, GitNotebookStore, Metrics};
use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

struct Fixture {
    app: Router,
    dir: PathBuf,
    _store: GitNotebookStore,
}

fn notebook(id: &str, sql: &str) -> Notebook {
    Notebook {
        id: id.into(),
        title: "Sales".into(),
        cells: vec![Cell {
            id: "c1".into(),
            sql: sql.into(),
            engine: None,
        }],
    }
}

async fn fixture() -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "aster-ownership-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let store = GitNotebookStore::open(&dir, "session/alice").unwrap();
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
        notebooks: Arc::new(store.clone()),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        team_git_targets: None,
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: None,
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
    };
    Fixture {
        app: app(Arc::new(state)),
        dir,
        _store: store,
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    subject: &str,
    role: &str,
    precondition: Option<(&str, String)>,
    body: serde_json::Value,
) -> Response<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("x-aster-subject", subject)
        .header("x-aster-roles", role)
        .header("content-type", "application/json");
    if path.starts_with("/aster.v1.Aster/") {
        request = request.header("connect-protocol-version", "1");
    }
    if let Some((name, value)) = precondition {
        request = request.header(name, value);
    }
    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn legacy_unassigned_save_is_refused_before_git_mutation() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let before = git(&fixture.dir, &["rev-parse", "HEAD"]);
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);

    let response = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-match", format!("\"{blob}\""))),
        json!(notebook("sales", "SELECT 2")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(git(&fixture.dir, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]), blob);
}

#[tokio::test]
async fn create_requires_an_explicit_absence_precondition() {
    let fixture = fixture().await;
    let response = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        None,
        json!(notebook("sales", "SELECT 1")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(fixture._store.list("alice").await.unwrap().is_empty());
}

#[tokio::test]
async fn legacy_get_exposes_the_committed_blob_oid() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let response = call(
        &fixture.app,
        "GET",
        "/api/notebooks/sales",
        "alice",
        "editor",
        None,
        json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("etag")
            .map(|value| value.to_str().unwrap()),
        Some(format!("\"{blob}\"").as_str())
    );

    let page = call(
        &fixture.app,
        "GET",
        "/notebooks/sales",
        "alice",
        "editor",
        None,
        json!({}),
    )
    .await;
    let body = axum::body::to_bytes(page.into_body(), 1 << 20)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains(&blob));
}

#[tokio::test]
async fn rpc_create_without_precondition_is_refused() {
    let fixture = fixture().await;
    let response = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/SaveNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"sales","notebook":notebook("sales", "SELECT 1")}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(fixture._store.list("alice").await.unwrap().is_empty());
}

#[tokio::test]
async fn rpc_create_get_and_stale_update_use_content_blob_revision() {
    let fixture = fixture().await;
    let create = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/SaveNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"sales", "ifAbsent":true, "notebook":notebook("sales", "SELECT 1")}),
    )
    .await;
    assert_eq!(create.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(create.into_body(), 1 << 20)
        .await
        .unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let oid = saved["contentRevision"].as_str().unwrap();
    assert_eq!(oid, git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]));
    assert_eq!(
        saved["revision"].as_str().unwrap(),
        git(&fixture.dir, &["rev-parse", "HEAD"])
    );
    let read = call(
        &fixture.app,
        "POST",
        "/aster.v1.Aster/GetNotebook",
        "alice",
        "editor",
        None,
        json!({"id":"sales"}),
    )
    .await;
    let bytes = axum::body::to_bytes(read.into_body(), 1 << 20)
        .await
        .unwrap();
    let loaded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(loaded["contentRevision"], oid);
    let first = call(&fixture.app, "POST", "/aster.v1.Aster/SaveNotebook", "alice", "editor", None,
        json!({"id":"sales", "expectedContentRevision":oid, "notebook":notebook("sales", "SELECT 2")})).await;
    assert_eq!(first.status(), StatusCode::OK);
    let stale = call(&fixture.app, "POST", "/aster.v1.Aster/SaveNotebook", "alice", "editor", None,
        json!({"id":"sales", "expectedContentRevision":oid, "notebook":notebook("sales", "SELECT 3")})).await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn rpc_assignment_requires_admin_and_valid_blob_precondition() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let oid = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let path = "/aster.v1.Aster/AssignNotebookOwner";
    let body = json!({"id":"sales", "owner":"alice", "expectedContentRevision":oid});
    let denied = call(
        &fixture.app,
        "POST",
        path,
        "bob",
        "editor",
        None,
        body.clone(),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let granted = call(&fixture.app, "POST", path, "root", "admin", None, body).await;
    assert_eq!(granted.status(), StatusCode::OK);
    let invalid = call(&fixture.app, "POST", "/aster.v1.Aster/SaveNotebook", "alice", "editor", None,
        json!({"id":"sales", "expectedContentRevision":"bad", "notebook":notebook("sales", "SELECT 2")})).await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_assignment_requires_a_committed_blob_and_admin_role() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let body = json!({"owner":"alice", "expected_content_revision":blob});
    let path = "/api/admin/notebooks/sales/owner";
    let ordinary = call(
        &fixture.app,
        "PUT",
        path,
        "bob",
        "editor",
        None,
        body.clone(),
    )
    .await;
    assert_eq!(ordinary.status(), StatusCode::FORBIDDEN);
    let admin = call(&fixture.app, "PUT", path, "root", "admin", None, body).await;
    assert_eq!(admin.status(), StatusCode::OK);
}

#[tokio::test]
async fn stale_tab_conflicts_but_an_unrelated_commit_does_not() {
    let fixture = fixture().await;
    let first = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 1")),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let old = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let unrelated = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/other",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("other", "SELECT 9")),
    )
    .await;
    assert_eq!(unrelated.status(), StatusCode::OK);
    let update = |oid: &str, sql: &str| {
        call(
            &fixture.app,
            "PUT",
            "/api/notebooks/sales",
            "alice",
            "editor",
            Some(("if-match", format!("\"{oid}\""))),
            json!(notebook("sales", sql)),
        )
    };
    let first = update(&old, "SELECT 2").await;
    assert_eq!(first.status(), StatusCode::OK);
    let current = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let before = git(&fixture.dir, &["rev-parse", "HEAD"]);
    let stale = update(&old, "SELECT 3").await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(git(&fixture.dir, &["rev-parse", "HEAD"]), before);
    assert_eq!(
        git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]),
        current
    );
}

#[tokio::test]
async fn correction_requires_exact_owner_blob_and_reason_then_revokes_old_owner() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let path = "/api/admin/notebooks/sales/owner";
    let assign = |body| call(&fixture.app, "PUT", path, "root", "admin", None, body);
    let stale = assign(json!({"owner":"alice", "expected_content_revision":"0".repeat(40)})).await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        assign(json!({"owner":"alice", "expected_content_revision":blob}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        assign(json!({"owner":"bob", "expected_content_revision":blob}))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        assign(json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":blob}))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        assign(json!({"owner":"bob", "expected_owner":"mallory", "expected_content_revision":blob, "reason":"correct typo"}))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(assign(json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":blob, "reason":"correct typo"})).await.status(), StatusCode::OK);
    let alice = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-match", format!("\"{blob}\""))),
        json!(notebook("sales", "SELECT 2")),
    )
    .await;
    assert_eq!(alice.status(), StatusCode::FORBIDDEN);
    let bob = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "bob",
        "editor",
        Some(("if-match", format!("\"{blob}\""))),
        json!(notebook("sales", "SELECT 2")),
    )
    .await;
    assert_eq!(bob.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_owner_inputs_are_bounded_and_exact() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let blob = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    for body in [
        json!({"owner":"a".repeat(257), "expected_content_revision":blob}),
        json!({"owner":" alice", "expected_content_revision":blob}),
        json!({"owner":"bob", "expected_owner":" alice", "expected_content_revision":blob, "reason":"fix"}),
        json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":blob, "reason":"x".repeat(1025)}),
    ] {
        let response = call(
            &fixture.app,
            "PUT",
            "/api/admin/notebooks/sales/owner",
            "root",
            "admin",
            None,
            body,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn concurrent_first_creates_claim_one_actor() {
    let fixture = fixture().await;
    let a = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "alice",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 1")),
    );
    let b = call(
        &fixture.app,
        "PUT",
        "/api/notebooks/sales",
        "bob",
        "editor",
        Some(("if-none-match", "*".into())),
        json!(notebook("sales", "SELECT 2")),
    );
    let (a, b) = tokio::join!(a, b);
    let statuses = [a.status(), b.status()];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::CONFLICT));
    assert_eq!(fixture._store.list("alice").await.unwrap(), vec!["sales"]);
}

#[tokio::test]
async fn moved_checkout_keeps_stable_owner_source_key() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "aster-source-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let old = dir.with_extension("old");
    let store = GitNotebookStore::open(&old, "session/alice").unwrap();
    let source = store.source_key();
    drop(store);
    std::fs::rename(&old, &dir).unwrap();
    let moved = GitNotebookStore::open(&dir, "session/alice").unwrap();
    assert_eq!(moved.source_key(), source);
}

#[tokio::test]
async fn corrupt_head_and_missing_blob_are_storage_errors() {
    let fixture = fixture().await;
    fixture
        ._store
        .save(&notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    let oid = git(&fixture.dir, &["rev-parse", "HEAD:sales.aster"]);
    let object = fixture
        .dir
        .join(".git/objects")
        .join(&oid[..2])
        .join(&oid[2..]);
    std::fs::remove_file(object).unwrap();
    assert!(matches!(
        fixture._store.get("sales").await,
        Err(CoreError::Storage(_))
    ));
    assert!(matches!(
        fixture
            ._store
            .save_if(
                &notebook("sales", "SELECT 2"),
                "seed",
                NotebookPrecondition::Blob(oid)
            )
            .await,
        Err(CoreError::Storage(_))
    ));
    let head = fixture.dir.join(".git/refs/heads/session/alice");
    std::fs::write(head, "broken\n").unwrap();
    assert!(matches!(
        fixture._store.list("alice").await,
        Err(CoreError::Storage(_))
    ));
    assert!(matches!(
        fixture
            ._store
            .save_if(
                &notebook("new", "SELECT 1"),
                "seed",
                NotebookPrecondition::Absent
            )
            .await,
        Err(CoreError::Storage(_))
    ));
}
