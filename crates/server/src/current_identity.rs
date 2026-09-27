//! Request-time identity check for eventual shared-model use. This helper is
//! not connected to ordinary routes until the authoritative provider is proven.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use aster_core::{CoreError, CurrentIdentity, CurrentIdentityProvider, Principal, Result, Role};
use async_trait::async_trait;
use axum::http::HeaderMap;
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::{cookie, principal_with_session, ApiError, AppState};

/// Authentik's current user and parent-group graph. No authorization result is
/// cached: every shared request must observe the provider again.
pub(crate) struct AuthentikCurrentIdentity {
    origin: reqwest::Url,
    token: String,
    admin_group_uuid: Option<String>,
    editor_group_uuid: Option<String>,
    http: reqwest::Client,
    budget: Duration,
}

#[derive(Deserialize)]
struct UsersPage {
    pagination: Pagination,
    results: Vec<AuthentikUser>,
}

#[derive(Deserialize)]
struct Pagination {
    count: usize,
}

#[derive(Deserialize)]
struct AuthentikUser {
    uuid: String,
    is_active: bool,
    groups: Vec<String>,
}

#[derive(Deserialize)]
struct AuthentikGroup {
    pk: String,
    // Deployed Authentik 2026.5.6 serializes all direct parents by UUID.
    // Missing this field must fail closed, not silently drop inheritance.
    parents: Vec<String>,
}

fn unavailable() -> CoreError {
    CoreError::Unauthorized("current identity unavailable".into())
}

pub(crate) fn uuid(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || bytes.iter().enumerate().any(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte != b'-'
            } else {
                !byte.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn cyclic_group_graph(edges: &HashMap<String, Vec<String>>) -> bool {
    fn visit(
        node: &str,
        edges: &HashMap<String, Vec<String>>,
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
    ) -> bool {
        if done.contains(node) {
            return false;
        }
        if !visiting.insert(node.to_string()) {
            return true;
        }
        if edges.get(node).is_some_and(|parents| {
            parents
                .iter()
                .any(|parent| visit(parent, edges, visiting, done))
        }) {
            return true;
        }
        visiting.remove(node);
        done.insert(node.to_string());
        false
    }
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();
    edges
        .keys()
        .any(|node| visit(node, edges, &mut visiting, &mut done))
}

impl AuthentikCurrentIdentity {
    /// Team Git needs current group UUIDs without the shared-model role map.
    pub(crate) fn new_for_team_git(origin: &str, token: String) -> Result<Self> {
        Self::build(origin, token, None)
    }

    /// Construct only for an explicitly configured HTTPS Authentik origin and
    /// an Aster-specific read token from SecretStore.
    pub(crate) fn new(
        origin: &str,
        token: String,
        admin_group_uuid: &str,
        editor_group_uuid: &str,
    ) -> Result<Self> {
        let admin_group_uuid = uuid(admin_group_uuid).ok_or_else(unavailable)?;
        let editor_group_uuid = uuid(editor_group_uuid).ok_or_else(unavailable)?;
        Self::build(origin, token, Some((admin_group_uuid, editor_group_uuid)))
    }

    fn build(origin: &str, token: String, roles: Option<(String, String)>) -> Result<Self> {
        let url = reqwest::Url::parse(origin).map_err(|_| unavailable())?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
            || token.is_empty()
            || token.len() > 4096
            || token.chars().any(char::is_control)
        {
            return Err(unavailable());
        }
        let (admin_group_uuid, editor_group_uuid) = match roles {
            Some((admin, editor)) => (Some(admin), Some(editor)),
            None => (None, None),
        };
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self {
            origin: url,
            token,
            admin_group_uuid,
            editor_group_uuid,
            http,
            budget: Duration::from_secs(5),
        })
    }

    async fn get_optional<T: DeserializeOwned>(&self, url: reqwest::Url) -> Result<Option<T>> {
        let mut response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| unavailable())?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(unavailable());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if bytes.len().saturating_add(chunk.len()) > 1_048_576 {
                return Err(unavailable());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| unavailable())
    }

    async fn get<T: DeserializeOwned>(&self, url: reqwest::Url) -> Result<T> {
        self.get_optional(url).await?.ok_or_else(unavailable)
    }
}

#[async_trait]
impl CurrentIdentityProvider for AuthentikCurrentIdentity {
    async fn group_exists(&self, group_uuid: &str) -> Result<bool> {
        tokio::time::timeout(self.budget, async {
            let group_uuid = uuid(group_uuid).ok_or_else(unavailable)?;
            let url = self
                .origin
                .join(&format!(
                    "/api/v3/core/groups/{group_uuid}/?include_users=false"
                ))
                .map_err(|_| unavailable())?;
            match self.get_optional::<AuthentikGroup>(url).await? {
                Some(group) if uuid(&group.pk).as_deref() == Some(group_uuid.as_str()) => Ok(true),
                Some(_) => Err(unavailable()),
                None => Ok(false),
            }
        })
        .await
        .map_err(|_| unavailable())?
    }

    async fn current(&self, user_uuid: &str) -> Result<CurrentIdentity> {
        tokio::time::timeout(self.budget, async {
            let user_uuid = uuid(user_uuid).ok_or_else(unavailable)?;
            let mut url = self
                .origin
                .join("/api/v3/core/users/")
                .map_err(|_| unavailable())?;
            url.query_pairs_mut()
                .append_pair("uuid", &user_uuid)
                .append_pair("include_groups", "true")
                .append_pair("page_size", "2");
            let page: UsersPage = self.get(url).await?;
            if page.pagination.count != 1 || page.results.len() != 1 {
                return Err(unavailable());
            }
            let user = page.results.into_iter().next().ok_or_else(unavailable)?;
            if !user.is_active || uuid(&user.uuid).as_deref() != Some(user_uuid.as_str()) {
                return Err(unavailable());
            }
            if user.groups.len() > 64 {
                return Err(unavailable());
            }
            let mut pending = VecDeque::new();
            for group in user.groups {
                pending.push_back(uuid(&group).ok_or_else(unavailable)?);
            }
            let mut groups = HashSet::new();
            let mut edges = HashMap::new();
            while let Some(group_uuid) = pending.pop_front() {
                if !groups.insert(group_uuid.clone()) {
                    continue;
                }
                if groups.len() > 64 {
                    return Err(unavailable());
                }
                let url = self
                    .origin
                    .join(&format!(
                        "/api/v3/core/groups/{group_uuid}/?include_users=false"
                    ))
                    .map_err(|_| unavailable())?;
                let group: AuthentikGroup = self.get(url).await?;
                if uuid(&group.pk).as_deref() != Some(group_uuid.as_str()) {
                    return Err(unavailable());
                }
                let mut parents = Vec::new();
                for parent in group.parents {
                    let parent = uuid(&parent).ok_or_else(unavailable)?;
                    pending.push_back(parent.clone());
                    parents.push(parent);
                }
                edges.insert(group_uuid, parents);
            }
            if cyclic_group_graph(&edges) {
                return Err(unavailable());
            }
            let roles = if self
                .admin_group_uuid
                .as_ref()
                .is_some_and(|admin| groups.contains(admin))
            {
                vec![Role::Admin]
            } else if self
                .editor_group_uuid
                .as_ref()
                .is_some_and(|editor| groups.contains(editor))
            {
                vec![Role::Editor]
            } else {
                vec![Role::Viewer]
            };
            let mut groups: Vec<_> = groups.into_iter().collect();
            groups.sort();
            Ok(CurrentIdentity {
                user_uuid,
                active: true,
                roles,
                groups,
            })
        })
        .await
        .map_err(|_| unavailable())?
    }
}

pub(crate) async fn refresh_for_shared(
    provider: &dyn CurrentIdentityProvider,
    principal: &Principal,
) -> Result<Principal> {
    let uuid = principal
        .user_uuid
        .as_deref()
        .filter(|uuid| !uuid.is_empty())
        .ok_or_else(|| CoreError::Unauthorized("verified user identity unavailable".into()))?;
    let current = provider
        .current(uuid)
        .await
        .map_err(|_| CoreError::Unauthorized("current identity unavailable".into()))?;
    if !current.active || current.user_uuid != uuid || current.roles.is_empty() {
        return Err(CoreError::Unauthorized(
            "current identity unavailable".into(),
        ));
    }
    Ok(Principal {
        subject: principal.subject.clone(),
        roles: current.roles,
        groups: current.groups,
        user_uuid: Some(uuid.to_string()),
    })
}

/// Require a verified OIDC session and a fresh current identity. The isolated
/// no-IdP development header seam may administer a test registry, but neither
/// a development-created session nor a production dev header can use it.
pub(crate) async fn current_principal(
    state: &AppState,
    headers: &HeaderMap,
    isolated_dev_admin: bool,
) -> std::result::Result<Principal, ApiError> {
    let (caller, verified) = principal_with_session(state, headers).await?;
    if verified {
        let provider = state.current_identity.as_deref().ok_or_else(unavailable)?;
        return Ok(refresh_for_shared(provider, &caller).await?);
    }
    if isolated_dev_admin
        && state.identity.is_none()
        && state.dev_login
        && cookie(headers, "aster_session").is_none()
    {
        return Ok(caller);
    }
    Err(unavailable().into())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use aster_core::{CoreError, CurrentIdentity, Role, SharedModelGrant};
    use async_trait::async_trait;

    use super::*;

    const USER: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    const CHILD: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    const PARENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const OTHER_PARENT: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

    #[test]
    fn team_git_group_reader_has_no_shared_model_role_configuration_dependency() {
        assert!(AuthentikCurrentIdentity::new_for_team_git(
            "https://auth.example.invalid",
            "disposable".into(),
        )
        .is_ok());
    }

    #[test]
    fn authentik_requires_https_origin_and_stable_group_ids() {
        assert!(AuthentikCurrentIdentity::new(
            "http://auth.example.invalid",
            "disposable".into(),
            PARENT,
            CHILD,
        )
        .is_err());
        assert!(AuthentikCurrentIdentity::new(
            "https://user:password@auth.example.invalid",
            "disposable".into(),
            PARENT,
            CHILD,
        )
        .is_err());
        assert!(AuthentikCurrentIdentity::new(
            "https://auth.example.invalid",
            "disposable".into(),
            "admin-group-name",
            CHILD,
        )
        .is_err());
    }

    #[test]
    fn group_parent_graph_allows_shared_ancestors_but_rejects_cycles() {
        let shared_parent = HashMap::from([
            (CHILD.to_string(), vec![PARENT.to_string()]),
            (OTHER_PARENT.to_string(), vec![PARENT.to_string()]),
            (PARENT.to_string(), vec![]),
        ]);
        assert!(!cyclic_group_graph(&shared_parent));
        let cycle = HashMap::from([
            (CHILD.to_string(), vec![PARENT.to_string()]),
            (PARENT.to_string(), vec![CHILD.to_string()]),
        ]);
        assert!(cyclic_group_graph(&cycle));
    }

    #[tokio::test]
    async fn authentik_group_lookup_distinguishes_existing_missing_and_unavailable() {
        use axum::extract::Path;
        use axum::http::StatusCode;
        use axum::routing::get;
        use axum::{Json, Router};
        use serde_json::json;

        let app = Router::new().route(
            "/api/v3/core/groups/{id}/",
            get(|Path(id): Path<String>| async move {
                if id == PARENT {
                    (StatusCode::OK, Json(json!({"pk":id,"parents":[]})))
                } else if id == OTHER_PARENT {
                    (StatusCode::NOT_FOUND, Json(json!({})))
                } else {
                    (StatusCode::FORBIDDEN, Json(json!({})))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = AuthentikCurrentIdentity {
            origin: reqwest::Url::parse(&origin).unwrap(),
            token: "disposable-token".into(),
            admin_group_uuid: Some(PARENT.into()),
            editor_group_uuid: Some(CHILD.into()),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            budget: Duration::from_secs(5),
        };
        assert!(provider.group_exists(PARENT).await.unwrap());
        assert!(!provider.group_exists(OTHER_PARENT).await.unwrap());
        assert!(provider.group_exists(CHILD).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn authentik_reads_direct_and_parent_groups_on_each_request() {
        use axum::extract::Path;
        use axum::routing::get;
        use axum::{Json, Router};
        use serde_json::json;

        let member = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let user_member = member.clone();
        let app = Router::new()
            .route(
                "/api/v3/core/users/",
                get(move || {
                    let member = user_member.clone();
                    async move {
                        let groups = if member.load(std::sync::atomic::Ordering::SeqCst) {
                            vec![CHILD]
                        } else {
                            vec![]
                        };
                        Json(json!({"pagination":{"count":1},"results":[{
                            "uuid":USER,"is_active":true,"groups":groups
                        }]}))
                    }
                }),
            )
            .route(
                "/api/v3/core/groups/{id}/",
                get(|Path(id): Path<String>| async move {
                    let parents = if id == CHILD {
                        vec![PARENT, OTHER_PARENT]
                    } else {
                        vec![]
                    };
                    Json(json!({"pk":id,"parents":parents}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = AuthentikCurrentIdentity {
            origin: reqwest::Url::parse(&origin).unwrap(),
            token: "disposable-token".into(),
            admin_group_uuid: Some(PARENT.into()),
            editor_group_uuid: Some(CHILD.into()),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            budget: Duration::from_secs(5),
        };

        let first = provider.current(USER).await.unwrap();
        assert!(first.groups.contains(&CHILD.to_string()));
        assert!(first.groups.contains(&PARENT.to_string()));
        assert!(first.groups.contains(&OTHER_PARENT.to_string()));
        assert_eq!(first.roles, vec![Role::Admin]);
        member.store(false, std::sync::atomic::Ordering::SeqCst);
        let removed = provider.current(USER).await.unwrap();
        assert_eq!(removed.groups, Vec::<String>::new());
        assert_eq!(removed.roles, vec![Role::Viewer]);
        server.abort();
    }

    #[tokio::test]
    async fn authentik_rejects_ambiguous_users_and_incomplete_group_graph() {
        use axum::extract::Path;
        use axum::routing::get;
        use axum::{Json, Router};
        use serde_json::json;

        let ambiguous = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let user_mode = ambiguous.clone();
        let app = Router::new()
            .route(
                "/api/v3/core/users/",
                get(move || {
                    let ambiguous = user_mode.clone();
                    async move {
                        let count = if ambiguous.load(std::sync::atomic::Ordering::SeqCst) {
                            2
                        } else {
                            1
                        };
                        Json(json!({"pagination":{"count":count},"results":[{
                            "uuid":USER,"is_active":true,"groups":[CHILD]
                        }]}))
                    }
                }),
            )
            .route(
                "/api/v3/core/groups/{id}/",
                get(|Path(id): Path<String>| async move { Json(json!({"pk":id})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = AuthentikCurrentIdentity {
            origin: reqwest::Url::parse(&origin).unwrap(),
            token: "disposable-token".into(),
            admin_group_uuid: Some(PARENT.into()),
            editor_group_uuid: Some(CHILD.into()),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            budget: Duration::from_secs(5),
        };
        assert!(provider.current(USER).await.is_err());
        ambiguous.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(provider.current(USER).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn authentik_group_traversal_has_one_total_deadline() {
        use axum::routing::get;
        use axum::{Json, Router};
        use serde_json::json;

        let app = Router::new()
            .route(
                "/api/v3/core/users/",
                get(|| async {
                    Json(json!({"pagination":{"count":1},"results":[{
                        "uuid":USER,"is_active":true,"groups":[CHILD]
                    }]}))
                }),
            )
            .route(
                "/api/v3/core/groups/{id}/",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Json(json!({"pk":CHILD,"parents":[]}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let provider = AuthentikCurrentIdentity {
            origin: reqwest::Url::parse(&origin).unwrap(),
            token: "disposable-token".into(),
            admin_group_uuid: Some(PARENT.into()),
            editor_group_uuid: Some(CHILD.into()),
            http: reqwest::Client::new(),
            budget: Duration::from_millis(20),
        };
        let started = std::time::Instant::now();
        assert!(provider.current(USER).await.is_err());
        assert!(started.elapsed() < Duration::from_millis(80));
        server.abort();
    }

    #[tokio::test]
    async fn authentik_refuses_redirect_before_sending_its_read_token_elsewhere() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use axum::http::{HeaderMap, StatusCode};
        use axum::routing::get;
        use axum::Router;

        let hits = std::sync::Arc::new(AtomicUsize::new(0));
        let redirected_hits = hits.clone();
        let destination = Router::new().route(
            "/api/v3/core/users/",
            get(move || {
                let hits = redirected_hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }
            }),
        );
        let destination_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination_url = format!(
            "http://{}/api/v3/core/users/",
            destination_listener.local_addr().unwrap()
        );
        let destination_task = tokio::spawn(async move {
            axum::serve(destination_listener, destination)
                .await
                .unwrap()
        });
        let redirect = Router::new().route(
            "/api/v3/core/users/",
            get(move || {
                let url = destination_url.clone();
                async move {
                    let mut headers = HeaderMap::new();
                    headers.insert(axum::http::header::LOCATION, url.parse().unwrap());
                    (StatusCode::FOUND, headers)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let redirect_task =
            tokio::spawn(async move { axum::serve(listener, redirect).await.unwrap() });
        let provider = AuthentikCurrentIdentity {
            origin: reqwest::Url::parse(&origin).unwrap(),
            token: "disposable-idp-read-token".into(),
            admin_group_uuid: Some(PARENT.into()),
            editor_group_uuid: Some(CHILD.into()),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            budget: Duration::from_secs(5),
        };
        assert!(provider.current(USER).await.is_err());
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        redirect_task.abort();
        destination_task.abort();
    }

    struct FakeAuthority {
        response: Mutex<Option<CurrentIdentity>>,
    }

    impl FakeAuthority {
        fn set(&self, response: Option<CurrentIdentity>) {
            *self.response.lock().unwrap() = response;
        }
    }

    #[async_trait]
    impl CurrentIdentityProvider for FakeAuthority {
        async fn current(&self, _: &str) -> Result<CurrentIdentity> {
            self.response
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| CoreError::Storage("fake identity API unavailable".into()))
        }

        async fn group_exists(&self, _: &str) -> Result<bool> {
            Ok(true)
        }
    }

    fn current(groups: Vec<&str>, roles: Vec<Role>) -> CurrentIdentity {
        CurrentIdentity {
            user_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            active: true,
            roles,
            groups: groups.into_iter().map(str::to_string).collect(),
        }
    }

    fn old_session() -> Principal {
        Principal {
            subject: "opaque-oidc-subject".into(),
            roles: vec![Role::Editor],
            groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
            user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
        }
    }

    #[tokio::test]
    async fn first_request_after_group_removal_refuses_group_and_derived_role_grants() {
        let authority = FakeAuthority {
            response: Mutex::new(Some(current(
                vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"],
                vec![Role::Editor],
            ))),
        };
        let stale = old_session();
        let group = SharedModelGrant::Group("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into());
        let role = SharedModelGrant::Role(Role::Editor);
        let fresh = refresh_for_shared(&authority, &stale).await.unwrap();
        assert!(group.allows(&fresh));
        assert!(role.allows(&fresh));

        authority.set(Some(current(vec![], vec![Role::Viewer])));
        let after_removal = refresh_for_shared(&authority, &stale).await.unwrap();
        assert!(!group.allows(&after_removal));
        assert!(!role.allows(&after_removal));
        assert!(group.allows(&stale), "session deliberately remains stale");
        assert!(role.allows(&stale));
    }

    #[tokio::test]
    async fn outage_inactive_mismatch_and_missing_signed_uuid_fail_closed() {
        let authority = FakeAuthority {
            response: Mutex::new(None),
        };
        let stale = old_session();
        assert!(refresh_for_shared(&authority, &stale).await.is_err());

        let mut inactive = current(vec![], vec![Role::Viewer]);
        inactive.active = false;
        authority.set(Some(inactive));
        assert!(refresh_for_shared(&authority, &stale).await.is_err());

        let mut mismatch = current(vec![], vec![Role::Viewer]);
        mismatch.user_uuid = "cccccccc-cccc-4ccc-8ccc-cccccccccccc".into();
        authority.set(Some(mismatch));
        assert!(refresh_for_shared(&authority, &stale).await.is_err());

        authority.set(Some(current(vec![], vec![])));
        assert!(refresh_for_shared(&authority, &stale).await.is_err());

        let mut legacy = stale;
        legacy.user_uuid = None;
        authority.set(Some(current(vec![], vec![Role::Viewer])));
        assert!(refresh_for_shared(&authority, &legacy).await.is_err());
    }
}
