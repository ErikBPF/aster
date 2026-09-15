use std::sync::Arc;

use aster_core::{authorize, Action, Notebook};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::{principal_from_headers, ApiError, AppState};

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
  section.innerHTML = '<header><code></code><input class="engine" placeholder="engine (default)"><button onclick="run(this)">Run</button></header><textarea spellcheck="false"></textarea><div class="out"></div>';
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
    out.push_str("<a href=\"/\">notebooks</a><span class=\"status\" id=\"status\"></span>");
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
) -> Result<Html<String>, ApiError> {
    let principal = principal_from_headers(&headers)?;
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
    Ok(Html(layout("aster", &body)))
}

pub async fn notebook_view(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Html<String>, ApiError> {
    let principal = principal_from_headers(&headers)?;
    authorize(&principal, Action::ReadNotebook)?;
    let notebook = state.notebooks.get(&id).await?;
    Ok(Html(layout(&notebook.title, &notebook_body(&notebook))))
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
             <button onclick=\"run(this)\">Run</button></header>\
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

#[derive(serde::Deserialize)]
pub struct DevLogin {
    subject: Option<String>,
    roles: Option<String>,
}

/// Temporary pre-SSO convenience so a browser can act as a subject.
/// ponytail: dev-only, deleted when the OIDC session extractor lands in S2.
pub async fn dev_login(Query(params): Query<DevLogin>) -> Response {
    let subject = params.subject.unwrap_or_else(|| "alice".into());
    let roles = params.roles.unwrap_or_else(|| "editor".into());
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        format!("aster_subject={subject}; Path=/; HttpOnly; SameSite=Lax")
            .parse()
            .expect("valid cookie"),
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        format!("aster_roles={roles}; Path=/; HttpOnly; SameSite=Lax")
            .parse()
            .expect("valid cookie"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn escape_neutralizes_markup() {
        assert_eq!(
            escape("<script>alert(\"x\") & 'y'</script>"),
            "&lt;script&gt;alert(&quot;x&quot;) &amp; 'y'&lt;/script&gt;"
        );
    }
}
