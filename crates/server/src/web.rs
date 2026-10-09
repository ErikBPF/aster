use std::sync::Arc;

use aster_core::{
    authorize, Action, CoreError, Health, Notebook, NotebookStore, Principal, TableRef,
};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::{ai, cookie, now, principal, ApiError, AppState};

// Assets live beside the crate so the markup, the styles and the script stay
// readable; `include_str!` keeps them in the binary with no extra service.
const CSS: &str = include_str!("../assets/app.css");
const JS: &str = include_str!("../assets/app.js");

/// ponytail: hand-rolled escaping for a handful of pages; add askama when a
/// designer owns the markup.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn namespace_token(segments: &[String]) -> Result<String, CoreError> {
    if let Ok(name) = aster_core::catalog::legacy_namespace(segments) {
        if !name.starts_with('~') {
            return Ok(name);
        }
    }
    use base64::Engine;
    let bytes =
        serde_json::to_vec(segments).map_err(|_| CoreError::Invalid("invalid namespace".into()))?;
    Ok(format!(
        "~{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    ))
}

fn decode_namespace(token: &str) -> Result<Vec<String>, CoreError> {
    use base64::Engine;
    if let Some(encoded) = token.strip_prefix('~') {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| CoreError::Invalid("invalid namespace encoding".into()))?;
        let segments: Vec<String> = serde_json::from_slice(&bytes)
            .map_err(|_| CoreError::Invalid("invalid namespace array".into()))?;
        return aster_core::catalog::namespace_segments("", &segments);
    }
    aster_core::catalog::namespace_segments(token, &[])
}

fn catalog_path(parts: &[&str]) -> String {
    let mut url = reqwest::Url::parse("http://aster.invalid/catalog").expect("static URL");
    {
        let mut segments = url.path_segments_mut().expect("static URL path");
        for part in parts {
            segments.push(part);
        }
    }
    url.path().to_string()
}

fn nav(current: &str, items: &[(&str, &str, &str)]) -> String {
    items
        .iter()
        .map(|(id, href, label)| {
            if *id == current {
                format!("<a href=\"{href}\" aria-current=\"page\">{label}</a>")
            } else {
                format!("<a href=\"{href}\">{label}</a>")
            }
        })
        .collect()
}

fn health_badge(health: Health) -> String {
    let class = match health {
        Health::Healthy => "ok",
        Health::Degraded => "warn",
        _ => "bad",
    };
    format!(
        "<span class=\"badge {class}\">{label}</span>",
        label = health.as_str()
    )
}

/// Application shell: sticky topbar with the navigation, the caller's identity
/// and the page status line, then the page body.
fn layout(current: &str, title: &str, principal: &Principal, body: &str) -> String {
    let role = principal
        .roles
        .iter()
        .max()
        .map(|role| role.to_string())
        .unwrap_or_else(|| "viewer".into());
    let links = nav(
        current,
        &[
            ("notebooks", "/", "notebooks"),
            ("catalog", "/catalog", "catalog"),
            ("contracts", "/contracts", "contracts"),
            ("ai", "/settings/llm", "ai"),
        ],
    );
    let mut out = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
    );
    out.push_str(&format!("<title>{}</title>", escape(title)));
    out.push_str("<style>");
    out.push_str(CSS);
    out.push_str(
        "</style></head><body><header class=\"topbar\">\
         <a class=\"brand\" href=\"/\">aster</a>",
    );
    out.push_str(&format!("<nav class=\"nav\">{links}</nav>"));
    out.push_str(
        "<div class=\"spacer\"></div><div class=\"who\"><span class=\"status\" id=\"status\"></span>",
    );
    out.push_str(&format!(
        "<code class=\"subject\">{}</code><span class=\"chip\">{}</span>\
         <a class=\"btn ghost\" href=\"/logout\">sign out</a></div></header><main class=\"wrap\">",
        escape(&principal.subject),
        escape(&role),
    ));
    out.push_str(body);
    out.push_str("</main><script>");
    out.push_str(JS);
    out.push_str("</script></body></html>");
    out
}

pub async fn index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if !state.local_notebooks_enabled() {
        return Err(CoreError::Unauthorized("team notebook context required".into()).into());
    }
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    let ids = state.owned_notebooks(&principal.subject).await?;

    // Where the user left off, resolved from the shared state plane, so a
    // restart or a different container still offers the same place (D18).
    let resume = match state.user_state.get(&principal.subject).await? {
        Some(working) => match working.notebook {
            Some(notebook) if ids.iter().any(|id| id == &notebook) => {
                let cell = working
                    .cell
                    .map(|cell| format!(" at cell <code>{cell}</code>", cell = escape(&cell)))
                    .unwrap_or_default();
                format!(
                    "<div class=\"panel\"><div class=\"panel-body\">pick up where you left off: \
                     <a href=\"/notebooks/{id}\">{id}</a>{cell}</div></div>",
                    id = escape(&notebook)
                )
            }
            _ => String::new(),
        },
        None => String::new(),
    };

    let mut cards = String::new();
    for id in &ids {
        let id = escape(id);
        cards.push_str(&format!(
            "<a class=\"card\" href=\"/notebooks/{id}\"><h3>{id}</h3>\
             <span class=\"status\">open notebook</span></a>"
        ));
    }
    let notebooks = if cards.is_empty() {
        "<div class=\"empty\">no notebooks yet — create one to get started</div>".to_string()
    } else {
        format!("<div class=\"grid\">{cards}</div>")
    };

    let body = format!(
        "<div class=\"page-head\"><div><h1>Notebooks</h1>\
         <p class=\"status\">signed in as <code>{subject}</code></p></div>\
         <div class=\"spacer\"></div>\
         <form id=\"create\"><input id=\"new-id\" placeholder=\"new notebook id\" required> \
         <button class=\"btn primary\">Create</button></form></div>{resume}{notebooks}",
        subject = escape(&principal.subject),
    );
    Ok(Html(layout("notebooks", "aster", &principal, &body)).into_response())
}

pub async fn notebook_view(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if !state.local_notebooks_enabled() {
        return Err(CoreError::Unauthorized("team notebook context required".into()).into());
    }
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    let snapshot = state
        .owned_notebook_snapshot(&id, &principal.subject)
        .await?;
    let body = notebook_body(&snapshot.notebook, &snapshot.content_revision, None);
    Ok(Html(layout(
        "notebooks",
        &snapshot.notebook.title,
        &principal,
        &body,
    ))
    .into_response())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamNotebookQuery {
    workspace: Option<String>,
}

pub async fn team_notebook_view(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, id)): Path<(String, String)>,
    Query(query): Query<TeamNotebookQuery>,
) -> Result<Response, ApiError> {
    let personal = match query.workspace.as_deref() {
        None | Some("session") => false,
        Some("personal") => true,
        _ => return Err(CoreError::Invalid("invalid workspace selection".into()).into()),
    };
    let (principal, sid) = crate::team_member_session(&state, &headers, &team).await?;
    authorize(&principal, Action::ReadNotebook)?;
    let workspaces = state
        .team_workspaces
        .as_ref()
        .ok_or_else(|| CoreError::NotFound("team notebooks are disabled".into()))?;
    let store = workspaces.workspace(&team, &principal, &sid, personal)?;
    let snapshot = store.snapshot(&id).await?;
    let workspace = if personal { "personal" } else { "session" };
    let body = notebook_body(
        &snapshot.notebook,
        &snapshot.content_revision,
        Some((&team, workspace)),
    );
    Ok(Html(layout(
        "notebooks",
        &snapshot.notebook.title,
        &principal,
        &body,
    ))
    .into_response())
}

/// Pages redirect a browser into the login flow; API routes answer 403 instead.
async fn browser_principal(state: &AppState, headers: &HeaderMap) -> Option<aster_core::Principal> {
    principal(state, headers).await.ok()
}

fn sign_in_redirect() -> Response {
    Redirect::to("/login").into_response()
}

fn set_cookie(response: &mut Response, value: &str) {
    response.headers_mut().append(
        header::SET_COOKIE,
        value.parse().expect("valid Set-Cookie value"),
    );
}

fn notebook_body(
    notebook: &Notebook,
    content_revision: &str,
    team_context: Option<(&str, &str)>,
) -> String {
    let mut cells = String::new();
    for cell in &notebook.cells {
        let engine = cell
            .engine
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_default();
        cells.push_str(&format!(
            "<section class=\"cell\" data-id=\"{id}\">\
             <div class=\"cell-gutter\"><span class=\"prompt in\">In&nbsp;[&nbsp;]</span></div>\
             <div class=\"cell-body\">\
             <div class=\"cell-head\"><code class=\"cell-id\">{id}</code>\
             <div class=\"cell-actions\">\
             <button type=\"button\" class=\"btn btn-run\" data-action=\"run\">▶ Run</button>\
             <button type=\"button\" class=\"btn\" data-action=\"ai\">AI</button>\
             <button type=\"button\" class=\"btn ghost\" data-action=\"add\">+</button>\
             <button type=\"button\" class=\"btn ghost\" data-action=\"up\">↑</button>\
             <button type=\"button\" class=\"btn ghost\" data-action=\"down\">↓</button>\
             <button type=\"button\" class=\"btn ghost danger\" data-action=\"delete\">✕</button>\
             </div>\
             <div class=\"select-wrap\"><span class=\"muted\">engine</span>\
             <select class=\"engine\" data-current=\"{engine}\"></select></div></div>\
             <div class=\"cell-split\">\
             <div class=\"cell-main\">\
             <textarea class=\"editor\" spellcheck=\"false\">{sql}</textarea>\
             <div class=\"out\"><span class=\"prompt out\">Out&nbsp;[&nbsp;]</span>\
             <div class=\"out-content\"></div></div></div>\
             <aside class=\"cell-chat\" aria-label=\"Cell conversation\" hidden>\
             <div class=\"chat-heading\"><strong>Cell assistant</strong>\
             <button type=\"button\" class=\"btn cell-chat-close\">Close</button></div>\
             <p class=\"status\">For this cell only. Sends this cell's SQL and last output; \
             it cannot read the notebook conversation or another cell's.</p>\
             <div class=\"cell-chat-history chat-history\" tabindex=\"0\" role=\"log\" \
             aria-label=\"Cell conversation messages\" aria-live=\"polite\"></div>\
             <p class=\"cell-chat-status status\" role=\"status\"></p>\
             <form class=\"cell-chat-form\"><label>Message</label>\
             <textarea class=\"cell-chat-prompt\" rows=\"3\" maxlength=\"8192\" required \
             placeholder=\"Ask about this query\"></textarea>\
             <button type=\"submit\" class=\"btn primary cell-chat-send\">Send</button></form></aside>\
             </div></div></section>",
            id = escape(&cell.id),
            engine = escape(&engine),
            sql = escape(&cell.sql),
        ));
    }

    // `<` is escaped so notebook text cannot close the script element.
    let json = serde_json::to_string(notebook)
        .unwrap_or_else(|_| "null".into())
        .replace('<', "\\u003c");

    let (crumb, sync_button, context_attributes) = match team_context {
        Some((team, workspace)) => {
            let path = format!("/teams/{}/notebooks/{}", escape(team), escape(&notebook.id));
            (
                format!(
                    "team {} / <a href=\"{path}?workspace=session\">Session</a> · \
                     <a href=\"{path}?workspace=personal\">Personal</a> /",
                    escape(team)
                ),
                "<button type=\"button\" class=\"btn\" data-action=\"sync\">Sync</button>",
                format!(
                    " data-team=\"{}\" data-workspace=\"{}\"",
                    escape(team),
                    escape(workspace)
                ),
            )
        }
        None => (
            "<a href=\"/\">notebooks</a> /".to_owned(),
            "",
            String::new(),
        ),
    };
    format!(
        "<div class=\"page-head\"><div>\
         <p class=\"crumb\">{crumb} {id}</p>\
         <h1>{title}</h1></div></div>\
         <div class=\"nb-bar\">\
         <button type=\"button\" class=\"btn\" data-action=\"add\">+ Cell</button>\
         <button type=\"button\" class=\"btn\" data-action=\"run-all\">▶ Run all</button>\
         <button type=\"button\" class=\"btn primary\" data-action=\"save\">Save</button>\
         {sync_button}\
         <button type=\"button\" class=\"btn\" id=\"chat-toggle\" aria-controls=\"chat-panel\" aria-expanded=\"false\">Chat</button>\
         <div class=\"spacer\"></div>\
         <label class=\"helper\">helper\
         <span class=\"select-wrap\"><select class=\"helper-select\" id=\"helper\"></select></span>\
         </label>\
         </div>\
         <p class=\"status\">run with <span class=\"kbd\">⌘/Ctrl</span> <span class=\"kbd\">Enter</span>, \
         run and move on with <span class=\"kbd\">Shift</span> <span class=\"kbd\">Enter</span>, \
         save with <span class=\"kbd\">⌘/Ctrl</span> <span class=\"kbd\">S</span></p>\
         <details><summary>Contract context for manual query review</summary>{contract_context}</details>\
         <div class=\"cells\" id=\"cells\">{cells}</div>\
         <aside id=\"chat-panel\" class=\"chat-panel\" aria-label=\"Notebook conversation\" hidden>\
         <div class=\"chat-heading\"><strong>Notebook assistant</strong>\
         <button type=\"button\" class=\"btn\" id=\"chat-expand\" aria-pressed=\"false\">Expand</button>\
         <button type=\"button\" class=\"btn\" id=\"chat-close\">Close</button></div>\
         <p class=\"status\">Private to you and this notebook. Changing helper sends this history to that helper.</p>\
         <div id=\"chat-history\" class=\"chat-history\" tabindex=\"0\" role=\"log\" aria-label=\"Conversation messages\" aria-live=\"polite\"></div>\
         <p id=\"chat-status\" role=\"status\" class=\"status\"></p>\
         <form id=\"chat-form\"><label for=\"chat-context\">Attached cell SQL (optional)</label>\
         <textarea id=\"chat-context\" rows=\"2\" maxlength=\"8192\"></textarea>\
         <label for=\"chat-prompt\">Message</label>\
         <textarea id=\"chat-prompt\" rows=\"3\" maxlength=\"8192\" required placeholder=\"Ask a question or refine a query\"></textarea>\
         <button type=\"submit\" class=\"btn primary\" id=\"chat-send\">Send</button></form></aside>\
         <script type=\"application/json\" id=\"nb\" data-content-revision=\"{content_revision}\"{context_attributes}>{json}</script>",
        crumb = crumb,
        sync_button = sync_button,
        context_attributes = context_attributes,
        id = escape(&notebook.id),
        title = escape(&notebook.title),
        content_revision = escape(content_revision),
        contract_context = contract_context(),
    )
}

const SESSION_COOKIE: &str = "aster_session";

// A pending browser login lasts at most five minutes; a shorter handshake-store
// TTL still expires it server-side. This cookie is host-only (no Domain).
const OIDC_COOKIE_MAX_AGE: i64 = 300;

fn secure_cookies(state: &AppState) -> bool {
    state.identity.as_ref().is_some_and(|identity| {
        identity
            .callback_uri()
            .and_then(|uri| reqwest::Url::parse(uri).ok())
            .is_none_or(|uri| uri.scheme() != "http")
    })
}

fn oidc_cookie_name(state: &AppState) -> &'static str {
    if secure_cookies(state) {
        "__Host-aster_oidc_state"
    } else {
        "aster_oidc_state"
    }
}

fn set_oidc_cookie(response: &mut Response, state: &AppState, value: &str, max_age: i64) {
    let name = oidc_cookie_name(state);
    let secure = if secure_cookies(state) {
        "; Secure"
    } else {
        ""
    };
    set_cookie(
        response,
        &format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age={max_age}"),
    );
}

/// Starts the authorization-code flow, or falls back to the dev login form when
/// no identity provider is configured.
pub async fn login(State(state): State<Arc<AppState>>) -> Result<Response, ApiError> {
    let Some(identity) = state.identity.as_ref() else {
        return Ok(Redirect::to("/dev-login").into_response());
    };
    let handshake = identity.begin().await?;
    if !safe_cookie_value(&handshake.state) {
        return Err(CoreError::Invalid("invalid login state".into()).into());
    }
    // The PKCE verifier and nonce wait in the shared store, keyed by the
    // provider's state parameter, and are redeemed exactly once by the callback
    // (D20).
    state
        .handshakes
        .put(
            &handshake.state,
            &format!("{}:{}", handshake.verifier, handshake.nonce),
            now(),
        )
        .await?;
    let mut response = Redirect::to(&handshake.url).into_response();
    set_oidc_cookie(&mut response, &state, &handshake.state, OIDC_COOKIE_MAX_AGE);
    Ok(response)
}

#[derive(serde::Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn callback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<CallbackQuery>,
) -> Result<Response, ApiError> {
    let returned_state = params
        .state
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("missing state".into()))?;
    let name = oidc_cookie_name(&state);
    let bindings: Vec<_> = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .filter(|(key, _)| *key == name)
        .map(|(_, value)| value)
        .collect();
    if returned_state.is_empty() || bindings.as_slice() != [returned_state] {
        // A foreign/malformed callback must neither consume the real handshake
        // nor clear a different pending login in the receiving browser.
        return Err(
            CoreError::Unauthorized("login browser binding missing or invalid".into()).into(),
        );
    }
    let mut response = redeem_callback(&state, &headers, params)
        .await
        .into_response();
    set_oidc_cookie(&mut response, &state, "", 0);
    Ok(response)
}

async fn redeem_callback(
    state: &AppState,
    headers: &HeaderMap,
    params: CallbackQuery,
) -> Result<Response, ApiError> {
    // Single use: the entry is gone whether or not the exchange succeeds, so a
    // replayed callback cannot mint a second session. Browser binding has already
    // been checked; the store's atomic take remains the cross-replica replay gate.
    let payload = state
        .handshakes
        .take(
            params.state.as_deref().expect("binding checked state"),
            now(),
        )
        .await?
        .ok_or_else(|| CoreError::Unauthorized("login attempt expired or already used".into()))?;
    if params.error.is_some() {
        tracing::warn!("identity provider refused the login");
        return Err(CoreError::Unauthorized("login refused".into()).into());
    }
    let code = params
        .code
        .ok_or_else(|| CoreError::Invalid("missing authorization code".into()))?;
    let (verifier, nonce) = payload
        .split_once(':')
        .ok_or_else(|| CoreError::Unauthorized("incomplete handshake payload".into()))?;

    let identity_provider = state
        .identity
        .as_ref()
        .ok_or_else(|| CoreError::NotFound("identity provider not configured".into()))?;
    let identity = identity_provider.complete(&code, verifier, nonce).await?;
    let record = state
        .sessions
        .create_verified(
            &identity,
            headers
                .get(header::USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_string()),
            now(),
        )
        .await?;

    let mut response = Redirect::to("/").into_response();
    let secure = if secure_cookies(state) {
        "; Secure"
    } else {
        ""
    };
    set_cookie(
        &mut response,
        &format!(
            "{SESSION_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age={}",
            record.sid, state.session_ttl_seconds
        ),
    );
    Ok(response)
}

pub async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(sid) = cookie(&headers, SESSION_COOKIE) {
        // Revocation is best effort: the caller still gets logged out locally if
        // the store is unreachable, and the failure is visible in the log.
        if let Err(error) = state.sessions.revoke(&sid).await {
            tracing::error!(%error, "session revoke failed");
        }
    }
    let mut response = Redirect::to("/login").into_response();
    let secure = if secure_cookies(&state) {
        "; Secure"
    } else {
        ""
    };
    for name in [SESSION_COOKIE, "aster_subject", "aster_roles"] {
        set_cookie(
            &mut response,
            &format!("{name}=; Path=/; HttpOnly; SameSite=Lax{secure}; Max-Age=0"),
        );
    }
    set_oidc_cookie(&mut response, &state, "", 0);
    response
}

pub async fn catalog(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    let mut body = String::from("<section class=\"panel\" aria-label=\"Physical inventory\"><h1>Catalog inventory</h1><p>Physical presence, admitted contracts and declared semantics are independent. Query-target authorization is not yet enforced by this inventory.</p>");
    match crate::catalog_inventory::all(&state, &headers).await {
        Ok(inventories) => {
            for inventory in inventories {
                body.push_str(&format!(
                    "<h2>{}</h2>",
                    escape(&format!(
                        "{} / {}",
                        inventory["catalog"], inventory["namespaceSegments"]
                    ))
                ));
                if inventory["physical"]["status"] == "unknown" {
                    body.push_str("<p role=\"status\">Physical inventory unavailable; no accessible entries confirmed.</p>");
                    continue;
                }
                body.push_str("<table class=\"schema\"><tr><th>Object</th><th>Physical</th><th>Contract</th><th>Semantics</th></tr>");
                for entry in inventory["entries"].as_array().into_iter().flatten() {
                    body.push_str(&format!(
                        "<tr><td>{}</td><td>present</td><td>{}</td><td>{}</td></tr>",
                        escape(entry["physicalName"].as_str().unwrap_or("")),
                        escape(entry["contract"]["status"].as_str().unwrap_or("unknown")),
                        escape(entry["semantics"]["status"].as_str().unwrap_or("unknown"))
                    ));
                }
                body.push_str("</table>");
            }
        }
        Err(error) => {
            tracing::warn!(%error, "inventory unavailable");
            body.push_str("<p role=\"status\">Inventory unavailable: current identity and configured schema ownership are required.</p>");
        }
    }
    body.push_str("</section><details open><summary>Authorized contract artifacts and query context (not physical inventory)</summary>");
    body.push_str(&contract_context());
    body.push_str("</details>");
    Ok(Html(layout(
        "catalog",
        "contracts and query context",
        &principal,
        &body,
    ))
    .into_response())
}

fn contract_context() -> String {
    "<section id=\"contract-context\" class=\"panel\" aria-label=\"Contract query context\">\
     <div class=\"panel-body\"><h1>Contracts and query context</h1>\
     <p>Start with declared meaning from compiled ODCS 3.2 contracts. Observation and execution require separate access.</p>\
     <label for=\"contract-selection\">Declared contract</label>\
     <select id=\"contract-selection\"><option value=\"\">Select a contract</option></select>\
     <button type=\"button\" class=\"btn\" id=\"contract-refresh\">Refresh contracts</button>\
     <p id=\"contract-status\" role=\"status\"></p>\
     <details><summary>Full declared document and provenance</summary><pre id=\"contract-document\"></pre></details>\
     <label for=\"contract-object\">Declared object</label>\
     <select id=\"contract-object\" disabled><option value=\"\">Select an object</option></select>\
     <div id=\"contract-preparation\" aria-live=\"polite\"></div>\
     <label for=\"contract-draft\">Query draft</label>\
     <textarea id=\"contract-draft\" rows=\"6\" spellcheck=\"false\"></textarea>\
     <button type=\"button\" class=\"btn\" id=\"contract-review-button\" disabled>Review draft with context</button>\
     <p>Review is manual: it does not validate SQL or execute it. No joins, aggregates or physical identifiers are inferred. \
     AI contract assistance and exports are not available here.</p>\
     <div id=\"contract-review\" aria-live=\"polite\"></div>\
     <a href=\"/catalog/physical\">Physical catalogs (expert observation)</a></div></section>".into()
}

pub async fn physical_catalog(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    if state.compiled_contracts.is_some() {
        return Ok(Redirect::to("/catalog").into_response());
    }

    let mut body = String::from(
        "<div class=\"page-head\"><div><h1>Catalog</h1>\
         <p class=\"status\">namespaces exposed by each registered metastore</p></div></div>",
    );
    if !state
        .catalogs
        .list()
        .iter()
        .any(|entry| crate::catalog_metadata_visible(&state, &entry.id().0))
    {
        body.push_str("<div class=\"empty\">no catalogs registered</div>");
    }
    for catalog in state.catalogs.list() {
        if !crate::catalog_inventory::visible(&state, &headers, &catalog.id().0).await {
            continue;
        }
        let id = catalog.id().to_string();
        let health = catalog.health().await;
        body.push_str(&format!(
            "<div class=\"panel\"><div class=\"panel-head\"><strong>{id}</strong>\
             <span class=\"chip\">{kind}</span>{health}</div><div class=\"panel-body\">",
            id = escape(&id),
            kind = escape(catalog.kind()),
            health = health_badge(health),
        ));
        match crate::catalog_inventory::namespaces(&state, &headers, &id).await {
            Ok(namespaces) if namespaces.is_empty() => {
                body.push_str("<p class=\"status\">no namespaces</p>")
            }
            Ok(namespaces) => {
                body.push_str("<ul class=\"list\">");
                for namespace in namespaces {
                    let segments = aster_core::catalog::namespace_segments(
                        &namespace.name,
                        &namespace.segments,
                    )?;
                    let token = namespace_token(&segments)?;
                    let url = escape(&catalog_path(&[&id, &token]));
                    body.push_str(&format!(
                        "<li><a href=\"{url}\">{name}</a></li>",
                        name = escape(&namespace.name)
                    ));
                }
                body.push_str("</ul>");
            }
            Err(error) => {
                tracing::warn!(%error, "physical inventory unavailable");
                body.push_str("<p class=\"status\">Physical inventory unavailable</p>");
            }
        }
        body.push_str("</div></div>");
    }
    Ok(Html(layout("catalog", "catalog", &principal, &body)).into_response())
}

pub async fn contracts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    if state.compiled_contracts.is_some() {
        return Ok(Html(layout(
            "contracts",
            "contracts and query context",
            &principal,
            &contract_context(),
        ))
        .into_response());
    }
    crate::require_unscoped_metadata(&state)?;

    let mut body = String::from(
        "<div class=\"page-head\"><div><h1>Data contracts</h1>\
         <p class=\"status\">declarations the assistant may quote when a cell names them</p>\
         </div></div>",
    );
    if state.contracts.is_empty() {
        body.push_str("<div class=\"empty\">no contracts loaded</div>");
    }
    for contract in state.contracts.iter() {
        body.push_str(&format!(
            "<div class=\"panel\"><div class=\"panel-head\"><strong>{name}</strong>\
             <span class=\"chip\">{id}</span></div><div class=\"panel-body\">",
            name = escape(&contract.name),
            id = escape(&contract.id),
        ));
        if let Some(description) = &contract.description {
            body.push_str(&format!("<p>{}</p>", escape(description)));
        }
        if let Some(owner) = &contract.owner {
            body.push_str(&format!("<p class=\"status\">owner {}</p>", escape(owner)));
        }
        if !contract.fields.is_empty() {
            body.push_str(
                "<table class=\"schema\"><tr><th>field</th><th>type</th><th>required</th></tr>",
            );
            for field in &contract.fields {
                body.push_str(&format!(
                    "<tr><td><code>{name}</code></td><td class=\"muted\">{data_type}</td>\
                     <td class=\"muted\">{required}</td></tr>",
                    name = escape(&field.name),
                    data_type = escape(field.data_type.as_deref().unwrap_or("")),
                    required = if field.required { "yes" } else { "no" },
                ));
            }
            body.push_str("</table>");
        }
        body.push_str("</div></div>");
    }
    Ok(Html(layout("contracts", "contracts", &principal, &body)).into_response())
}

pub async fn catalog_namespace(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, namespace)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    crate::require_catalog_metadata(&state, &id)?;

    let segments = decode_namespace(&namespace)?;
    let tables = crate::catalog_inventory::tables(&state, &headers, &id, &segments).await?;

    let mut items = String::new();
    for descriptor in &tables {
        let table = &descriptor.table;
        let url = escape(&catalog_path(&[&id, &namespace, &table.name]));
        items.push_str(&format!(
            "<li data-name=\"{name}\"><a href=\"{url}\">{name}</a> \
             <span class=\"muted\">{format}</span></li>",
            name = escape(&table.name),
            format = escape(descriptor.format.as_deref().unwrap_or("format unspecified")),
        ));
    }
    if items.is_empty() {
        items.push_str("<li class=\"status\">no tables</li>");
    }
    let body = format!(
        "<p class=\"crumb\"><a href=\"/catalog\">catalog</a> / {catalog}</p>\
         <div class=\"page-head\"><div><h1>{namespace}</h1>\
         <p class=\"status\">{count} table(s)</p></div><div class=\"spacer\"></div>\
         <input placeholder=\"filter tables\" oninput=\"filterList(this)\"></div>\
         <div class=\"panel\"><ul class=\"list\">{items}</ul></div>",
        catalog = escape(&id),
        namespace = escape(&namespace),
        count = tables.len(),
    );
    Ok(Html(layout(
        "catalog",
        &format!("{id}/{namespace}"),
        &principal,
        &body,
    ))
    .into_response())
}

pub async fn catalog_table(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, namespace, table)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    crate::require_catalog_metadata(&state, &id)?;

    let catalog = state
        .catalogs
        .get(&aster_core::CatalogId::new(id.clone()))
        .ok_or_else(|| CoreError::NotFound("unknown catalog".into()))?;
    let table_ref = TableRef {
        namespace_segments: decode_namespace(&namespace)?,
        namespace: decode_namespace(&namespace)?.join("."),
        name: table.clone(),
    };
    let descriptor =
        crate::catalog_inventory::tables(&state, &headers, &id, &table_ref.namespace_segments)
            .await?
            .into_iter()
            .find(|entry| entry.table.name == table)
            .ok_or_else(|| CoreError::NotFound("unknown table".into()))?;
    let namespace_url = escape(&catalog_path(&[&id, &namespace]));
    if !descriptor.schema_available {
        let body = format!(
            "<p class=\"crumb\"><a href=\"{namespace_url}\">{namespace}</a></p>\
             <div class=\"page-head\"><h1>{table}</h1></div>\
             <div class=\"panel\"><p>Format: {format}</p>\
             <p>Base location: {location}</p>\
             <p>Schema unavailable. Data access has not been validated.</p></div>",
            namespace_url = namespace_url,
            namespace = escape(&namespace),
            table = escape(&table),
            format = escape(descriptor.format.as_deref().unwrap_or("unspecified")),
            location = escape(
                descriptor
                    .base_location
                    .as_deref()
                    .unwrap_or("not provided")
            ),
        );
        return Ok(Html(layout(
            "catalog",
            &format!("{id}/{namespace}/{table}"),
            &principal,
            &body,
        ))
        .into_response());
    }
    let schema = catalog.table_schema(&table_ref).await?;

    let mut rows = String::new();
    for column in &schema.columns {
        rows.push_str(&format!(
            "<tr><td><code>{name}</code></td><td class=\"muted\">{data_type}</td>\
             <td class=\"muted\">{nullable}</td></tr>",
            name = escape(&column.name),
            data_type = escape(&column.data_type),
            nullable = if column.nullable { "yes" } else { "no" },
        ));
    }
    let body = format!(
        "<p class=\"crumb\"><a href=\"/catalog\">catalog</a> / \
         <a href=\"{namespace_url}\">{namespace}</a></p>\
         <div class=\"page-head\"><div><h1>{table}</h1>\
         <p class=\"status\">{count} column(s)</p></div>\
         <div class=\"spacer\"></div>\
         <a class=\"btn\" href=\"{namespace_url}\">back to tables</a></div>\
         <div class=\"panel\"><table class=\"schema\">\
         <tr><th>column</th><th>type</th><th>nullable</th></tr>{rows}</table></div>",
        namespace_url = namespace_url,
        namespace = escape(&namespace),
        table = escape(&table),
        count = schema.columns.len(),
    );
    Ok(Html(layout(
        "catalog",
        &format!("{id}/{namespace}/{table}"),
        &principal,
        &body,
    ))
    .into_response())
}

/// Registration list for the caller's own OpenAI-compatible endpoints. A token is
/// written once and never rendered back.
pub async fn llm_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers).await else {
        return Ok(sign_in_redirect());
    };
    let helpers = state.llm.list(&principal.subject).await?;

    let mut list = String::new();
    for helper in &helpers {
        list.push_str(&format!(
            "<li><div><code>{id}</code> <span class=\"muted\">{base_url}</span> \
             <span class=\"badge\">{model}</span></div>\
             <form method=\"post\" action=\"/settings/llm/{id}/delete\">\
             <button class=\"btn ghost danger\">remove</button></form></li>",
            id = escape(&helper.id),
            base_url = escape(ai::safe_base_url(&helper.base_url)),
            model = escape(&helper.model),
        ));
    }
    let registered = if helpers.is_empty() {
        "<p class=\"status\">no endpoint registered</p>".to_string()
    } else {
        format!("<ul class=\"list\">{list}</ul>")
    };

    let body = format!(
        "<div class=\"page-head\"><div><h1>AI helpers</h1>\
         <p class=\"status\">each helper is one OpenAI-compatible endpoint; a notebook picks one</p>\
         </div></div>\
         {registered}\
         <form class=\"panel\" method=\"post\" action=\"/settings/llm\">\
         <div class=\"panel-head\">add or replace a helper</div>\
         <div class=\"panel-body\">\
         <p><label>name<br><input name=\"id\" size=\"24\" value=\"\" \
         placeholder=\"lab-qwen\"></label></p>\
         <p><label>base url<br><input name=\"base_url\" size=\"48\" value=\"\" \
         placeholder=\"http://llm.local:4000/v1\"></label></p>\
         <p><label>model<br><input name=\"model\" size=\"36\" value=\"\"></label></p>\
         <p><label>token<br><input name=\"api_key\" type=\"password\" size=\"48\"></label></p>\
         <p><button class=\"btn primary\">Save</button></p>\
         <p class=\"status\">The token stays on the server; notebooks call it through \
         <code>/api/ai</code>. Submitting an existing name replaces that helper.</p></div></form>",
    );
    Ok(Html(layout("ai", "ai helpers", &principal, &body)).into_response())
}

#[derive(serde::Deserialize)]
pub struct DevLogin {
    subject: Option<String>,
    roles: Option<String>,
}

/// Temporary pre-SSO convenience so a browser can act as a subject.
/// ponytail: dev-only, deleted when the SSO session extractor replaces it.
pub async fn dev_login(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DevLogin>,
) -> Response {
    if !state.dev_login {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }
    let subject = params.subject.unwrap_or_else(|| "alice".into());
    let roles = params.roles.unwrap_or_else(|| "editor".into());
    if !safe_cookie_value(&subject) || !safe_cookie_value(&roles) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "invalid subject or roles",
        )
            .into_response();
    }
    let mut response = Redirect::to("/").into_response();
    for (name, value) in [("aster_subject", &subject), ("aster_roles", &roles)] {
        let cookie = format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax");
        match cookie.parse() {
            Ok(parsed) => {
                response.headers_mut().append(header::SET_COOKIE, parsed);
            }
            Err(_) => {
                return (axum::http::StatusCode::BAD_REQUEST, "invalid cookie value")
                    .into_response()
            }
        }
    }
    response
}

/// Cookie values travel in a header, so keep them to a conservative charset:
/// anything else could smuggle header syntax (CR/LF) into the response.
fn safe_cookie_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | ':'))
}

#[cfg(test)]
mod tests {
    use super::{escape, safe_cookie_value};

    #[test]
    fn escape_neutralizes_markup() {
        assert_eq!(
            escape("<script>alert(\"x\") & 'y'</script>"),
            "&lt;script&gt;alert(&quot;x&quot;) &amp; 'y'&lt;/script&gt;"
        );
    }

    #[test]
    fn cookie_values_reject_header_smuggling() {
        assert!(safe_cookie_value("alice"));
        assert!(safe_cookie_value("alice@example.com"));
        assert!(!safe_cookie_value("alice\r\nSet-Cookie: x=1"));
        assert!(!safe_cookie_value(""));
        assert!(!safe_cookie_value(&"a".repeat(65)));
    }
}
