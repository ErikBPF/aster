//! V2b assertion RED: routes are intentionally absent until team policy and
//! request-scoped Git workspaces are wired. All identities come from sessions.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use aster_core::*;
use aster_server::{
    app, AppState, GitNotebookStore, Metrics, TeamGitPolicy, TeamGitTargets, TeamGitVerifier,
    TeamPolicy, TeamWorkspaces,
};
use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

struct Fixture {
    app: Router,
    dir: PathBuf,
    root: PathBuf,
    policies: HashMap<String, TeamPolicy>,
    workspaces: Arc<TeamWorkspaces>,
    sessions: Arc<InMemorySessions>,
    _legacy: GitNotebookStore,
}

async fn fixture() -> Fixture {
    fixture_with_current(None).await
}

async fn fixture_with_current(
    current_identity: Option<Arc<dyn CurrentIdentityProvider>>,
) -> Fixture {
    fixture_with_services(current_identity, None).await
}

async fn fixture_with_services(
    current_identity: Option<Arc<dyn CurrentIdentityProvider>>,
    team_git_targets: Option<Arc<TeamGitTargets>>,
) -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aster-team-red-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let dir = root.join("legacy");
    let legacy = GitNotebookStore::open(&dir, "legacy").unwrap();
    let alpha_repo = bare_repository(&root, "alpha", "main", "SELECT 101");
    let alpha_next_repo = bare_repository(&root, "alpha-next", "trunk", "SELECT 303");
    let beta_repo = bare_repository(&root, "beta", "stable", "SELECT 202");
    let policies = HashMap::from([
        (
            "alpha".to_owned(),
            TeamPolicy {
                member_claim: "/teams/alpha/members".into(),
                maintainer_claim: "/teams/alpha/maintainers".into(),
                allowed_repositories: HashSet::from([
                    "example/alpha".into(),
                    "example/alpha-next".into(),
                ]),
                local_repositories: HashMap::from([
                    ("example/alpha".into(), alpha_repo),
                    ("example/alpha-next".into(), alpha_next_repo),
                ]),
            },
        ),
        (
            "beta".to_owned(),
            TeamPolicy {
                member_claim: "/teams/beta/members".into(),
                maintainer_claim: "/teams/beta/maintainers".into(),
                allowed_repositories: HashSet::from(["example/beta".into()]),
                local_repositories: HashMap::from([("example/beta".into(), beta_repo)]),
            },
        ),
    ]);
    let workspaces =
        Arc::new(TeamWorkspaces::new(root.join("workspaces"), policies.clone()).unwrap());
    workspaces
        .configure("alpha", "example/alpha", "main", None, "fixture")
        .unwrap();
    workspaces
        .configure("beta", "example/beta", "stable", None, "fixture")
        .unwrap();
    let sessions = Arc::new(InMemorySessions::new(3600));
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
        notebooks: Arc::new(legacy.clone()),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: Some(workspaces.clone()),
        team_git_targets,
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: None,
        current_identity,
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        contracts: Arc::new(vec![]),
        http: reqwest::Client::new(),
        sessions: sessions.clone(),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: false,
        metrics: Arc::new(Metrics::new()),
    };
    Fixture {
        app: app(Arc::new(state)),
        dir,
        root,
        policies,
        workspaces,
        sessions,
        _legacy: legacy,
    }
}

fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn session_checkouts(root: &Path, team: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for subject in std::fs::read_dir(root.join("workspaces").join(team).join("sessions")).unwrap() {
        for session in std::fs::read_dir(subject.unwrap().path()).unwrap() {
            found.push(session.unwrap().path());
        }
    }
    found.sort();
    found
}

fn only_session_checkout(root: &Path, team: &str) -> PathBuf {
    let found = session_checkouts(root, team);
    assert_eq!(found.len(), 1);
    found[0].clone()
}

#[tokio::test]
async fn team_browser_notebook_uses_qualified_workspace_and_sync_action() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let beta_only = sid(&f.sessions, "bob", &["/teams/beta/members"]).await;
    let path = "/teams/alpha/notebooks/base?workspace=session";
    let response = call(&f.app, "GET", path, Some(&alice), json!({}), &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("id=\"nb\""));
    assert!(html.contains("data-team=\"alpha\""));
    assert!(html.contains("data-workspace=\"session\""));
    assert!(html.contains("data-action=\"sync\""));
    assert!(html.contains("href=\"/teams/alpha/notebooks/base?workspace=session\""));
    assert!(html.contains("href=\"/teams/alpha/notebooks/base?workspace=personal\""));
    let personal = call(
        &f.app,
        "GET",
        "/teams/alpha/notebooks/base?workspace=personal",
        Some(&alice),
        json!({}),
        &[],
    )
    .await;
    assert_eq!(personal.status(), StatusCode::OK);
    let personal_html = String::from_utf8(
        axum::body::to_bytes(personal.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(personal_html.contains("data-workspace=\"personal\""));
    assert_eq!(
        call(&f.app, "GET", path, Some(&beta_only), json!({}), &[])
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/notebooks/base",
            Some(&alice),
            json!({}),
            &[],
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

async fn notebook_sql(app: &Router, path: &str, sid: &str) -> String {
    let response = call(app, "GET", path, Some(sid), json!({}), &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    value["cells"][0]["sql"].as_str().unwrap().to_owned()
}

fn bare_repository(root: &Path, team: &str, branch: &str, base_sql: &str) -> PathBuf {
    let bare = root.join(format!("{team}.git"));
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    let seed = root.join(format!("{team}-seed"));
    git(root, &["init", "-q", seed.to_str().unwrap()]);
    git(&seed, &["config", "user.name", "fixture"]);
    git(&seed, &["config", "user.email", "fixture@example.invalid"]);
    std::fs::write(
        seed.join("base.aster"),
        format!("# aster notebook v2\n# title: Base\n-- cell c1\n{base_sql}\n"),
    )
    .unwrap();
    git(&seed, &["add", "base.aster"]);
    git(&seed, &["commit", "-qm", "base"]);
    git(&seed, &["branch", "-M", branch]);
    git(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&seed, &["push", "-q", "origin", branch]);
    bare
}

async fn sid(sessions: &InMemorySessions, subject: &str, groups: &[&str]) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    sessions
        .create(
            subject,
            vec![Role::Editor],
            groups.iter().map(|s| (*s).into()).collect(),
            None,
            now,
        )
        .await
        .unwrap()
        .sid
}

struct CurrentGroups(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl CurrentIdentityProvider for CurrentGroups {
    async fn current(&self, user_uuid: &str) -> Result<CurrentIdentity> {
        Ok(CurrentIdentity {
            user_uuid: user_uuid.into(),
            active: true,
            roles: vec![Role::Viewer],
            groups: self.0.lock().unwrap().clone(),
        })
    }

    async fn group_exists(&self, _: &str) -> Result<bool> {
        Ok(false)
    }
}

struct VerifiedTarget;

#[async_trait::async_trait]
impl TeamGitVerifier for VerifiedTarget {
    async fn verified_default_commit(
        &self,
        repository: &str,
        installation_id: u64,
        repository_id: u64,
        branch: &str,
    ) -> Result<String> {
        if branch == "absent" {
            return Err(CoreError::Storage("default ref unavailable".into()));
        }
        match repository {
            "example/alpha" => {
                assert_eq!((installation_id, repository_id, branch), (41, 73, "main"))
            }
            "example/beta" => {
                assert_eq!((installation_id, repository_id, branch), (42, 74, "stable"))
            }
            _ => panic!("unexpected repository reached verifier"),
        }
        Ok("a".repeat(40))
    }
}

#[tokio::test]
async fn current_identity_without_team_uuid_policy_cannot_use_cached_workspace_claim() {
    let current = Arc::new(CurrentGroups(Mutex::new(vec![
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
    ])));
    let f = fixture_with_current(Some(current)).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let sid = f
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec!["/teams/alpha/members".into()],
                user_uuid: Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".into()),
            },
            None,
            now,
        )
        .await
        .unwrap()
        .sid;
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/base",
            Some(&sid),
            json!({}),
            &[],
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL from notebook-team-target-postgres.sh"]
async fn postgres_team_target_route_refreshes_membership_on_every_workspace_request() {
    const MEMBER: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let targets = Arc::new(
        TeamGitTargets::connect(
            &std::env::var("ASTER_TEST_METADATA_URL").unwrap(),
            HashMap::from([(
                "alpha".into(),
                TeamGitPolicy {
                    member_group_uuid: MEMBER.into(),
                    maintainer_group_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
                    installation_id: 41,
                    allowed_repositories: HashMap::from([("example/alpha".into(), 73)]),
                },
            )]),
            Arc::new(VerifiedTarget),
        )
        .await
        .unwrap(),
    );
    let current = Arc::new(CurrentGroups(Mutex::new(vec![MEMBER.into()])));
    let f = fixture_with_services(Some(current.clone()), Some(targets)).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let sid = f
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec!["/teams/alpha/members".into()],
                user_uuid: Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".into()),
            },
            None,
            now,
        )
        .await
        .unwrap()
        .sid;
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/base",
            Some(&sid),
            json!({}),
            &[]
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/fresh",
            Some(&sid),
            notebook("SELECT 1"),
            &[("if-none-match", "*")],
        )
        .await
        .status(),
        StatusCode::OK,
        "verified session editor can save while team-only group reader reports Viewer"
    );
    *current.0.lock().unwrap() = vec![];
    for (method, path, body, extras) in [
        ("GET", "/api/teams/alpha/notebooks/base", json!({}), vec![]),
        (
            "GET",
            "/api/teams/alpha/notebooks/base/conversation",
            json!({}),
            vec![],
        ),
        (
            "GET",
            "/api/teams/alpha/notebooks/base/helper",
            json!({}),
            vec![],
        ),
        (
            "GET",
            "/api/teams/alpha/recovery/sessions",
            json!({}),
            vec![],
        ),
        (
            "GET",
            "/api/teams/alpha/recovery/sessions/old/notebooks/base",
            json!({}),
            vec![],
        ),
        (
            "PUT",
            "/api/teams/alpha/notebooks/base/helper",
            json!({"helper":"go"}),
            vec![],
        ),
        (
            "PUT",
            "/api/teams/alpha/notebooks/new",
            notebook("SELECT 1"),
            vec![("if-none-match", "*")],
        ),
        (
            "POST",
            "/api/teams/alpha/notebooks/base/sync",
            json!({}),
            vec![],
        ),
        (
            "POST",
            "/aster.v1.Aster/GetConversation",
            json!({"notebook":"base","team":"alpha","workspace":"session"}),
            vec![("connect-protocol-version", "1")],
        ),
        (
            "POST",
            "/aster.v1.Aster/SendMessage",
            json!({"notebook":"base","team":"alpha","workspace":"session","helper":"go","prompt":"hello","expectedRevision":"0"}),
            vec![("connect-protocol-version", "1")],
        ),
        (
            "POST",
            "/aster.v1.Aster/GetNotebookHelper",
            json!({"notebook":"base","team":"alpha","workspace":"session"}),
            vec![("connect-protocol-version", "1")],
        ),
        (
            "POST",
            "/aster.v1.Aster/PutNotebookHelper",
            json!({"notebook":"base","team":"alpha","workspace":"session","helper":"go"}),
            vec![("connect-protocol-version", "1")],
        ),
    ] {
        assert_eq!(
            call(&f.app, method, path, Some(&sid), body, &extras)
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "{method} {path} must observe current membership"
        );
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL from notebook-team-target-postgres.sh"]
async fn postgres_team_target_route_uses_fresh_uuid_groups_and_never_activates_workspace() {
    const MEMBER: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const MAINTAINER: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    const BETA_MEMBER: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    const BETA_MAINTAINER: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
    let team = format!("alpha-route-{}", std::process::id());
    let beta_team = format!("beta-route-{}", std::process::id());
    let policy = HashMap::from([
        (
            team.clone(),
            TeamGitPolicy {
                member_group_uuid: MEMBER.into(),
                maintainer_group_uuid: MAINTAINER.into(),
                installation_id: 41,
                allowed_repositories: HashMap::from([("example/alpha".into(), 73)]),
            },
        ),
        (
            beta_team.clone(),
            TeamGitPolicy {
                member_group_uuid: BETA_MEMBER.into(),
                maintainer_group_uuid: BETA_MAINTAINER.into(),
                installation_id: 42,
                allowed_repositories: HashMap::from([("example/beta".into(), 74)]),
            },
        ),
    ]);
    let targets = Arc::new(
        TeamGitTargets::connect(
            &std::env::var("ASTER_TEST_METADATA_URL").unwrap(),
            policy.clone(),
            Arc::new(VerifiedTarget),
        )
        .await
        .unwrap(),
    );
    let current = Arc::new(CurrentGroups(Mutex::new(vec![
        MEMBER.into(),
        MAINTAINER.into(),
    ])));
    let f = fixture_with_services(Some(current.clone()), Some(targets)).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let sid = f
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Viewer],
                // Cached path claims are intentionally not the UUID authority.
                groups: vec!["/teams/alpha/maintainers".into()],
                user_uuid: Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".into()),
            },
            None,
            now,
        )
        .await
        .unwrap()
        .sid;
    let path = format!("/api/teams/{team}/notebook-target");
    let response = call(
        &f.app,
        "PUT",
        &path,
        Some(&sid),
        json!({"repository":"example/alpha","default_branch":"main"}),
        &[("if-none-match", "*")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["default_commit"], "a".repeat(40));
    assert_eq!(value["active"], false);
    assert_eq!(f.workspaces.target("alpha").unwrap().unwrap().version, 1);
    *current.0.lock().unwrap() = vec![
        "/teams/alpha/members".into(),
        "/teams/alpha/maintainers".into(),
    ];
    assert_eq!(
        call(
            &f.app,
            "PUT",
            &path,
            Some(&sid),
            json!({"repository":"example/alpha","default_branch":"main"}),
            &[("if-match", "\"1\"")],
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    *current.0.lock().unwrap() = vec![MEMBER.into(), MAINTAINER.into()];
    for (repository, branch, expected) in [
        ("example/other", "main", StatusCode::FORBIDDEN),
        ("example/alpha", "main?bad", StatusCode::BAD_REQUEST),
        ("example/alpha", "absent", StatusCode::BAD_GATEWAY),
    ] {
        assert_eq!(
            call(
                &f.app,
                "PUT",
                &path,
                Some(&sid),
                json!({"repository":repository,"default_branch":branch}),
                &[("if-match", "\"1\"")],
            )
            .await
            .status(),
            expected
        );
    }
    assert_eq!(
        call(
            &f.app,
            "PUT",
            &path,
            Some(&sid),
            json!({"repository":"example/alpha","default_branch":"main"}),
            &[("if-match", "\"2\"")],
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    *current.0.lock().unwrap() = vec![MEMBER.into()];
    assert_eq!(
        call(&f.app, "GET", &path, Some(&sid), json!({}), &[])
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        call(
            &f.app,
            "PUT",
            &path,
            Some(&sid),
            json!({"repository":"example/alpha","default_branch":"main"}),
            &[("if-match", "\"1\"")],
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    *current.0.lock().unwrap() = vec![];
    assert_eq!(
        call(&f.app, "GET", &path, Some(&sid), json!({}), &[])
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    *current.0.lock().unwrap() = vec![BETA_MEMBER.into(), BETA_MAINTAINER.into()];
    assert_eq!(
        call(&f.app, "GET", &path, Some(&sid), json!({}), &[])
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/unknown/notebook-target",
            Some(&sid),
            json!({"repository":"example/beta","default_branch":"stable"}),
            &[("if-none-match", "*")],
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let beta_path = format!("/api/teams/{beta_team}/notebook-target");
    assert_eq!(
        call(
            &f.app,
            "PUT",
            &beta_path,
            Some(&sid),
            json!({"repository":"example/beta","default_branch":"stable"}),
            &[("if-none-match", "*")],
        )
        .await
        .status(),
        StatusCode::OK
    );
    let reopened = TeamGitTargets::connect(
        &std::env::var("ASTER_TEST_METADATA_URL").unwrap(),
        policy,
        Arc::new(VerifiedTarget),
    )
    .await
    .unwrap();
    let beta_principal = Principal {
        subject: "alice".into(),
        roles: vec![Role::Viewer],
        groups: vec![BETA_MEMBER.into(), BETA_MAINTAINER.into()],
        user_uuid: Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".into()),
    };
    assert_eq!(
        reopened
            .get(&beta_team, &beta_principal)
            .await
            .unwrap()
            .version,
        1
    );
    let pool = sqlx::PgPool::connect(&std::env::var("ASTER_TEST_METADATA_URL").unwrap())
        .await
        .unwrap();
    let (actor, repository_id, installation_id): (String, i64, i64) = sqlx::query_as(
        "SELECT actor_subject, repository_id, installation_id FROM team_git_target_events WHERE team = $1 AND version = 1",
    )
    .bind(&beta_team)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (actor.as_str(), repository_id, installation_id),
        ("alice", 74, 42)
    );
}

#[tokio::test]
async fn cached_team_maintainer_loses_target_write_after_current_group_removal() {
    let authority = Arc::new(CurrentGroups(Mutex::new(vec![])));
    let f = fixture_with_current(Some(authority.clone())).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let sid = f
        .sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec![
                    "/teams/alpha/members".into(),
                    "/teams/alpha/maintainers".into(),
                ],
                user_uuid: Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()),
            },
            None,
            now,
        )
        .await
        .unwrap()
        .sid;
    let response = call(
        &f.app,
        "PUT",
        "/api/teams/alpha/notebook-target",
        Some(&sid),
        json!({"repository":"example/alpha","default_branch":"main"}),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.workspaces.target("alpha").unwrap().unwrap().version, 1);
    *authority.0.lock().unwrap() = vec![
        "/teams/alpha/members".into(),
        "/teams/alpha/maintainers".into(),
    ];
    let response = call(
        &f.app,
        "PUT",
        "/api/teams/alpha/notebook-target",
        Some(&sid),
        json!({"repository":"example/alpha","default_branch":"main"}),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(f.workspaces.target("alpha").unwrap().unwrap().version, 2);
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    sid: Option<&str>,
    body: serde_json::Value,
    extra: &[(&str, &str)],
) -> Response<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(sid) = sid {
        request = request.header("cookie", format!("aster_session={sid}"));
    }
    for (name, value) in extra {
        request = request.header(*name, *value);
    }
    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

fn notebook(sql: &str) -> serde_json::Value {
    json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":sql}]})
}

async fn json_body(response: Response<Body>) -> serde_json::Value {
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn workspace_private_context_conversations_do_not_cross_teams_or_sessions() {
    let f = fixture().await;
    let first = sid(
        &f.sessions,
        "alice",
        &["/teams/alpha/members", "/teams/beta/members"],
    )
    .await;
    let second = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let mut ids = HashSet::new();
    for (team, session) in [("alpha", &first), ("beta", &first), ("alpha", &second)] {
        let response = call(
            &f.app,
            "GET",
            &format!("/api/teams/{team}/notebooks/base/conversation"),
            Some(session),
            json!({}),
            &[],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        ids.insert(body["id"].as_str().unwrap().to_owned());
    }
    assert_eq!(
        ids.len(),
        3,
        "one user and notebook ID need three transcripts"
    );
}

#[tokio::test]
async fn workspace_private_context_helper_choice_does_not_cross_teams_or_sessions() {
    let f = fixture().await;
    let first = sid(
        &f.sessions,
        "alice",
        &["/teams/alpha/members", "/teams/beta/members"],
    )
    .await;
    let second = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    for helper in ["alpha", "beta"] {
        assert_eq!(
            call(
                &f.app,
                "PUT",
                &format!("/api/llm/{helper}"),
                Some(&first),
                json!({"base_url":"http://127.0.0.1:9/v1","model":helper,"api_key":"fake"}),
                &[],
            )
            .await
            .status(),
            StatusCode::OK
        );
    }
    for (team, session, helper) in [
        ("alpha", &first, "alpha"),
        ("beta", &first, "beta"),
        ("alpha", &second, "beta"),
    ] {
        assert_eq!(
            call(
                &f.app,
                "PUT",
                &format!("/api/teams/{team}/notebooks/base/helper"),
                Some(session),
                json!({"helper":helper}),
                &[],
            )
            .await
            .status(),
            StatusCode::OK
        );
    }
    for (team, session, expected) in [
        ("alpha", &first, "alpha"),
        ("beta", &first, "beta"),
        ("alpha", &second, "beta"),
    ] {
        let response = call(
            &f.app,
            "GET",
            &format!("/api/teams/{team}/notebooks/base/helper"),
            Some(session),
            json!({}),
            &[],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["helper"], expected);
    }
}

#[tokio::test]
async fn workspace_private_context_unqualified_and_foreign_reads_refuse() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let beta_only = sid(&f.sessions, "bob", &["/teams/beta/members"]).await;
    for (method, path, body, extras) in [
        ("GET", "/api/notebooks/base/helper", json!({}), vec![]),
        (
            "PUT",
            "/api/notebooks/base/helper",
            json!({"helper":"alpha"}),
            vec![],
        ),
        (
            "POST",
            "/aster.v1.Aster/GetConversation",
            json!({"notebook":"base"}),
            vec![("connect-protocol-version", "1")],
        ),
        (
            "POST",
            "/aster.v1.Aster/SendMessage",
            json!({"notebook":"base","helper":"alpha","prompt":"hello","expectedRevision":"0"}),
            vec![("connect-protocol-version", "1")],
        ),
        (
            "POST",
            "/aster.v1.Aster/GetNotebookHelper",
            json!({"notebook":"base"}),
            vec![("connect-protocol-version", "1")],
        ),
        (
            "POST",
            "/aster.v1.Aster/PutNotebookHelper",
            json!({"notebook":"base","helper":"alpha"}),
            vec![("connect-protocol-version", "1")],
        ),
    ] {
        assert_eq!(
            call(&f.app, method, path, Some(&alice), body, &extras)
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "{path} must not reach legacy metadata"
        );
    }
    for path in [
        "/api/teams/alpha/notebooks/missing/conversation",
        "/api/teams/alpha/notebooks/missing/helper",
    ] {
        assert_eq!(
            call(&f.app, "GET", path, Some(&alice), json!({}), &[])
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/base/conversation",
            Some(&beta_only),
            json!({}),
            &[],
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/base/conversation",
            Some(&alice),
            json!({}),
            &[("x-aster-workspace", "sessions/other-branch")],
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn team_mode_unqualified_notebook_reads_refuse_legacy_checkout() {
    let f = fixture().await;
    f._legacy
        .save(
            &Notebook {
                id: "legacy-secret".into(),
                title: "Legacy".into(),
                cells: vec![],
            },
            "alice",
        )
        .await
        .unwrap();
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    for path in ["/api/notebooks", "/api/notebooks/legacy-secret"] {
        assert_eq!(
            call(&f.app, "GET", path, Some(&alice), json!({}), &[])
                .await
                .status(),
            StatusCode::FORBIDDEN,
            "{path} must not read the legacy checkout in team mode"
        );
    }
    for (path, body) in [
        ("/aster.v1.Aster/GetNotebook", json!({"id":"legacy-secret"})),
        ("/aster.v1.Aster/ListNotebooks", json!({})),
    ] {
        assert_eq!(
            call(
                &f.app,
                "POST",
                path,
                Some(&alice),
                body,
                &[("connect-protocol-version", "1")],
            )
            .await
            .status(),
            StatusCode::FORBIDDEN,
            "{path} must not read the legacy checkout in team mode"
        );
    }
}

#[tokio::test]
async fn qualified_connect_notebooks_read_only_selected_team_workspace() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let bob = sid(&f.sessions, "bob", &["/teams/beta/members"]).await;
    let second = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let connect = &[("connect-protocol-version", "1")][..];

    let listed = call(
        &f.app,
        "POST",
        "/aster.v1.Aster/ListNotebooks",
        Some(&alice),
        json!({"team":"alpha","workspace":"session"}),
        connect,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(json_body(listed).await["ids"], json!(["base"]));

    for (session, team, workspace, expected) in [
        (&alice, "alpha", "session", StatusCode::OK),
        (&alice, "alpha", "personal", StatusCode::OK),
        (&bob, "beta", "session", StatusCode::OK),
        (&alice, "beta", "session", StatusCode::FORBIDDEN),
        (&second, "alpha", "session", StatusCode::OK),
    ] {
        let response = call(
            &f.app,
            "POST",
            "/aster.v1.Aster/GetNotebook",
            Some(session),
            json!({"id":"base","team":team,"workspace":workspace}),
            connect,
        )
        .await;
        assert_eq!(response.status(), expected, "{team}/{workspace}");
        if expected == StatusCode::OK {
            assert_eq!(json_body(response).await["id"], "base");
        }
    }
}

#[tokio::test]
async fn team_mode_connect_save_refuses_legacy_write() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let response = call(
        &f.app,
        "POST",
        "/aster.v1.Aster/SaveNotebook",
        Some(&alice),
        json!({"id":"base","notebook":notebook("SELECT 1"),"ifAbsent":true}),
        &[("connect-protocol-version", "1")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn workspace_private_context_connect_reads_same_team_conversation_and_helper() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let rest = call(
        &f.app,
        "GET",
        "/api/teams/alpha/notebooks/base/conversation",
        Some(&alice),
        json!({}),
        &[],
    )
    .await;
    assert_eq!(rest.status(), StatusCode::OK);
    let rest_id = json_body(rest).await["id"].as_str().unwrap().to_owned();
    let rpc = call(
        &f.app,
        "POST",
        "/aster.v1.Aster/GetConversation",
        Some(&alice),
        json!({"notebook":"base","team":"alpha","workspace":"session"}),
        &[("connect-protocol-version", "1")],
    )
    .await;
    assert_eq!(rpc.status(), StatusCode::OK);
    assert_eq!(json_body(rpc).await["id"], rest_id);
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/llm/go",
            Some(&alice),
            json!({"base_url":"http://127.0.0.1:9/v1","model":"go","api_key":"fake"}),
            &[],
        )
        .await
        .status(),
        StatusCode::OK
    );
    let put = call(
        &f.app,
        "POST",
        "/aster.v1.Aster/PutNotebookHelper",
        Some(&alice),
        json!({"notebook":"base","team":"alpha","workspace":"session","helper":"go"}),
        &[("connect-protocol-version", "1")],
    )
    .await;
    assert_eq!(put.status(), StatusCode::OK);
    let rest = call(
        &f.app,
        "GET",
        "/api/teams/alpha/notebooks/base/helper",
        Some(&alice),
        json!({}),
        &[],
    )
    .await;
    assert_eq!(rest.status(), StatusCode::OK);
    assert_eq!(json_body(rest).await["helper"], "go");
}

#[tokio::test]
async fn workspace_private_context_connect_send_appends_only_to_selected_session() {
    let f = fixture().await;
    let alice = sid(
        &f.sessions,
        "alice",
        &["/teams/alpha/members", "/teams/beta/members"],
    )
    .await;
    let mock = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(|| async {
            axum::Json(json!({"choices":[{"message":{"content":"SELECT 1"}}]}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, mock).await });
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/llm/go",
            Some(&alice),
            json!({"base_url":format!("http://{addr}/v1"),"model":"go","api_key":"fake"}),
            &[],
        )
        .await
        .status(),
        StatusCode::OK
    );
    let sent = call(
        &f.app,
        "POST",
        "/aster.v1.Aster/SendMessage",
        Some(&alice),
        json!({"notebook":"base","team":"alpha","workspace":"session","helper":"go","prompt":"Explain","expectedRevision":"0"}),
        &[("connect-protocol-version", "1")],
    )
    .await;
    assert_eq!(sent.status(), StatusCode::OK);
    let body = json_body(sent).await;
    assert_eq!(body["revision"], "1");
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    let alpha = json_body(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/base/conversation",
            Some(&alice),
            json!({}),
            &[],
        )
        .await,
    )
    .await;
    let beta = json_body(
        call(
            &f.app,
            "GET",
            "/api/teams/beta/notebooks/base/conversation",
            Some(&alice),
            json!({}),
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(alpha["revision"], 1);
    assert_eq!(beta["revision"], 0);
    task.abort();
}

#[tokio::test]
async fn workspace_private_context_connect_denials_never_call_helper_or_append() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let bob = sid(&f.sessions, "bob", &["/teams/beta/members"]).await;
    let revoked = sid(&f.sessions, "revoked", &["/teams/alpha/members"]).await;
    f.sessions.revoke(&revoked).await.unwrap();
    let calls = Arc::new(AtomicU64::new(0));
    let seen = calls.clone();
    let mock = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move || {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                axum::Json(json!({"choices":[{"message":{"content":"SELECT 1"}}]}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, mock).await });
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/llm/go",
            Some(&alice),
            json!({"base_url":format!("http://{addr}/v1"),"model":"go","api_key":"fake"}),
            &[],
        )
        .await
        .status(),
        StatusCode::OK
    );
    for (sid, notebook, team, expected) in [
        (&bob, "base", Some("alpha"), StatusCode::FORBIDDEN),
        (&alice, "missing", Some("alpha"), StatusCode::NOT_FOUND),
        (&revoked, "base", Some("alpha"), StatusCode::FORBIDDEN),
        (&alice, "base", None, StatusCode::FORBIDDEN),
    ] {
        let response = call(
            &f.app,
            "POST",
            "/aster.v1.Aster/SendMessage",
            Some(sid),
            json!({"notebook":notebook,"team":team,"workspace":"session","helper":"go","prompt":"Explain","expectedRevision":"0"}),
            &[("connect-protocol-version", "1")],
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let conversation = json_body(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/base/conversation",
            Some(&alice),
            json!({}),
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(conversation["revision"], 0);
    task.abort();
}

#[tokio::test]
async fn verified_team_maintainer_can_configure_while_members_cannot() {
    let f = fixture().await;
    let maintainer = sid(
        &f.sessions,
        "alice",
        &["/teams/alpha/members", "/teams/alpha/maintainers"],
    )
    .await;
    let member = sid(&f.sessions, "bob", &["/teams/alpha/members"]).await;
    let other_team = sid(
        &f.sessions,
        "carol",
        &["/teams/beta/members", "/teams/beta/maintainers"],
    )
    .await;
    let path = "/api/teams/alpha/notebook-target";
    let body = json!({"repository":"example/alpha", "default_branch":"main"});
    assert_eq!(
        call(
            &f.app,
            "PUT",
            path,
            Some(&maintainer),
            json!({"repository":"foreign/repo", "default_branch":"main"}),
            &[("if-match", "\"1\"")]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&f.app, "PUT", path, Some(&maintainer), body.clone(), &[])
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &f.app,
            "PUT",
            path,
            Some(&maintainer),
            body.clone(),
            &[("if-match", "\"0\"")]
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &f.app,
            "PUT",
            path,
            Some(&maintainer),
            body.clone(),
            &[("if-match", "\"1\"")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        call(&f.app, "PUT", path, Some(&member), body.clone(), &[])
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&f.app, "PUT", path, Some(&other_team), body, &[])
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn two_verified_teams_can_create_the_same_id_in_isolated_workspaces() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let carol = sid(&f.sessions, "carol", &["/teams/beta/members"]).await;
    let header = [("if-none-match", "*")];
    let alpha = call(
        &f.app,
        "PUT",
        "/api/teams/alpha/notebooks/sales",
        Some(&alice),
        notebook("SELECT 1"),
        &header,
    )
    .await;
    let beta = call(
        &f.app,
        "PUT",
        "/api/teams/beta/notebooks/sales",
        Some(&carol),
        notebook("SELECT 2"),
        &header,
    )
    .await;
    assert_eq!(alpha.status(), StatusCode::OK);
    assert_eq!(beta.status(), StatusCode::OK);
    let alpha = call(
        &f.app,
        "GET",
        "/api/teams/alpha/notebooks/sales",
        Some(&alice),
        json!({}),
        &[],
    )
    .await;
    let beta = call(
        &f.app,
        "GET",
        "/api/teams/beta/notebooks/sales",
        Some(&carol),
        json!({}),
        &[],
    )
    .await;
    let a: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(alpha.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    let b: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(beta.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(a["cells"][0]["sql"], "SELECT 1");
    assert_eq!(b["cells"][0]["sql"], "SELECT 2");
    let a_checkout = only_session_checkout(&f.root, "alpha");
    let b_checkout = only_session_checkout(&f.root, "beta");
    assert_ne!(
        git_output(&a_checkout, &["rev-parse", "HEAD"]),
        git_output(&b_checkout, &["rev-parse", "HEAD"])
    );
    assert_eq!(
        git_output(&a_checkout, &["rev-parse", "HEAD^"]),
        git_output(&f.root.join("alpha.git"), &["rev-parse", "refs/heads/main"])
    );
    assert_eq!(
        git_output(&b_checkout, &["rev-parse", "HEAD^"]),
        git_output(
            &f.root.join("beta.git"),
            &["rev-parse", "refs/heads/stable"]
        )
    );
}

#[tokio::test]
async fn two_sessions_of_one_subject_do_not_share_a_mutable_branch() {
    let f = fixture().await;
    let one = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let two = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let header = [("if-none-match", "*")];
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            Some(&one),
            notebook("SELECT 1"),
            &header
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            Some(&two),
            notebook("SELECT 2"),
            &header
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        notebook_sql(&f.app, "/api/teams/alpha/notebooks/sales", &one).await,
        "SELECT 1"
    );
    assert_eq!(
        notebook_sql(&f.app, "/api/teams/alpha/notebooks/sales", &two).await,
        "SELECT 2"
    );
    let checkouts = session_checkouts(&f.root, "alpha");
    assert_eq!(checkouts.len(), 2);
    let one_checkout = &checkouts[0];
    let two_checkout = &checkouts[1];
    assert_ne!(one_checkout, two_checkout);
    assert_ne!(
        git_output(one_checkout, &["rev-parse", "HEAD"]),
        git_output(two_checkout, &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn two_members_of_one_team_have_separate_session_branches() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let bob = sid(&f.sessions, "bob", &["/teams/alpha/members"]).await;
    for (session, sql) in [(&alice, "SELECT 11"), (&bob, "SELECT 22")] {
        assert_eq!(
            call(
                &f.app,
                "PUT",
                "/api/teams/alpha/notebooks/sales",
                Some(session),
                notebook(sql),
                &[("if-none-match", "*")]
            )
            .await
            .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        notebook_sql(&f.app, "/api/teams/alpha/notebooks/sales", &alice).await,
        "SELECT 11"
    );
    assert_eq!(
        notebook_sql(&f.app, "/api/teams/alpha/notebooks/sales", &bob).await,
        "SELECT 22"
    );
    assert_eq!(session_checkouts(&f.root, "alpha").len(), 2);
}

#[tokio::test]
async fn explicit_personal_choice_shares_only_the_subjects_persistent_branch() {
    let f = fixture().await;
    let first = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let second = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            Some(&first),
            notebook("SELECT 44"),
            &[("if-none-match", "*"), ("x-aster-workspace", "personal")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let shared = call(
        &f.app,
        "GET",
        "/api/teams/alpha/notebooks/sales",
        Some(&second),
        json!({}),
        &[("x-aster-workspace", "personal")],
    )
    .await;
    assert_eq!(shared.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(shared.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["cells"][0]["sql"], "SELECT 44");
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/sales",
            Some(&second),
            json!({}),
            &[]
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn forged_groups_or_client_branch_cannot_select_a_workspace() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let before = std::process::Command::new("git")
        .arg("-C")
        .arg(&f.dir)
        .args(["status", "--porcelain"])
        .output()
        .unwrap()
        .stdout;
    let forged = call(
        &f.app,
        "PUT",
        "/api/teams/beta/notebooks/sales",
        Some(&alice),
        notebook("SELECT 2"),
        &[
            ("if-none-match", "*"),
            ("x-aster-groups", "/teams/beta/members"),
        ],
    )
    .await;
    assert_eq!(forged.status(), StatusCode::FORBIDDEN);
    assert!(!f.root.join("workspaces/beta").exists());
    let foreign_ref = call(&f.app, "PUT", "/api/teams/alpha/notebooks/sales", Some(&alice), json!({"id":"sales", "title":"Sales", "cells":[{"id":"c1", "sql":"SELECT 9"}], "branch":"sessions/bob/foreign"}), &[("if-none-match", "*")]).await;
    assert_eq!(foreign_ref.status(), StatusCode::BAD_REQUEST);
    assert!(!f.root.join("workspaces/alpha").exists());
    let after = std::process::Command::new("git")
        .arg("-C")
        .arg(&f.dir)
        .args(["status", "--porcelain"])
        .output()
        .unwrap()
        .stdout;
    assert_eq!(after, before);
}

#[tokio::test]
async fn enabled_team_workspaces_make_legacy_writes_read_only() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    let response = call(
        &f.app,
        "PUT",
        "/api/notebooks/legacy-sales",
        Some(&alice),
        json!({"id":"legacy-sales", "title":"Legacy", "cells":[{"id":"c1", "sql":"SELECT 1"}]}),
        &[("if-none-match", "*")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!f.dir.join("legacy-sales.aster").exists());
}

#[tokio::test]
async fn target_change_and_restart_keep_an_unsynced_session_on_its_original_target() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            Some(&alice),
            notebook("SELECT 7"),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let old_target = f.workspaces.target("alpha").unwrap().unwrap();
    assert_eq!(old_target.repository, "example/alpha");
    let changed = f
        .workspaces
        .configure(
            "alpha",
            "example/alpha-next",
            "trunk",
            Some(old_target.version),
            "alice",
        )
        .unwrap();
    assert_ne!(changed.version, old_target.version);
    let root = f.root.clone();
    let policies = f.policies.clone();
    drop(f);

    let reopened = TeamWorkspaces::new(root.join("workspaces"), policies).unwrap();
    assert_eq!(
        reopened.target("alpha").unwrap().unwrap().repository,
        "example/alpha-next"
    );
    let principal = Principal {
        subject: "alice".into(),
        roles: vec![Role::Editor],
        groups: vec!["/teams/alpha/members".into()],
        user_uuid: None,
    };
    let old_store = reopened
        .workspace("alpha", &principal, &alice, false)
        .unwrap();
    assert_eq!(
        old_store.get("sales").await.unwrap().cells[0].sql,
        "SELECT 7"
    );
    let old_checkout = only_session_checkout(&root, "alpha");
    let metadata: serde_json::Value = serde_json::from_slice(
        &std::fs::read(old_checkout.join(".git/aster.workspace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata["repository"], "example/alpha");
    assert_eq!(metadata["version"], old_target.version);
}

#[tokio::test]
async fn first_session_starts_at_pinned_default_commit_after_upstream_moves() {
    let f = fixture().await;
    let pinned = f
        .workspaces
        .target("alpha")
        .unwrap()
        .unwrap()
        .default_commit
        .unwrap();
    let seed = f.root.join("alpha-seed");
    std::fs::write(seed.join("later.txt"), "later upstream change\n").unwrap();
    git(&seed, &["add", "later.txt"]);
    git(&seed, &["commit", "-qm", "upstream moved"]);
    git(&seed, &["push", "-q", "origin", "main"]);
    assert_ne!(git_output(&seed, &["rev-parse", "HEAD"]), pinned);

    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            Some(&alice),
            notebook("SELECT 9"),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = only_session_checkout(&f.root, "alpha");
    assert_eq!(git_output(&checkout, &["rev-parse", "HEAD^"]), pinned);
}

#[tokio::test]
async fn session_bearer_secret_is_never_a_git_ref_or_checkout_path() {
    let f = fixture().await;
    let alice = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            Some(&alice),
            notebook("SELECT 1"),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    let checkout = only_session_checkout(&f.root, "alpha");
    let refs = git_output(&checkout, &["for-each-ref", "--format=%(refname)"]);
    assert!(!refs.contains(&alice), "Git ref leaked a bearer session ID");
    assert!(
        !checkout.to_string_lossy().contains(&alice),
        "checkout path leaked a bearer session ID"
    );
}

#[tokio::test]
async fn expired_session_content_requires_explicit_owner_recovery() {
    let f = fixture().await;
    let old = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    assert_eq!(
        call(
            &f.app,
            "PUT",
            "/api/teams/alpha/notebooks/sales",
            Some(&old),
            notebook("SELECT 77"),
            &[("if-none-match", "*")]
        )
        .await
        .status(),
        StatusCode::OK
    );
    f.sessions.revoke(&old).await.unwrap();
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/sales",
            Some(&old),
            json!({}),
            &[]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let current = sid(&f.sessions, "alice", &["/teams/alpha/members"]).await;
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/api/teams/alpha/notebooks/sales",
            Some(&current),
            json!({}),
            &[]
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );

    let list = call(
        &f.app,
        "GET",
        "/api/teams/alpha/recovery/sessions",
        Some(&current),
        json!({}),
        &[],
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let rows: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(list.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    let key = rows[0]["session_key"].as_str().unwrap();
    assert_ne!(key, old);
    assert!(!key.contains(&old));
    let path = format!("/api/teams/alpha/recovery/sessions/{key}/notebooks/sales");
    assert_eq!(notebook_sql(&f.app, &path, &current).await, "SELECT 77");
    let bob = sid(&f.sessions, "bob", &["/teams/alpha/members"]).await;
    assert_eq!(
        call(&f.app, "GET", &path, Some(&bob), json!({}), &[])
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let bob_list = call(
        &f.app,
        "GET",
        "/api/teams/alpha/recovery/sessions",
        Some(&bob),
        json!({}),
        &[],
    )
    .await;
    assert_eq!(bob_list.status(), StatusCode::OK);
    let bob_rows: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(bob_list.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(bob_rows, json!([]));
}
