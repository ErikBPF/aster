// Client for the server-rendered shell. Every page shares the nav and the
// status line, so every entry point tolerates the elements it needs being
// absent.

const nbNode = document.getElementById('nb');
const nb = nbNode ? JSON.parse(nbNode.textContent) : null;
const statusLine = document.getElementById('status');
const cellsRoot = document.getElementById('cells');
const helperSession = Array.from(crypto.getRandomValues(new Uint8Array(16)),
  byte => byte.toString(16).padStart(2, '0')).join('');

let dirty = false;
let engineInfo = new Map();

function status(text) {
  if (statusLine) statusLine.textContent = text;
}

function setDirty(value) {
  dirty = value;
  if (dirty) status('unsaved changes');
  else if (statusLine) statusLine.textContent = '';
}

window.addEventListener('beforeunload', (event) => {
  if (dirty) event.preventDefault();
});

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
  area.addEventListener('input', () => {
    resize(area);
    setDirty(true);
  });
  /* Suggestions follow the token under the caret: a trailing word character or
   * dot keeps them coming, anything else dismisses the list. */
  area.addEventListener('input', () => {
    if (/[A-Za-z0-9_."-]$/.test(area.value.slice(0, area.selectionStart))) scheduleComplete(area);
    else closeComplete();
  });
  area.addEventListener('keydown', (event) => completeKeydown(event, area));
  area.addEventListener('blur', () => closeComplete());

  const out = document.createElement('div');
  out.className = 'out';
  const outPrompt = document.createElement('span');
  outPrompt.className = 'prompt out';
  outPrompt.textContent = 'Out [ ]';
  const outContent = document.createElement('div');
  outContent.className = 'out-content';
  out.append(outPrompt, outContent);

  body.append(head, area, out);
  section.append(gutter, body);
  fillEngines(select, engine || '');
  return section;
}

document.addEventListener('click', (event) => {
  const target = event.target.closest && event.target.closest('[data-action]');
  if (!target) return;
  const cell = target.closest('.cell');
  switch (target.dataset.action) {
    case 'run': run(target); break;
    case 'run-all': runAll(); break;
    case 'ai': generate(target); break;
    case 'save': save(); break;
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
      body: JSON.stringify({ sql: area.value, engine }),
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

async function generate(btn) {
  const cell = btn.closest('.cell');
  const area = cell.querySelector('.editor');
  const out = cell.querySelector('.out');
  outMeta(out, 'asking the model…', '');
  const res = await fetch('/api/ai', {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-opencode-session': helperSession },
    body: JSON.stringify({
      prompt: `Write a query for: ${(nb && nb.title) || 'a report'}`,
      sql: area.value,
      helper: helperValue(),
    }),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    renderError(out, data.error || `request failed (${res.status})`);
    return;
  }
  area.value = data.sql;
  resize(area);
  setDirty(true);
  outMeta(out, 'suggestion inserted; run it to check.', '');
}

async function save() {
  const res = await fetch('/api/notebooks/' + encodeURIComponent(nb.id), {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ...nb, cells: cellsFromDom() }),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    status('save failed: ' + (data.error || res.status));
    return;
  }
  setDirty(false);
  status('saved ' + String(data.revision).slice(0, 8));
}

/* ---- ai helpers ----------------------------------------------------- */

const helperSelect = document.getElementById('helper');
let helpers = [];

/* The notebook toolbar offers every helper this user registered; the choice
 * lives in the working state so a reload keeps it. */
async function loadHelpers() {
  if (!helperSelect) return;
  const res = await fetch('/api/llm');
  if (res.ok) helpers = await res.json().catch(() => []);
  fillHelpers('');
  try {
    const state = await (await fetch('/api/state')).json();
    if (state && state.notebook === (nb && nb.id) && state.helper) fillHelpers(state.helper);
  } catch (error) {
    /* best effort */
  }
}

function fillHelpers(current) {
  helperSelect.replaceChildren();
  if (helpers.length === 0) {
    const option = document.createElement('option');
    option.value = '';
    option.textContent = 'no helper registered';
    helperSelect.appendChild(option);
    helperSelect.disabled = true;
    return;
  }
  helperSelect.disabled = false;
  for (const helper of helpers) {
    const option = document.createElement('option');
    option.value = helper.id;
    option.textContent = `${helper.id} · ${helper.model}`;
    helperSelect.appendChild(option);
  }
  helperSelect.value = current && helpers.some((h) => h.id === current) ? current : helpers[0].id;
}

function helperValue() {
  return helperSelect && helperSelect.value ? helperSelect.value : null;
}

if (helperSelect) {
  helperSelect.addEventListener('change', () => recordState(null, null));
}

/* Where this user is, kept in the shared state plane so the index page and the
 * TUI can resume on any container. Best effort: a failure never blocks a cell. */
function recordState(cell, engine) {
  if (!nb) return;
  fetch('/api/state', {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ notebook: nb.id, cell, engine, helper: helperValue() }),
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
  await fetch('/api/notebooks/' + encodeURIComponent(id), {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ id, title: id, cells: [{ id: 'c1', sql: 'SELECT 1', engine: null }] }),
  });
  location.href = '/notebooks/' + encodeURIComponent(id);
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
  showComplete(area, items, caret - token.length);
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

if (nb) {
  // Opening the notebook is itself a place to resume from.
  recordState(null, null);
}

loadEngines().then(autosizeAll);
loadHelpers();

const createForm = document.getElementById('create');
if (createForm) createForm.addEventListener('submit', createNotebook);
