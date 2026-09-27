// Client for the server-rendered shell. Every page shares the nav and the
// status line, so every entry point tolerates the elements it needs being
// absent.

const nbNode = document.getElementById('nb');
const nb = nbNode ? JSON.parse(nbNode.textContent) : null;
const team = nbNode?.dataset.team || null;
const workspace = nbNode?.dataset.workspace || 'session';
let contentRevision = nbNode?.dataset.contentRevision || null;
const statusLine = document.getElementById('status');
const cellsRoot = document.getElementById('cells');
const helperSession = Array.from(crypto.getRandomValues(new Uint8Array(16)),
  byte => byte.toString(16).padStart(2, '0')).join('');


let dirty = false;
let editGeneration = 0;
let engineInfo = new Map();

function status(text) {
  if (statusLine) statusLine.textContent = text;
}

function setDirty(value) {
  if (value) editGeneration += 1;
  dirty = value;
  const syncButton = document.querySelector('[data-action="sync"]');
  if (syncButton) syncButton.disabled = dirty;
  if (dirty) status('unsaved changes');
  else if (statusLine) statusLine.textContent = '';
}

/* ---- engines -------------------------------------------------------- */

async function loadEngines() {
  const res = await fetch('/api/engines');
  if (!res.ok) return;
  const list = await res.json();
  engineInfo = new Map(list.map((engine) => [engine.id, engine]));
  for (const select of document.querySelectorAll('select.engine')) {
    const current = select.dataset.current || select.value || '';
    fillEngines(select, current);
  }
}

function fillEngines(select, current) {
  select.replaceChildren();
  const fallback = document.createElement('option');
  fallback.value = '';
  fallback.textContent = 'default engine';
  select.appendChild(fallback);
  const ids = [...engineInfo.keys()];
  if (current && !ids.includes(current)) ids.unshift(current);
  for (const id of ids) {
    const engine = engineInfo.get(id);
    const option = document.createElement('option');
    option.value = id;
    option.textContent = engine ? `${id} · ${engine.kind} · ${engine.health}` : id;
    if (engine && engine.health !== 'healthy') option.classList.add('warn');
    select.appendChild(option);
  }
  select.value = current;
}

/* ---- cells ---------------------------------------------------------- */

function resize(area) {
  area.style.height = 'auto';
  area.style.height = area.scrollHeight + 'px';
}

function autosizeAll() {
  for (const area of document.querySelectorAll('.editor')) resize(area);
}

function cellsFromDom() {
  return [...document.querySelectorAll('.cell')].map((cell) => ({
    id: cell.dataset.id,
    sql: cell.querySelector('.editor').value,
    engine: cell.querySelector('.engine').value || null,
  }));
}

function cellButton(label, className, action) {
  const element = document.createElement('button');
  element.type = 'button';
  element.className = className;
  element.dataset.action = action;
  element.textContent = label;
  return element;
}

function wireEditor(area) {
  area.addEventListener('input', () => {
    resize(area);
    setDirty(true);
  });
  area.addEventListener('input', () => {
    if (/[A-Za-z0-9_."-]$/.test(area.value.slice(0, area.selectionStart))) scheduleComplete(area);
    else closeComplete();
  });
  area.addEventListener('keydown', (event) => completeKeydown(event, area));
  area.addEventListener('blur', () => closeComplete());
}

function cellTemplate(id, sql, engine) {
  const section = document.createElement('section');
  section.className = 'cell';
  section.dataset.id = id;

  const gutter = document.createElement('div');
  gutter.className = 'cell-gutter';
  const inPrompt = document.createElement('span');
  inPrompt.className = 'prompt in';
  inPrompt.textContent = 'In [ ]';
  gutter.appendChild(inPrompt);

  const body = document.createElement('div');
  body.className = 'cell-body';

  const head = document.createElement('div');
  head.className = 'cell-head';

  const name = document.createElement('code');
  name.className = 'cell-id';
  name.textContent = id;
  head.appendChild(name);

  const actions = document.createElement('div');
  actions.className = 'cell-actions';
  actions.append(
    cellButton('▶ Run', 'btn btn-run', 'run'),
    cellButton('AI', 'btn', 'ai'),
    cellButton('+', 'btn ghost', 'add'),
    cellButton('↑', 'btn ghost', 'up'),
    cellButton('↓', 'btn ghost', 'down'),
    cellButton('✕', 'btn ghost danger', 'delete'),
  );
  head.appendChild(actions);

  const selectWrap = document.createElement('div');
  selectWrap.className = 'select-wrap';
  const label = document.createElement('span');
  label.className = 'muted';
  label.textContent = 'engine';
  const select = document.createElement('select');
  select.className = 'engine';
  select.dataset.current = engine || '';
  selectWrap.append(label, select);
  head.appendChild(selectWrap);

  const area = document.createElement('textarea');
  area.className = 'editor';
  area.spellcheck = false;
  area.value = sql;
  /* Suggestions follow the token under the caret: a trailing word character or
   * dot keeps them coming, anything else dismisses the list. */
  wireEditor(area);

  const out = document.createElement('div');
  out.className = 'out';
  const outPrompt = document.createElement('span');
  outPrompt.className = 'prompt out';
  outPrompt.textContent = 'Out [ ]';
  const outContent = document.createElement('div');
  outContent.className = 'out-content';
  out.append(outPrompt, outContent);

  const main = document.createElement('div');
  main.className = 'cell-main';
  main.append(area, out);

  const split = document.createElement('div');
  split.className = 'cell-split';
  split.append(main, cellChatTemplate());

  body.append(head, split);
  section.append(gutter, body);
  fillEngines(select, engine || '');
  return section;
}

/* The cell conversation mirrors the notebook panel's markup; it is scoped to
 * one cell and stays closed until the cell's AI button opens it. */
function cellChatTemplate() {
  const panel = document.createElement('aside');
  panel.className = 'cell-chat';
  panel.hidden = true;
  panel.setAttribute('aria-label', 'Cell conversation');

  const heading = document.createElement('div');
  heading.className = 'chat-heading';
  const title = document.createElement('strong');
  title.textContent = 'Cell assistant';
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'btn cell-chat-close';
  close.textContent = 'Close';
  heading.append(title, close);

  const note = document.createElement('p');
  note.className = 'status';
  note.textContent = "For this cell only. Sends this cell's SQL and last output; it cannot read the notebook conversation or another cell's.";

  const history = document.createElement('div');
  history.className = 'cell-chat-history chat-history';
  history.tabIndex = 0;
  history.setAttribute('role', 'log');
  history.setAttribute('aria-label', 'Cell conversation messages');
  history.setAttribute('aria-live', 'polite');

  const status = document.createElement('p');
  status.className = 'cell-chat-status status';
  status.setAttribute('role', 'status');

  const form = document.createElement('form');
  form.className = 'cell-chat-form';
  const label = document.createElement('label');
  label.textContent = 'Message';
  const prompt = document.createElement('textarea');
  prompt.className = 'cell-chat-prompt';
  prompt.rows = 3;
  prompt.maxLength = 8192;
  prompt.required = true;
  prompt.placeholder = 'Ask about this query';
  const send = document.createElement('button');
  send.type = 'submit';
  send.className = 'btn primary cell-chat-send';
  send.textContent = 'Send';
  form.append(label, prompt, send);

  panel.append(heading, note, history, status, form);
  return panel;
}

document.addEventListener('click', (event) => {
  const target = event.target.closest && event.target.closest('[data-action]');
  if (!target) return;
  const cell = target.closest('.cell');
  switch (target.dataset.action) {
    case 'run': run(target); break;
    case 'run-all': runAll(); break;
    case 'ai': toggleCellChat(cell); break;
    case 'save': save(); break;
    case 'sync': syncNotebook(); break;
    case 'add': addCell(cell); break;
    case 'up': moveCell(cell, -1); break;
    case 'down': moveCell(cell, 1); break;
    case 'delete': removeCell(cell); break;
  }
});

function addCell(after) {
  const id = 'c' + Date.now().toString(36);
  const section = cellTemplate(id, '', null);
  if (after) after.after(section);
  else cellsRoot.appendChild(section);
  section.querySelector('.editor').focus();
  setDirty(true);
  return section;
}

function removeCell(cell) {
  const cells = [...document.querySelectorAll('.cell')];
  if (cells.length === 1) {
    status('a notebook keeps at least one cell');
    return;
  }
  cell.remove();
  setDirty(true);
}

function moveCell(cell, delta) {
  const sibling = delta < 0 ? cell.previousElementSibling : cell.nextElementSibling;
  if (!sibling) return;
  if (delta < 0) sibling.before(cell);
  else sibling.after(cell);
  setDirty(true);
}

/* ---- running -------------------------------------------------------- */

/* Results live beside the Out prompt, so the In/Out counters stay visible. */
function outBox(out) {
  return out.querySelector('.out-content') || out;
}

/* Jupyter-style execution counter: one number per run, remembered on the cell. */
let execCount = 0;

function markExecuted(cell) {
  execCount += 1;
  cell.dataset.exec = String(execCount);
  const inPrompt = cell.querySelector('.prompt.in');
  if (inPrompt) inPrompt.textContent = `In [${execCount}]`;
  const outPrompt = cell.querySelector('.prompt.out');
  if (outPrompt) outPrompt.textContent = `Out [${execCount}]`;
  return execCount;
}

function outMeta(out, text, badge) {
  const meta = document.createElement('div');
  meta.className = 'out-meta';
  const label = document.createElement('span');
  label.textContent = text;
  meta.appendChild(label);
  if (badge) {
    const chip = document.createElement('span');
    chip.className = 'badge warn';
    chip.textContent = badge;
    meta.appendChild(chip);
  }
  outBox(out).replaceChildren(meta);
  return meta;
}

function outBody(out) {
  const body = document.createElement('div');
  body.className = 'out-body';
  outBox(out).appendChild(body);
  return body;
}

function renderError(out, message) {
  const element = document.createElement('div');
  element.className = 'err';
  element.textContent = message;
  outBox(out).replaceChildren(element);
}

const NUMERIC = /(int|decimal|numeric|double|real|float|bigint)/i;

function renderResult(out, result, ms) {
  outMeta(out, `${result.rows.length} row${result.rows.length === 1 ? '' : 's'} · ${ms} ms`, result.truncated ? 'truncated' : '');
  const body = outBody(out);
  const table = document.createElement('table');
  table.className = 'res';

  const head = document.createElement('thead');
  const headRow = document.createElement('tr');
  for (const column of result.columns) {
    const th = document.createElement('th');
    th.textContent = column.name;
    th.title = column.type;
    headRow.appendChild(th);
  }
  head.appendChild(headRow);
  table.appendChild(head);

  const tbody = document.createElement('tbody');
  for (const row of result.rows) {
    const tr = document.createElement('tr');
    row.forEach((value, index) => {
      const td = document.createElement('td');
      if (value === null) {
        td.textContent = '∅';
        td.className = 'null';
      } else {
        td.textContent = typeof value === 'object' ? JSON.stringify(value) : String(value);
        const type = result.columns[index] ? result.columns[index].type : '';
        if (NUMERIC.test(type)) td.className = 'num';
      }
      tr.appendChild(td);
    });
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  body.appendChild(table);

  if (result.truncated) {
    const note = document.createElement('p');
    note.className = 'status';
    note.textContent = 'more rows available on the engine';
    body.appendChild(note);
  }
}

/* Ctrl/Cmd+Enter runs the focused cell, or sends the focused chat message. */
document.addEventListener('keydown', (event) => {
  if (!(event.ctrlKey || event.metaKey) || event.key !== 'Enter') return;
  const target = event.target;
  if (!target || !target.closest) return;
  const prompt = target.closest('.cell-chat-prompt, #chat-prompt');
  if (prompt) {
    const form = prompt.closest('form');
    if (form) {
      event.preventDefault();
      form.requestSubmit();
    }
    return;
  }
  const editor = target.closest('.editor');
  if (editor) {
    const cell = editor.closest('.cell');
    const run = cell ? cell.querySelector('[data-action=run]') : null;
    if (run) {
      event.preventDefault();
      run.click();
    }
  }
});

async function run(btn) {
  const cell = btn.closest('.cell');
  const area = cell.querySelector('.editor');
  const out = cell.querySelector('.out');
  const engine = cell.querySelector('.engine').value || null;

  cell.classList.add('running');
  btn.disabled = true;
  markExecuted(cell);
  outMeta(out, 'running…', '');
  const started = performance.now();

  let response;
  try {
    response = await fetch('/api/query', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ sql: area.value, engine, notebook: nb.id, cell: cell.dataset.id }),
    });
  } catch (error) {
    cell.classList.remove('running');
    btn.disabled = false;
    renderError(out, 'network error: ' + error);
    return;
  }

  const ms = Math.round(performance.now() - started);
  const data = await response.json().catch(() => ({}));
  cell.classList.remove('running');
  btn.disabled = false;

  if (!response.ok) {
    renderError(out, data.error || `request failed (${response.status})`);
    return;
  }
  renderResult(out, data, ms);
  recordState(cell.dataset.id, engine);
}

/* Run all: cells in document order, one at a time, stopping on nothing — a
 * failed cell shows its error and the run continues. */
async function runAll() {
  for (const cell of document.querySelectorAll('.cell')) {
    await run(cell.querySelector('.btn-run'));
  }
}

async function save() {
  const generation = editGeneration;
  const res = await fetch(notebookPath(), {
    method: 'PUT',
    headers: {
      'content-type': 'application/json',
      ...workspaceHeader(),
      ...(contentRevision ? { 'if-match': '"' + contentRevision + '"' } : { 'if-none-match': '*' }),
    },
    body: JSON.stringify({ ...nb, cells: cellsFromDom() }),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    status('save failed: ' + (data.error || res.status));
    return;
  }
  contentRevision = data.content_revision;
  if (generation === editGeneration) {
    setDirty(false);
    status((team ? 'saved locally; Sync pending ' : 'saved ') + String(data.revision).slice(0, 8));
  } else {
    status('unsaved changes');
  }
}

function notebookPath() {
  const id = encodeURIComponent(nb.id);
  return team ? '/api/teams/' + encodeURIComponent(team) + '/notebooks/' + id : '/api/notebooks/' + id;
}

function workspaceHeader() {
  return team ? { 'x-aster-workspace': workspace } : {};
}

async function syncNotebook() {
  if (!team) return;
  if (dirty) { status('Save before Sync'); return; }
  const generation = editGeneration;
  const button = document.querySelector('[data-action="sync"]');
  button.disabled = true;
  try {
    const response = await fetch(notebookPath() + '/sync', {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...workspaceHeader() },
      body: '{}',
    });
    const result = await response.json().catch(() => ({}));
    if (generation === editGeneration && !dirty) {
      status(response.ok
        ? 'synced ' + String(result.remote_revision).slice(0, 8)
        : 'Sync failed: ' + (result.error || response.status));
    }
  } catch (error) {
    if (generation === editGeneration && !dirty) status('Sync failed: ' + error);
  } finally { button.disabled = dirty; }
}

/* ---- ai helpers ----------------------------------------------------- */

const helperSelect = document.getElementById('helper');
let helpers = [];
let savedHelper = null;
let helperReady = false;
let helperSaving = false;
let pendingState = null;

/* The selection belongs to this user and notebook, outside the Git file. */
async function loadHelpers() {
  if (!helperSelect || !nb) return;
  helperSelect.disabled = true;
  helperSelect.replaceChildren(new Option('loading helpers…', ''));
  try {
    const url = notebookPath() + '/helper';
    const [list, choice] = await Promise.all([fetch('/api/llm'), fetch(url, { headers: workspaceHeader() })]);
    if (!list.ok || !choice.ok) throw new Error('helper selection unavailable');
    helpers = await list.json();
    const preference = await choice.json();
    savedHelper = preference.helper || null;
    helperReady = true;
    fillHelpers(savedHelper);
    if (pendingState) {
      const { cell, engine } = pendingState;
      pendingState = null;
      recordState(cell, engine);
    }
  } catch (error) {
    status('helper selection unavailable');
    helperSelect.replaceChildren(new Option('helper selection unavailable', ''));
    helperSelect.disabled = true;
  }
  updateChatSend();
}

function fillHelpers(current) {
  helperSelect.replaceChildren();
  const placeholder = new Option(helpers.length ? 'select a helper' : 'no helper registered', '');
  placeholder.disabled = true;
  helperSelect.appendChild(placeholder);
  if (current && !helpers.some((helper) => helperMatches(helper, current))) {
    const missing = new Option(`${current} (unavailable)`, current);
    missing.disabled = true;
    helperSelect.appendChild(missing);
  }
  if (helpers.length === 0) {
    helperSelect.disabled = true;
  } else {
    helperSelect.disabled = false;
  }
  for (const helper of helpers) {
    // Keep an already saved legacy personal ID usable without writing on load.
    const value = helper.scope !== 'shared' && helper.id === current ? current : helperRef(helper);
    const scope = helper.scope === 'shared' ? 'Shared' : 'Personal';
    helperSelect.appendChild(new Option(`${scope} · ${helper.id} · ${helper.model}`, value));
  }
  helperSelect.value = current || '';
}

function helperRef(helper) {
  return helper.ref || helper.id;
}

function helperMatches(helper, value) {
  return helperRef(helper) === value || (helper.scope !== 'shared' && helper.id === value);
}

function helperValue() {
  const value = helperSelect && helperSelect.value;
  return helperReady && !helperSaving && value === savedHelper && helpers.some((helper) => helperMatches(helper, value))
    ? value : null;
}

function updateChatSend() {
  const send = document.getElementById('chat-send');
  if (send) send.disabled = chatBusy || !helperValue();
}

if (helperSelect) {
  helperSelect.addEventListener('change', async () => {
    const chosen = helperSelect.value;
    if (!helperReady || helperSaving || !helpers.some((helper) => helperMatches(helper, chosen))) {
      fillHelpers(savedHelper);
      return;
    }
    helperSaving = true;
    helperSelect.disabled = true;
    updateChatSend();
    try {
      const response = await fetch(notebookPath() + '/helper', {
        method: 'PUT',
        headers: { 'content-type': 'application/json', ...workspaceHeader() },
        body: JSON.stringify({ helper: chosen }),
      });
      const value = await response.json();
      if (!response.ok || value.helper !== chosen) throw new Error('helper selection did not save');
      savedHelper = chosen;
      status('helper saved');
      recordNotebook();
    } catch (error) {
      fillHelpers(savedHelper);
      status('helper selection failed');
    } finally {
      helperSaving = false;
      helperSelect.disabled = helpers.length === 0;
      updateChatSend();
    }
  });
}

/* Where this user is, kept in the shared state plane so the index page and the
 * TUI can resume on any container. Best effort: a failure never blocks a cell. */
async function recordNotebook() {
  if (team) return;
  try {
    const response = await fetch('/api/state');
    if (!response.ok) return;
    const state = await response.json();
    if (state.notebook !== nb.id) recordState(null, null);
  } catch (error) { /* best effort */ }
}

function recordState(cell, engine) {
  // A legacy helper must migrate before an ordinary working-state write can
  // replace the old state row.
  if (!nb || team) return;
  if (!helperReady) {
    pendingState = { cell, engine };
    return;
  }
  fetch('/api/state', {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ notebook: nb.id, cell, engine }),
  }).catch(() => {});
}

/* ---- keyboard ------------------------------------------------------- */

document.addEventListener('keydown', (event) => {
  const inEditor = event.target.classList && event.target.classList.contains('editor');
  const accelerator = event.metaKey || event.ctrlKey;

  if (accelerator && event.key === 'Enter') {
    const cell = event.target.closest && event.target.closest('.cell');
    if (cell) {
      event.preventDefault();
      run(cell.querySelector('.btn-run'));
    }
    return;
  }
  if (accelerator && event.key.toLowerCase() === 's' && nb) {
    event.preventDefault();
    save();
    return;
  }
  if (event.shiftKey && event.key === 'Enter' && inEditor) {
    const cell = event.target.closest('.cell');
    event.preventDefault();
    run(cell.querySelector('.btn-run')).then(() => {
      const next = cell.nextElementSibling;
      if (next) next.querySelector('.editor').focus();
      else addCell(cell).querySelector('.editor').focus();
    });
  }
});

/* ---- index page ----------------------------------------------------- */

async function createNotebook(event) {
  event.preventDefault();
  const input = document.getElementById('new-id');
  const id = input.value.trim();
  if (!id) return;
  const response = await fetch('/api/notebooks/' + encodeURIComponent(id), {
    method: 'PUT',
    headers: { 'content-type': 'application/json', 'if-none-match': '*' },
    body: JSON.stringify({ id, title: id, cells: [{ id: 'c1', sql: 'SELECT 1', engine: null }] }),
  });
  if (response.ok) location.href = '/notebooks/' + encodeURIComponent(id);
  else status('create failed: ' + (await response.json().catch(() => ({}))).error);
}

/* ---- catalog filter ------------------------------------------------- */

function filterList(input) {
  const needle = input.value.trim().toLowerCase();
  for (const item of document.querySelectorAll('[data-name]')) {
    item.classList.toggle('hidden', needle !== '' && !item.dataset.name.toLowerCase().includes(needle));
  }
}

/* ---- sql completion -------------------------------------------------- */

/* A dropdown under the focused editor. Suggestions come from the server, which
 * knows the catalogs and their namespaces; the browser only has to place the
 * box, keep a selection and splice the chosen label into the cell. */
const completeBox = document.createElement('div');
completeBox.className = 'complete hidden';
document.body.appendChild(completeBox);

let completeState = { area: null, start: 0, items: [], index: 0 };
let completeTimer = null;

function tokenBefore(text, caret) {
  const match = text.slice(0, caret).match(/[A-Za-z0-9_."-]*$/);
  return match ? match[0] : '';
}

function completeOpen() {
  return !completeBox.classList.contains('hidden');
}

function closeComplete() {
  clearTimeout(completeTimer);
  completeBox.classList.add('hidden');
  completeBox.replaceChildren();
  completeState = { area: null, start: 0, items: [], index: 0 };
}

function renderComplete() {
  completeBox.replaceChildren();
  completeState.items.forEach((item, index) => {
    const row = document.createElement('div');
    row.className = 'complete-item' + (index === completeState.index ? ' active' : '');
    const kind = document.createElement('span');
    kind.className = 'complete-kind';
    kind.textContent = item.kind;
    const label = document.createElement('span');
    label.textContent = item.label;
    row.append(kind, label);
    /* mousedown, not click: the editor must not lose focus first. */
    row.addEventListener('mousedown', (event) => {
      event.preventDefault();
      acceptComplete(index);
    });
    completeBox.appendChild(row);
  });
}

function showComplete(area, items, start) {
  completeState = { area, start, items, index: 0 };
  const rect = area.getBoundingClientRect();
  completeBox.style.left = rect.left + 'px';
  completeBox.style.top = rect.bottom + 2 + 'px';
  completeBox.style.minWidth = Math.max(rect.width, 240) + 'px';
  completeBox.classList.remove('hidden');
  renderComplete();
}

async function requestComplete(area) {
  const caret = area.selectionStart;
  const token = tokenBefore(area.value, caret);
  let items;
  try {
    const res = await fetch(
      '/api/sql/complete?sql=' + encodeURIComponent(area.value.slice(0, caret)),
    );
    if (!res.ok) return closeComplete();
    items = await res.json();
  } catch (error) {
    return closeComplete();
  }
  if (!Array.isArray(items) || items.length === 0) return closeComplete();
  /* Replace only the segment after the last dot, so accepting `sales` after
     `polaris.` yields `polaris.sales` rather than dropping the qualifier. A
     token with no dot (a bare or quoted name) is replaced whole. */
  const segment = token.slice(token.lastIndexOf('.') + 1);
  showComplete(area, items, caret - segment.length);
}

function scheduleComplete(area) {
  clearTimeout(completeTimer);
  completeTimer = setTimeout(() => requestComplete(area), 120);
}

function acceptComplete(index) {
  const { area, start, items } = completeState;
  if (!area || !items[index]) return;
  /* `insert` is the label quoted when SQL needs it; plain names are identical. */
  const text = items[index].insert || items[index].label;
  const caret = area.selectionStart;
  area.value = area.value.slice(0, start) + text + area.value.slice(caret);
  const position = start + text.length;
  area.setSelectionRange(position, position);
  resize(area);
  setDirty(true);
  closeComplete();
  area.focus();
}

function completeKeydown(event, area) {
  if ((event.metaKey || event.ctrlKey) && event.key === ' ') {
    event.preventDefault();
    return requestComplete(area);
  }
  if (!completeOpen()) return;
  const { items } = completeState;
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const step = event.key === 'ArrowDown' ? 1 : -1;
    completeState.index = (completeState.index + step + items.length) % items.length;
    return renderComplete();
  }
  if ((event.key === 'Tab' || event.key === 'Enter') && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
    event.preventDefault();
    event.stopPropagation();
    return acceptComplete(completeState.index);
  }
  if (event.key === 'Escape') {
    event.preventDefault();
    closeComplete();
  }
}

/* ---- boot ----------------------------------------------------------- */

for (const area of document.querySelectorAll('.editor')) wireEditor(area);
loadEngines().then(autosizeAll);
loadHelpers();
if (nb) recordState(null, null);

const createForm = document.getElementById('create');
if (createForm) createForm.addEventListener('submit', createNotebook);


/* Saved conversation state is server-owned; browser state holds only the draft. */
const chatPanel = document.getElementById('chat-panel');
let conversation = null;
let chatBusy = false;

async function chatRpc(method, body) {
  if (team) body = { ...body, team, workspace };
  const response = await fetch('/aster.v1.Aster/' + method, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'connect-protocol-version': '1' },
    body: JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) {
    const error = new Error(value.message || 'Conversation request failed');
    error.conflict = response.status === 409;
    throw error;
  }
  return value;
}

function renderConversation() {
  const history = document.getElementById('chat-history');
  history.replaceChildren();
  for (const message of conversation.messages || []) {
    const entry = document.createElement('article');
    entry.className = 'chat-message ' + (message.role === 'assistant' ? 'assistant' : 'user');
    const label = document.createElement('strong');
    label.textContent = message.role === 'assistant' ? message.helper || 'Assistant' : 'You';
    const text = document.createElement('div');
    text.className = 'chat-text';
    text.textContent = message.content;
    entry.append(label, text);
    if (message.role === 'assistant') {
      /* One suggestion per reply: the last fenced SQL block is the answer. */
      const blocks = [...message.content.matchAll(/```sql\s*\n([\s\S]*?)```/gi)];
      if (blocks.length > 0) {
        const suggested = blocks[blocks.length - 1][1].trim();
        const insert = document.createElement('button');
        insert.type = 'button';
        insert.className = 'btn';
        insert.textContent = 'Insert SQL as new cell';
        insert.addEventListener('click', () => {
          const cell = addCell(document.querySelector('.cell:last-child'));
          cell.querySelector('.editor').value = suggested;
          resize(cell.querySelector('.editor'));
          setDirty(true);
        });
        entry.appendChild(insert);
      }
    }
    history.appendChild(entry);
  }
  history.scrollTop = history.scrollHeight;
}

async function loadConversation() {
  const loaded = await chatRpc('GetConversation', { notebook: nb.id });
  if (!conversation || BigInt(loaded.revision || '0') >= BigInt(conversation.revision || '0')) {
    conversation = loaded;
    renderConversation();
  }
}

async function openChat() {
  chatPanel.hidden = false;
  document.body.classList.add('chat-open');
  document.getElementById('chat-toggle').setAttribute('aria-expanded', 'true');
  document.getElementById('chat-prompt').focus();
  if (!chatBusy) {
    document.getElementById('chat-send').disabled = true;
    try {
      await loadConversation();
      document.getElementById('chat-status').textContent = '';
    } catch (error) {
      conversation = null;
      document.getElementById('chat-status').textContent = error.message;
    } finally { updateChatSend(); }
  }
}

function closeChat() {
  chatPanel.hidden = true;
  document.body.classList.remove('chat-open', 'chat-wide');
  document.getElementById('chat-toggle').setAttribute('aria-expanded', 'false');
  document.getElementById('chat-expand').setAttribute('aria-pressed', 'false');
  document.getElementById('chat-expand').textContent = 'Expand';
  document.getElementById('chat-toggle').focus();
}

if (chatPanel) {
  document.getElementById('chat-toggle').addEventListener('click', () => chatPanel.hidden ? openChat() : closeChat());
  document.getElementById('chat-close').addEventListener('click', closeChat);
  document.getElementById('chat-expand').addEventListener('click', (event) => {
    const expanded = document.body.classList.toggle('chat-wide');
    event.target.setAttribute('aria-pressed', String(expanded));
    event.target.textContent = expanded ? 'Narrow' : 'Expand';
  });
  chatPanel.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') { event.preventDefault(); closeChat(); }
  });
  document.getElementById('chat-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    if (chatBusy) return;
    const prompt = document.getElementById('chat-prompt');
    const context = document.getElementById('chat-context');
    const status = document.getElementById('chat-status');
    const chosen = helperValue();
    if (!chosen) { status.textContent = 'Register and select a helper first.'; return; }
    chatBusy = true;
    document.getElementById('chat-send').disabled = true;
    prompt.disabled = context.disabled = true;
    status.textContent = 'Waiting for the helper…';
    try {
      if (!conversation) await loadConversation();
      conversation = await chatRpc('SendMessage', {
        notebook: nb.id, helper: chosen, prompt: prompt.value,
        context: context.value, expectedRevision: conversation.revision || '0',
      });
      renderConversation();
      prompt.value = '';
      context.value = '';
      status.textContent = 'Saved. Replies do not execute queries.';
    } catch (error) {
      status.textContent = error.message + ' Your draft is unchanged; reload history before resending.';
      try { await loadConversation(); } catch (_) { conversation = null; }
    } finally {
      chatBusy = false;
      updateChatSend();
      prompt.disabled = context.disabled = false;
      if (!chatPanel.hidden) prompt.focus();
    }
  });
}

/* ---- cell conversation ------------------------------------------------ */

/* One history per cell, kept in the browser only; the server owns the stored
 * conversation under the notebook+cell key. */
const cellChats = new Map();

function cellChatParts(cell) {
  return {
    panel: cell.querySelector('.cell-chat'),
    history: cell.querySelector('.cell-chat-history'),
    status: cell.querySelector('.cell-chat-status'),
    prompt: cell.querySelector('.cell-chat-prompt'),
    send: cell.querySelector('.cell-chat-send'),
  };
}

function cellChatState(id) {
  if (!cellChats.has(id)) cellChats.set(id, { revision: '0', messages: [], busy: false, loaded: false });
  return cellChats.get(id);
}

/* The cell's own SQL and last output. Never the notebook conversation, never
 * another cell: the server grounds on exactly this text. */
function cellChatContext(cell) {
  const sql = cell.querySelector('.editor').value.trim();
  const out = cell.querySelector('.out-content');
  const output = (out ? out.innerText : '').trim().slice(0, 4000);
  let text = sql ? 'Current cell SQL:\n' + sql : '';
  if (output) text += (text ? '\n\n' : '') + 'Last run output:\n' + output;
  return text.slice(0, 8192);
}

function renderCellChat(cell) {
  const { history } = cellChatParts(cell);
  const state = cellChatState(cell.dataset.id);
  history.replaceChildren();
  for (const message of state.messages) {
    const entry = document.createElement('article');
    entry.className = 'chat-message ' + (message.role === 'assistant' ? 'assistant' : 'user');
    const label = document.createElement('strong');
    label.textContent = message.role === 'assistant' ? message.helper || 'Assistant' : 'You';
    const text = document.createElement('div');
    text.className = 'chat-text';
    text.textContent = message.content;
    entry.append(label, text);
    if (message.role === 'assistant') {
      /* One suggestion per reply: the last fenced SQL block is the answer. */
      const blocks = [...message.content.matchAll(/```sql\s*\n([\s\S]*?)```/gi)];
      if (blocks.length > 0) {
        const suggested = blocks[blocks.length - 1][1].trim();
        const apply = document.createElement('button');
        apply.type = 'button';
        apply.className = 'btn';
        apply.textContent = 'Replace this cell';
        apply.addEventListener('click', () => {
          const editor = cell.querySelector('.editor');
          editor.value = suggested;
          resize(editor);
          setDirty(true);
        });
        entry.appendChild(apply);
      }
    }
    history.appendChild(entry);
  }
  history.scrollTop = history.scrollHeight;
}

async function loadCellChat(cell) {
  const state = cellChatState(cell.dataset.id);
  const loaded = await chatRpc('GetConversation', { notebook: nb.id, cell: cell.dataset.id });
  if (!state.loaded || BigInt(loaded.revision || '0') >= BigInt(state.revision || '0')) {
    state.revision = loaded.revision || '0';
    state.messages = loaded.messages || [];
    state.loaded = true;
    renderCellChat(cell);
  }
}

async function openCellChat(cell) {
  const parts = cellChatParts(cell);
  parts.panel.hidden = false;
  parts.prompt.focus();
  try {
    await loadCellChat(cell);
    parts.status.textContent = '';
  } catch (error) {
    parts.status.textContent = error.message;
  }
}

function closeCellChat(cell) {
  cellChatParts(cell).panel.hidden = true;
}

function toggleCellChat(cell) {
  if (!cell) return;
  if (cellChatParts(cell).panel.hidden) openCellChat(cell);
  else closeCellChat(cell);
}

async function sendCellChat(cell) {
  const parts = cellChatParts(cell);
  const state = cellChatState(cell.dataset.id);
  if (state.busy) return;
  const chosen = helperValue();
  if (!chosen) { parts.status.textContent = 'Register and select a helper first.'; return; }
  state.busy = true;
  parts.send.disabled = true;
  parts.prompt.disabled = true;
  parts.status.textContent = 'Waiting for the helper…';
  try {
    if (!state.loaded) await loadCellChat(cell);
    const sent = await chatRpc('SendMessage', {
      notebook: nb.id, cell: cell.dataset.id, helper: chosen,
      prompt: parts.prompt.value, context: cellChatContext(cell),
      expectedRevision: state.revision || '0',
    });
    state.revision = sent.revision || state.revision;
    state.messages = sent.messages || [];
    state.loaded = true;
    renderCellChat(cell);
    parts.prompt.value = '';
    parts.status.textContent = 'Saved. Replies do not execute queries.';
  } catch (error) {
    parts.status.textContent = error.message + ' Your draft is unchanged; reopen to reload history.';
  } finally {
    state.busy = false;
    parts.send.disabled = false;
    parts.prompt.disabled = false;
    if (!parts.panel.hidden) parts.prompt.focus();
  }
}

if (cellsRoot) {
  cellsRoot.addEventListener('click', (event) => {
    const close = event.target.closest && event.target.closest('.cell-chat-close');
    if (!close) return;
    const cell = close.closest('.cell');
    if (cell) closeCellChat(cell);
  });
  cellsRoot.addEventListener('submit', (event) => {
    if (!event.target.classList.contains('cell-chat-form')) return;
    event.preventDefault();
    const cell = event.target.closest('.cell');
    if (cell) sendCellChat(cell);
  });
}
