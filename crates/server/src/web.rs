use std::sync::Arc;

use aster_core::{authorize, Action, CoreError, Notebook, Session, TableRef};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::{cookie, now, principal, ApiError, AppState};

const CSS: &str = r#"
:root{color-scheme:light dark}
body{font:14px/1.5 ui-sans-serif,system-ui,sans-serif;margin:0}
header{border-bottom:1px solid #8884;padding:.6rem 1rem;display:flex;gap:1rem;align-items:center}
a{color:inherit}
main{padding:1rem;max-width:60rem}
.cell{border:1px solid #8884;border-radius:6px;margin:1rem 0}
.cell>header{display:flex;gap:.5rem;align-items:center;border:0;border-bottom:1px solid #8884;padding:.4rem .6rem}
.cell input{flex:1;min-width:6rem}
textarea{width:100%;box-sizing:border-box;border:0;padding:.6rem;font:13px/1.5 ui-monospace,monospace;background:transparent;resize:vertical;min-height:4rem}
.out{overflow:auto;padding:0 .6rem .6rem}
table{border-collapse:collapse;font:12px/1.4 ui-monospace,monospace}
th,td{border:1px solid #8884;padding:.2rem .5rem;text-align:left}
button{cursor:pointer}
.status{color:#888}
"#;

const JS: &str = r#"
const nb = JSON.parse(document.getElementById('nb').textContent);
const status = document.getElementById('status');

function cellsFromDom() {
  return [...document.querySelectorAll('.cell')].map(el => ({
    id: el.dataset.id,
    sql: el.querySelector('textarea').value,
    engine: el.querySelector('.engine').value || null,
  }));
}

function resultTable(res) {
  const table = document.createElement('table');
  const head = document.createElement('tr');
  for (const col of res.columns) {
    const th = document.createElement('th');
    th.textContent = col.name;
    head.appendChild(th);
  }
  table.appendChild(head);
  for (const row of res.rows) {
    const tr = document.createElement('tr');
    for (const value of row) {
      const td = document.createElement('td');
      td.textContent = value === null ? '∅' : String(value);
      tr.appendChild(td);
    }
    table.appendChild(tr);
  }
  return table;
}

async function run(button) {
  const cell = button.closest('.cell');
  const out = cell.querySelector('.out');
  out.textContent = 'running…';
  const res = await fetch('/api/query', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ sql: cell.querySelector('textarea').value, engine: cell.querySelector('.engine').value || null }),
  });
  const data = await res.json();
  if (!res.ok) {
    out.textContent = 'error: ' + (data.error || res.status);
    return;
  }
  out.replaceChildren(resultTable(data));
}

async function generate(button) {
  const cell = button.closest('.cell');
  const area = cell.querySelector('textarea');
  const out = cell.querySelector('.out');
  out.textContent = 'asking the model…';
  const res = await fetch('/api/ai', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ prompt: 'Write a query for: ' + (nb.title || 'a report'), sql: area.value }),
  });
  const data = await res.json();
  if (!res.ok) {
    out.textContent = 'error: ' + (data.error || res.status);
    return;
  }
  area.value = data.sql;
  out.textContent = 'model suggestion inserted; run it to check.';
}

async function save() {
  const res = await fetch('/api/notebooks/' + encodeURIComponent(nb.id), {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ...nb, cells: cellsFromDom() }),
  });
  const data = await res.json().catch(() => ({}));
  status.textContent = res.ok ? 'saved ' + String(data.revision).slice(0, 8) : 'save failed: ' + res.status;
}

function addCell() {
  const section = document.createElement('section');
  section.className = 'cell';
  section.dataset.id = 'c' + Date.now().toString(36);
  section.innerHTML = '<header><code></code><input class="engine" placeholder="engine (default)"><button onclick="run(this)">Run</button><button onclick="generate(this)">AI</button></header><textarea spellcheck="false"></textarea><div class="out"></div>';
  section.querySelector('code').textContent = section.dataset.id;
  document.getElementById('cells').appendChild(section);
  section.querySelector('textarea').focus();
}

async function createNotebook() {
  const id = document.getElementById('new-id').value.trim();
  if (!id) return;
  await fetch('/api/notebooks/' + encodeURIComponent(id), {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ id, title: id, cells: [{ id: 'c1', sql: 'SELECT 1', engine: null }] }),
  });
  location.href = '/notebooks/' + encodeURIComponent(id);
}
"#;

/// ponytail: hand-rolled escaping for two static pages; add askama when a third
/// page or a designer owns the markup.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn layout(title: &str, body: &str) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    out.push_str("<title>");
    out.push_str(&escape(title));
    out.push_str("</title><style>");
    out.push_str(CSS);
    out.push_str("</style></head><body><header><strong>aster</strong>");
    out.push_str("<a href=\"/\">notebooks</a><a href=\"/catalog\">catalog</a>");
    out.push_str("<a href=\"/contracts\">contracts</a>");
    out.push_str("<a href=\"/settings/llm\">ai</a>");
    out.push_str("<span class=\"status\" id=\"status\"></span>");
    out.push_str("<a href=\"/logout\" style=\"margin-left:auto\">sign out</a>");
    out.push_str("</header><main>");
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
    let Some(principal) = browser_principal(&state, &headers) else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    let ids = state.notebooks.list(&principal.subject).await?;

    let mut items = String::new();
    for id in &ids {
        items.push_str(&format!(
            "<li><a href=\"/notebooks/{id}\">{id}</a></li>",
            id = escape(id)
        ));
    }
    if items.is_empty() {
        items.push_str("<li>no notebooks yet</li>");
    }

    let body = format!(
        "<p>signed in as <code>{subject}</code></p><h1>Notebooks</h1><ul>{items}</ul>\
         <h2>New notebook</h2><p><input id=\"new-id\" placeholder=\"notebook id\"> \
         <button onclick=\"createNotebook()\">Create</button></p>",
        subject = escape(&principal.subject),
    );
    Ok(Html(layout("aster", &body)).into_response())
}

pub async fn notebook_view(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers) else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;
    let notebook = state.notebooks.get(&id).await?;
    Ok(Html(layout(&notebook.title, &notebook_body(&notebook))).into_response())
}

/// Pages redirect a browser into the login flow; API routes answer 403 instead.
fn browser_principal(state: &AppState, headers: &HeaderMap) -> Option<aster_core::Principal> {
    principal(state, headers).ok()
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

fn notebook_body(notebook: &Notebook) -> String {
    let mut cells = String::new();
    for cell in &notebook.cells {
        let engine = cell
            .engine
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_default();
        cells.push_str(&format!(
            "<section class=\"cell\" data-id=\"{id}\"><header><code>{id}</code>\
             <input class=\"engine\" placeholder=\"engine (default)\" value=\"{engine}\">\
             <button onclick=\"run(this)\">Run</button>\
             <button onclick=\"generate(this)\">AI</button></header>\
             <textarea spellcheck=\"false\">{sql}</textarea><div class=\"out\"></div></section>",
            id = escape(&cell.id),
            engine = escape(&engine),
            sql = escape(&cell.sql),
        ));
    }

    // `<` is escaped so notebook text cannot close the script element.
    let json = serde_json::to_string(notebook)
        .unwrap_or_else(|_| "null".into())
        .replace('<', "\\u003c");

    format!(
        "<h1>{title}</h1><div id=\"cells\">{cells}</div>\
         <p><button onclick=\"addCell()\">Add cell</button> \
         <button onclick=\"save()\">Save</button></p>\
         <script type=\"application/json\" id=\"nb\">{json}</script>",
        title = escape(&notebook.title),
    )
}

const SESSION_COOKIE: &str = "aster_session";
const HANDSHAKE_COOKIE: &str = "aster_oidc";
const SESSION_TTL_SECONDS: i64 = 8 * 3600;

/// Starts the OIDC authorization-code flow, or falls back to the dev login form
/// when no identity provider is configured.
pub async fn login(State(state): State<Arc<AppState>>) -> Result<Response, ApiError> {
    let Some(oidc) = state.oidc.as_ref() else {
        return Ok(Redirect::to("/dev-login").into_response());
    };
    let handshake = oidc.handshake().await?;
    // state, PKCE verifier and nonce travel together in one signed cookie, so
    // the server keeps no per-login state between the two redirects.
    let sealed = state.sessions.seal(&format!(
        "{}:{}:{}",
        handshake.state, handshake.verifier, handshake.nonce
    ));
    let mut response = Redirect::to(&handshake.url).into_response();
    set_cookie(
        &mut response,
        &format!("{HANDSHAKE_COOKIE}={sealed}; Path=/; HttpOnly; SameSite=Lax; Max-Age=600"),
    );
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
    if let Some(error) = params.error {
        return Err(CoreError::Unauthorized(format!("identity provider error: {error}")).into());
    }
    let code = params
        .code
        .ok_or_else(|| CoreError::Invalid("missing authorization code".into()))?;
    let returned_state = params
        .state
        .ok_or_else(|| CoreError::Invalid("missing state".into()))?;

    let sealed = cookie(&headers, HANDSHAKE_COOKIE)
        .ok_or_else(|| CoreError::Unauthorized("login attempt expired".into()))?;
    let opened = state
        .sessions
        .open(&sealed)
        .ok_or_else(|| CoreError::Unauthorized("bad handshake cookie".into()))?;
    let mut parts = opened.splitn(3, ':');
    let expected_state = parts.next().unwrap_or_default();
    let verifier = parts.next().unwrap_or_default();
    let nonce = parts.next().unwrap_or_default();
    if expected_state.is_empty() || verifier.is_empty() || nonce.is_empty() {
        return Err(CoreError::Unauthorized("incomplete handshake cookie".into()).into());
    }
    if expected_state != returned_state {
        return Err(CoreError::Unauthorized("state mismatch".into()).into());
    }

    let oidc = state
        .oidc
        .as_ref()
        .ok_or_else(|| CoreError::NotFound("oidc not configured".into()))?;
    let (subject, roles) = oidc.complete(&code, verifier, nonce).await?;
    let token = state.sessions.encode(&Session {
        subject,
        roles,
        expires_at: now() + SESSION_TTL_SECONDS,
    })?;

    let mut response = Redirect::to("/").into_response();
    set_cookie(
        &mut response,
        &format!(
            "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECONDS}"
        ),
    );
    set_cookie(
        &mut response,
        &format!("{HANDSHAKE_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    Ok(response)
}

pub async fn logout() -> Response {
    let mut response = Redirect::to("/login").into_response();
    for name in [
        SESSION_COOKIE,
        HANDSHAKE_COOKIE,
        "aster_subject",
        "aster_roles",
    ] {
        set_cookie(
            &mut response,
            &format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
        );
    }
    response
}

pub async fn catalog(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers) else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;

    let mut body = String::from("<h1>Catalog</h1>");
    for catalog in state.catalogs.list() {
        let id = catalog.id().to_string();
        let health = catalog.health().await;
        body.push_str(&format!(
            "<h2>{id} <span class=\"status\">{health:?}</span></h2>",
            id = escape(&id)
        ));
        match catalog.list_namespaces().await {
            Ok(namespaces) if namespaces.is_empty() => {
                body.push_str("<p class=\"status\">no namespaces</p>")
            }
            Ok(namespaces) => {
                body.push_str("<ul>");
                for namespace in namespaces {
                    let url = format!(
                        "/catalog/{}/{namespace}",
                        escape(&id),
                        namespace = escape(&namespace.name)
                    );
                    body.push_str(&format!(
                        "<li><a href=\"{url}\">{name}</a></li>",
                        name = escape(&namespace.name)
                    ));
                }
                body.push_str("</ul>");
            }
            Err(error) => body.push_str(&format!(
                "<p class=\"status\">{}</p>",
                escape(&error.to_string())
            )),
        }
    }
    Ok(Html(layout("catalog", &body)).into_response())
}

pub async fn contracts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers) else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;

    let mut body = String::from("<h1>Data contracts</h1>");
    if state.contracts.is_empty() {
        body.push_str("<p class=\"status\">no contracts loaded</p>");
    }
    for contract in state.contracts.iter() {
        body.push_str(&format!(
            "<h2>{name} <span class=\"status\">{id}</span></h2>",
            name = escape(&contract.name),
            id = escape(&contract.id)
        ));
        if let Some(description) = &contract.description {
            body.push_str(&format!("<p>{}</p>", escape(description)));
        }
        if let Some(owner) = &contract.owner {
            body.push_str(&format!("<p class=\"status\">owner {}</p>", escape(owner)));
        }
        if !contract.fields.is_empty() {
            body.push_str("<table><tr><th>field</th><th>type</th><th>required</th></tr>");
            for field in &contract.fields {
                body.push_str(&format!(
                    "<tr><td>{name}</td><td>{data_type}</td><td>{required}</td></tr>",
                    name = escape(&field.name),
                    data_type = escape(field.data_type.as_deref().unwrap_or("")),
                    required = field.required
                ));
            }
            body.push_str("</table>");
        }
    }
    Ok(Html(layout("contracts", &body)).into_response())
}

pub async fn catalog_namespace(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, namespace)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers) else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;

    let catalog = state
        .catalogs
        .get(&aster_core::CatalogId::new(id.clone()))
        .ok_or_else(|| CoreError::NotFound("unknown catalog".into()))?;
    let tables = catalog.list_tables(&namespace).await?;

    let mut items = String::new();
    for table in &tables {
        let url = format!(
            "/catalog/{}/{}/{name}",
            escape(&id),
            escape(&namespace),
            name = escape(&table.name)
        );
        items.push_str(&format!(
            "<li><a href=\"{url}\">{name}</a></li>",
            name = escape(&table.name)
        ));
    }
    if items.is_empty() {
        items.push_str("<li>no tables</li>");
    }
    let body = format!(
        "<h1>{catalog} / {namespace}</h1><ul>{items}</ul>",
        catalog = escape(&id),
        namespace = escape(&namespace),
    );
    Ok(Html(layout(&format!("{id}/{namespace}"), &body)).into_response())
}

pub async fn catalog_table(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, namespace, table)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers) else {
        return Ok(sign_in_redirect());
    };
    authorize(&principal, Action::ReadNotebook)?;

    let catalog = state
        .catalogs
        .get(&aster_core::CatalogId::new(id.clone()))
        .ok_or_else(|| CoreError::NotFound("unknown catalog".into()))?;
    let schema = catalog
        .table_schema(&TableRef {
            namespace: namespace.clone(),
            name: table.clone(),
        })
        .await?;

    let mut rows = String::new();
    for column in &schema.columns {
        rows.push_str(&format!(
            "<tr><td>{name}</td><td>{data_type}</td><td>{nullable}</td></tr>",
            name = escape(&column.name),
            data_type = escape(&column.data_type),
            nullable = if column.nullable { "yes" } else { "no" },
        ));
    }
    let body = format!(
        "<h1>{catalog} / {namespace} / {table}</h1>\
         <table><tr><th>column</th><th>type</th><th>nullable</th></tr>{rows}</table>",
        catalog = escape(&id),
        namespace = escape(&namespace),
        table = escape(&table),
    );
    Ok(Html(layout(&format!("{id}/{namespace}/{table}"), &body)).into_response())
}

/// Registration form for the caller's own OpenAI-compatible endpoint. The token
/// is written once and never rendered back.
pub async fn llm_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(principal) = browser_principal(&state, &headers) else {
        return Ok(sign_in_redirect());
    };
    let current = state.llm.get(&principal.subject).await?;
    let (base_url, model, note) = match current {
        Some(config) => (
            config.base_url,
            config.model,
            "a token is already registered; submitting replaces it".to_string(),
        ),
        None => (
            String::new(),
            String::new(),
            "no endpoint registered".to_string(),
        ),
    };

    let body = format!(
        "<h1>AI endpoint</h1><p class=\"status\">{note}</p>\
         <form method=\"post\" action=\"/settings/llm\">\
         <p><label>base url <input name=\"base_url\" size=\"40\" value=\"{base_url}\" \
         placeholder=\"http://llm.local:4000/v1\"></label></p>\
         <p><label>model <input name=\"model\" size=\"30\" value=\"{model}\"></label></p>\
         <p><label>token <input name=\"api_key\" type=\"password\" size=\"40\"></label></p>\
         <p><button>Save</button></p></form>\
         <p class=\"status\">The token stays on the server; notebooks call it through \
         <code>/api/ai</code>.</p>",
        note = escape(&note),
        base_url = escape(&base_url),
        model = escape(&model),
    );
    Ok(Html(layout("ai endpoint", &body)).into_response())
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
