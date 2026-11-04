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
let selectedContract = null;
function contractForTurn() {
  const selected = document.getElementById('contract-selection');
  if (selected?.value && !selectedContract) throw new Error('Select and load an exact contract object before asking the helper.');
  return selectedContract ? {contractSelection: {...selectedContract}} : {};
}

function syncContractDraft(editor) {
  const draft = document.getElementById('contract-draft');
  if (!draft) return;
  draft.value = editor.value;
  document.getElementById('contract-review').replaceChildren();
}

/* Contract context is request-scoped; never persist admitted meaning in browser storage. */
if (document.getElementById('contract-context')) {
  const selection = document.getElementById('contract-selection');
  const object = document.getElementById('contract-object');
  const documentView = document.getElementById('contract-document');
  const details = document.createElement('section');
  details.id = 'contract-details';
  details.setAttribute('aria-label', 'Declared contract details');
  documentView.closest('details').before(details);
  const preparation = document.getElementById('contract-preparation');
  const draft = document.getElementById('contract-draft');
  const review = document.getElementById('contract-review');
  const reviewButton = document.getElementById('contract-review-button');
  const message = document.getElementById('contract-status');
  let generation = 0;
  let contracts = [];
  function clear() {
    selectedContract = null;
    documentView.textContent = '';
    details.replaceChildren();
    preparation.replaceChildren();
    review.replaceChildren();
    reviewButton.disabled = true;
  }
  async function rpc(method, body) {
    const response = await fetch('/aster.v1.Aster/' + method, {
      method: 'POST', cache: 'no-store',
      headers: {'content-type': 'application/json', 'connect-protocol-version': '1'},
      body: JSON.stringify(body),
    });
    if (!response.ok) throw new Error('Contract unavailable or access denied. Invalid or unsupported bundles are refused at startup.');
    return JSON.parse((await response.json()).contextJson);
  }
  function show(parent, title, value) {
    const heading = document.createElement('h2');
    heading.textContent = title;
    const content = document.createElement('pre');
    content.textContent = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    parent.append(heading, content);
  }
  // Walk the whole declared tree, including extension fields. Presence, not
  // truthiness, determines what is displayed: false and zero are declarations.
  function fields(value) {
    if (value === null || typeof value !== 'object') {
      const text = document.createElement('span');
      text.textContent = String(value);
      return text;
    }
    if (Array.isArray(value)) {
      const list = document.createElement('ol');
      for (const item of value) {
        const entry = document.createElement('li');
        entry.append(fields(item));
        list.append(entry);
      }
      return list;
    }
    const list = document.createElement('dl');
    for (const [key, item] of Object.entries(value)) {
      const label = document.createElement('dt');
      label.textContent = {schema: 'Tables / schema', properties: 'Columns / properties',
        quality: 'Declared quality rules'}[key] || key;
      const content = document.createElement('dd');
      content.append(fields(item));
      list.append(label, content);
    }
    return list;
  }
  function declared(value) {
    const heading = document.createElement('h2');
    heading.textContent = 'Declared contract';
    const note = document.createElement('p');
    note.textContent = 'Declared meaning and rules, not observed facts. Quality results: not measured.';
    details.replaceChildren(heading, note, fields(value));
  }
  function context(parent, value) {
    show(parent, 'Declared meaning and semantics', {contractMeaning: value.contractMeaning, declared: value.declared});
    show(parent, 'Provenance and selected object', {provenance: value.provenance, object: value.object});
    show(parent, 'Explicit physical binding', value.binding);
    show(parent, 'Observed metadata (separate admission)', value.observation);
    show(parent, 'Drift comparison (separate facts)', value.comparison);
  }
  async function refresh() {
    const current = ++generation;
    clear();
    contracts = [];
    selection.replaceChildren(new Option('Select a contract', ''));
    object.replaceChildren(new Option('Select an object', ''));
    object.disabled = true;
    message.textContent = 'Loading authorized contracts...';
    try {
      const value = await rpc('ListContracts', {});
      if (current !== generation) return;
      contracts = value.contracts;
      contracts.forEach((entry, index) => selection.add(new Option(`${entry.id} / ${entry.version} / ${entry.target}`, String(index))));
      message.textContent = contracts.length ? 'Choose a declared contract before physical observation.' : 'No authorized contracts available.';
    } catch (error) {
      if (current === generation) message.textContent = error.message;
    }
  }
  async function load(withReview = false) {
    const current = ++generation;
    clear();
    const entry = selection.value === '' ? null : contracts[Number(selection.value)];
    const pointer = object.value;
    object.disabled = true;
    if (!entry) return;
    message.textContent = 'Checking current contract access...';
    try {
      const value = await rpc('GetContract', {path: entry.path, sha256: entry.sha256});
      if (current !== generation) return;
      const prepared = pointer ? await rpc('PrepareContractQuery', {path: entry.path, sha256: entry.sha256, object: pointer}) : null;
      if (current !== generation) return;
      documentView.textContent = JSON.stringify(value, null, 2);
      declared(value.declared);
      object.replaceChildren(new Option('Select an object', ''));
      (value.declared.schema || []).forEach((item, index) => object.add(new Option(item.businessName || item.name, `/schema/${index}`)));
      object.value = pointer;
      object.disabled = false;
      if (prepared) {
        selectedContract = {path: entry.path, sha256: entry.sha256, object: pointer};
        context(preparation, prepared);
        reviewButton.disabled = false;
        if (withReview) {
          show(review, 'Query draft for manual review', draft.value);
          context(review, prepared);
        }
      }
      message.textContent = 'Declared meaning remains usable without observation. Nothing has executed.';
    } catch (error) {
      if (current !== generation) return;
      clear();
      contracts = [];
      selection.replaceChildren(new Option('Select a contract', ''));
      object.replaceChildren(new Option('Select an object', ''));
      message.textContent = error.message;
    }
  }
  selection.addEventListener('change', () => { object.value = ''; load(); });
  object.addEventListener('change', () => load());
  draft.addEventListener('input', () => review.replaceChildren());
  document.addEventListener('input', (event) => {
    if (!event.target.matches('.cell .editor')) return;
    syncContractDraft(event.target);
  });
  document.addEventListener('focusin', (event) => {
    if (!event.target.matches('.cell .editor')) return;
    syncContractDraft(event.target);
  });
  reviewButton.addEventListener('click', () => load(true));
  document.getElementById('contract-refresh').addEventListener('click', refresh);
  window.addEventListener('pageshow', (event) => { if (event.persisted) refresh(); });
  refresh();
}


let dirty = false;
let editGeneration = 0;
let autosaveTimer = null;
let saving = false;
let autosaveBlocked = false;
let catalogsPromise;
const contextGeneration = new WeakMap();

function status(text) {
  if (statusLine) statusLine.textContent = text;
}

function scheduleAutosave() {
  if (!nb || autosaveBlocked) return;
  clearTimeout(autosaveTimer);
  autosaveTimer = setTimeout(() => save(true), 1800);
}

function setDirty(value) {
  if (value) editGeneration += 1;
  dirty = value;
  const syncButton = document.querySelector('[data-action="sync"]');
  if (syncButton) syncButton.disabled = dirty;
  if (dirty) {
    status('unsaved changes');
    scheduleAutosave();
  } else if (statusLine) statusLine.textContent = '';
}

window.addEventListener('beforeunload', (event) => {
  if (!dirty) return;
  event.preventDefault();
  event.returnValue = '';
});

/* ---- engines -------------------------------------------------------- */

/* One engine belongs to the notebook. Cells inherit it; the server refuses a
 * run that names a different engine. */
const notebookEngine = document.getElementById('notebook-engine');

async function loadEngines() {
  if (!notebookEngine) return;
  const legacy = (nb?.cells || []).find((cell) => cell.engine)?.engine;
  const current = nb?.engine || legacy || notebookEngine.dataset.current || '';
  fillEngines(notebookEngine, current);
  notebookEngine.disabled = true;
  try {
    const response = await fetch('/api/engines', {cache: 'no-store'});
    if (!response.ok) throw new Error('Engine choices unavailable');
    const choices = await response.json();
    fillEngines(notebookEngine, current, new Map(choices.map((entry) => [entry.id, entry])));
    for (const option of notebookEngine.options) {
      if (option.value && !choices.some((entry) => entry.id === option.value)) option.disabled = true;
    }
    notebookEngine.disabled = choices.length === 0;
  } catch (error) {
    status(error.message);
  }
}

function notebookEngineId() {
  return notebookEngine ? (notebookEngine.value || null) : null;
}

notebookEngine?.addEventListener('change', () => setDirty(true));

function fillEngines(select, current, engineInfo = new Map()) {
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

function contextSelect(cell, name, className) {
  const label = document.createElement('label');
  label.className = 'select-wrap';
  const title = document.createElement('span');
  title.textContent = name;
  const select = document.createElement('select');
  select.className = className;
  select.setAttribute('aria-label', name);
  label.append(title, select);
  const context = cell.querySelector('.cell-context') || cell.querySelector('.cell-head');
  context.append(label);
  return select;
}

async function initCellContext(cell) {
  const saved = nb?.cells.find((entry) => entry.id === cell.dataset.id);
  cell.metadata = {...saved?.metadata};
  if (cell.dataset.kind === 'markdown') return;
  const catalog = contextSelect(cell, 'Catalog', 'catalog-context');
  const schema = contextSelect(cell, 'Schema', 'query-schema');
  catalog.add(new Option('Select a catalog', ''));
  schema.add(new Option('Select a schema', ''));
  // Keep saved context serializable while discovery is pending or unavailable.
  if (cell.metadata.catalog_context) catalog.add(new Option(cell.metadata.catalog_context, cell.metadata.catalog_context, false, true));
  if (cell.metadata.schema) schema.add(new Option(cell.metadata.schema, cell.metadata.schema, false, true));
  catalog.disabled = schema.disabled = true;
  schema.addEventListener('change', () => setDirty(true));
  catalog.addEventListener('change', () => {
    setDirty(true);
    loadCellContext(cell, '');
  });
  try {
    catalogsPromise ||= fetch('/api/catalogs', {cache: 'no-store'}).then(async (response) => {
      if (!response.ok) throw new Error('Catalog choices unavailable');
      return response.json();
    });
    const catalogs = await catalogsPromise;
    catalog.replaceChildren(new Option('Select a catalog', ''));
    for (const entry of catalogs) catalog.add(new Option(entry.id, entry.id));
    const current = cell.metadata.catalog_context || (catalogs.length === 1 ? catalogs[0].id : '');
    if (current && !catalogs.some((entry) => entry.id === current)) {
      const missing = new Option(current + ' (unavailable)', current);
      missing.disabled = true;
      catalog.add(missing);
    }
    catalog.value = current;
    catalog.disabled = false;
    await loadCellContext(cell, cell.metadata.schema || '');
  } catch (error) {
    status(error.message);
  }
}

async function loadCellContext(cell, currentSchema) {
  const generation = (contextGeneration.get(cell) || 0) + 1;
  contextGeneration.set(cell, generation);
  const catalog = cell.querySelector('.catalog-context');
  const schema = cell.querySelector('.query-schema');
  if (!catalog || !schema) return;
  schema.disabled = true;
  schema.replaceChildren(new Option('Select a schema', ''));
  if (currentSchema) schema.add(new Option(currentSchema, currentSchema));
  schema.value = currentSchema;
  if (!catalog.value) return;
  try {
    const namespaces = await fetch('/api/catalogs/' + encodeURIComponent(catalog.value) + '/namespaces', {cache: 'no-store'});
    if (!namespaces.ok) throw new Error('Catalog context unavailable; select an authorized catalog.');
    const schemas = await namespaces.json();
    if (contextGeneration.get(cell) !== generation) return;
    schema.replaceChildren(new Option('Select a schema', ''));
    for (const entry of schemas) schema.add(new Option(entry.name, entry.name));
    if (currentSchema && !schemas.some((entry) => entry.name === currentSchema)) {
      const missing = new Option(currentSchema + ' (unavailable)', currentSchema);
      missing.disabled = true;
      schema.add(missing);
    }
    schema.value = currentSchema || (schemas.some((entry) => entry.name === 'tiny') ? 'tiny' : '');
    schema.disabled = false;
  } catch (error) {
    if (contextGeneration.get(cell) === generation) status(error.message);
  }
}

/* ---- markdown ------------------------------------------------------- */

/* The first non-empty line selects the cell kind. Only `%md` changes it;
 * `%sql` is accepted and stripped like the implicit default. */
function cellMagic(text) {
  const line = text.split('\n').find((entry) => entry.trim() !== '');
  return line ? line.trim() : '';
}

function cellKind(text) {
  return cellMagic(text) === '%md' ? 'markdown' : 'sql';
}

function cellSource(text) {
  const lines = text.split('\n');
  for (let index = 0; index < lines.length; index += 1) {
    const trimmed = lines[index].trim();
    if (trimmed === '') continue;
    if (trimmed === '%md' || trimmed === '%sql') return lines.slice(index + 1).join('\n');
    break;
  }
  return text;
}

function renderMarkdownInto(element, text) {
  if (!window.marked || !window.DOMPurify) {
    element.textContent = cellSource(text);
    return;
  }
  const html = window.marked.parse(cellSource(text), {gfm: true, breaks: false});
  element.innerHTML = window.DOMPurify.sanitize(html, {USE_PROFILES: {html: true}});
  const blocks = element.querySelectorAll('pre > code.language-mermaid');
  blocks.forEach((code, index) => {
    const holder = document.createElement('div');
    holder.className = 'mermaid';
    holder.dataset.diagram = 'mermaid-' + index;
    holder.textContent = code.textContent;
    code.parentElement.replaceWith(holder);
  });
  if (window.mermaid && blocks.length > 0) {
    window.mermaid.run({nodes: element.querySelectorAll('.mermaid')}).catch((error) => {
      status('diagram: ' + error.message);
    });
  }
}

function mountMarkdown(cell) {
  const editor = cell.querySelector('.editor');
  const preview = cell.querySelector('.md-preview');
  if (!editor || !preview) return;
  const render = () => renderMarkdownInto(preview, editor.value);
  render();
  if (editor.dataset.mdWired === '1') return;
  editor.dataset.mdWired = '1';
  editor.addEventListener('input', () => {
    if (cellKind(editor.value) === 'markdown') render();
  });
  editor.addEventListener('blur', () => {
    if (cellKind(editor.value) !== 'markdown') setCellKind(cell, 'sql');
  });
}

function setCellKind(cell, kind) {
  cell.dataset.kind = kind;
  cell.classList.toggle('md-cell', kind === 'markdown');
  const run = cell.querySelector('[data-action=run]');
  if (run) run.hidden = kind === 'markdown';
  const toggle = cell.querySelector('[data-action=md-toggle]');
  if (toggle) toggle.hidden = kind !== 'markdown';
  const out = cell.querySelector('.out');
  if (out) out.hidden = kind === 'markdown';
  const editor = cell.querySelector('.editor');
  const preview = cell.querySelector('.md-preview');
  if (kind === 'markdown') {
    cell.classList.remove('md-editing');
    if (toggle) toggle.textContent = 'Edit';
    if (editor) editor.classList.add('hidden');
    if (preview) preview.classList.remove('hidden');
    mountMarkdown(cell);
  } else {
    cell.classList.remove('md-editing');
    if (editor) editor.classList.remove('hidden');
    if (preview) preview.classList.add('hidden');
  }
}

function toggleMarkdownEdit(cell) {
  if (cell.dataset.kind !== 'markdown') return;
  const editor = cell.querySelector('.editor');
  if (cellKind(editor.value) !== 'markdown') {
    setCellKind(cell, 'sql');
    return;
  }
  const editing = cell.classList.toggle('md-editing');
  const toggle = cell.querySelector('[data-action=md-toggle]');
  const preview = cell.querySelector('.md-preview');
  if (toggle) toggle.textContent = editing ? 'Preview' : 'Edit';
  if (editor) editor.classList.toggle('hidden', !editing);
  if (preview) preview.classList.toggle('hidden', editing);
  if (!editing) renderMarkdownInto(preview, editor.value);
  else editor.focus();
}

if (window.mermaid) {
  window.mermaid.initialize({startOnLoad: false, securityLevel: 'strict', theme: 'neutral'});
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
  return [...document.querySelectorAll('.cell')].map((cell) => {
    const catalog = cell.querySelector('.catalog-context');
    const schema = cell.querySelector('.query-schema');
    return {
      id: cell.dataset.id,
      sql: cell.querySelector('.editor').value,
      engine: null,
      metadata: {...cell.metadata,
        catalog_context: catalog ? (catalog.value || null) : null,
        schema: schema ? (schema.value || null) : null},
    };
  });
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
  area.addEventListener('blur', () => {
    const cell = area.closest('.cell');
    if (cell && cell.dataset.kind !== 'markdown' && cellKind(area.value) === 'markdown') {
      setCellKind(cell, 'markdown');
    }
  });
}

function cellTemplate(id, sql) {
  const section = document.createElement('section');
  section.className = 'cell';
  section.dataset.id = id;
  section.dataset.kind = 'sql';

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

  const context = document.createElement('div');
  context.className = 'cell-context';
  head.appendChild(context);

  const actions = document.createElement('div');
  actions.className = 'cell-actions';
  const run = cellButton('▶ Run', 'btn btn-run', 'run');
  const mdToggle = cellButton('Edit', 'btn ghost md-toggle', 'md-toggle');
  mdToggle.hidden = true;
  actions.append(
    run,
    cellButton('AI', 'btn', 'ai'),
    mdToggle,
    cellButton('↑', 'btn ghost', 'up'),
    cellButton('↓', 'btn ghost', 'down'),
    cellButton('✕', 'btn ghost danger', 'delete'),
  );
  head.appendChild(actions);

  const area = document.createElement('textarea');
  area.className = 'editor';
  area.spellcheck = false;
  area.value = sql;
  /* Suggestions follow the token under the caret: a trailing word character or
   * dot keeps them coming, anything else dismisses the list. */
  wireEditor(area);

  const preview = document.createElement('div');
  preview.className = 'md-preview hidden';

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
  main.append(area, preview, out);

  const split = document.createElement('div');
  split.className = 'cell-split';
  split.append(main, cellChatTemplate());

  const insert = document.createElement('div');
  insert.className = 'insert-bar';
  insert.append(cellButton('+ Cell', 'btn ghost', 'add-cell'));

  body.append(head, split, insert);
  section.append(gutter, body);
  initCellContext(section);
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
  note.className = 'status cell-chat-note';
  note.textContent = 'This cell only: its SQL and last output, plus material you attach.';

  const sessions = document.createElement('div');
  sessions.className = 'cell-chat-sessions chat-sessions';
  sessions.setAttribute('role', 'tablist');
  sessions.setAttribute('aria-label', 'Cell sessions');

  const fetch = document.createElement('button');
  fetch.type = 'button';
  fetch.className = 'btn cell-chat-fetch';
  fetch.textContent = 'Attach notebook summary';

  const context = document.createElement('textarea');
  context.className = 'cell-chat-context';
  context.rows = 2;
  context.maxLength = 8192;
  context.placeholder = 'Attached notebook material (optional)';

  const attach = document.createElement('div');
  attach.className = 'cell-chat-attach';
  attach.append(fetch, context);

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

  panel.append(heading, sessions, note, attach, history, status, form);
  return panel;
}

document.addEventListener('click', (event) => {
  const target = event.target.closest && event.target.closest('[data-action]');
  if (!target) return;
  const cell = target.closest('.cell');
  if (cell) currentCell = cell;
  switch (target.dataset.action) {
    case 'run': run(target); break;
    case 'run-all': runAll(); break;
    case 'ai': toggleCellChat(cell); break;
    case 'save': save(); break;
    case 'sync': syncNotebook(); break;
    case 'add-cell': addCell(cell); break;
    case 'md-toggle': toggleMarkdownEdit(cell); break;
    case 'up': moveCell(cell, -1); break;
    case 'down': moveCell(cell, 1); break;
    case 'delete': removeCell(cell); break;
  }
});

let currentCell = null;
document.addEventListener('focusin', (event) => {
  const cell = event.target.closest && event.target.closest('.cell');
  if (cell) currentCell = cell;
});

function addCell(after) {
  const id = 'c' + Date.now().toString(36);
  const section = cellTemplate(id, '');
  if (after) after.after(section);
  else cellsRoot.appendChild(section);
  currentCell = section;
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
    if (cell && cell.dataset.kind === 'markdown') {
      event.preventDefault();
      event.stopImmediatePropagation();
      toggleMarkdownEdit(cell);
      return;
    }
    const run = cell ? cell.querySelector('[data-action=run]') : null;
    if (run) {
      event.preventDefault();
      event.stopImmediatePropagation();
      run.click();
    }
  }
});

async function run(btn) {
  const cell = btn.closest('.cell');
  if (cell.dataset.kind === 'markdown') {
    status('markdown cells do not execute queries');
    return;
  }
  const area = cell.querySelector('.editor');
  if (cellKind(area.value) === 'markdown') {
    setCellKind(cell, 'markdown');
    status('markdown cells do not execute queries');
    return;
  }
  if (dirty) {
    await save();
    if (dirty) {
      renderError(cell.querySelector('.out'), 'save the notebook before running on this engine.');
      return;
    }
  }
  const out = cell.querySelector('.out');
  const engine = notebookEngineId();
  const catalog = cell.querySelector('.catalog-context');
  const schema = cell.querySelector('.query-schema');
  if (!catalog || !catalog.value || catalog.disabled || schema.disabled ||
      catalog.selectedOptions[0]?.disabled || schema.selectedOptions[0]?.disabled) {
    renderError(out, 'Select an available catalog and schema context before running.');
    return;
  }

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
      body: JSON.stringify({ sql: cellSource(area.value), engine, notebook: nb.id, cell: cell.dataset.id,
        catalog_context: catalog.value, schema: schema.value || null }),
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
    if (cell.dataset.kind === 'markdown') continue;
    await run(cell.querySelector('.btn-run'));
  }
}

async function save(silent = false) {
  if (!nb) return;
  if (saving) {
    if (silent) scheduleAutosave();
    return;
  }
  if (silent && !dirty) return;
  clearTimeout(autosaveTimer);
  saving = true;
  const generation = editGeneration;
  let res;
  try {
    res = await fetch(notebookPath(), {
      method: 'PUT',
      headers: {
        'content-type': 'application/json',
        ...workspaceHeader(),
        ...(contentRevision ? { 'if-match': '"' + contentRevision + '"' } : { 'if-none-match': '*' }),
      },
      body: JSON.stringify({ ...nb, engine: notebookEngineId(), cells: cellsFromDom() }),
    });
  } catch (error) {
    saving = false;
    status((silent ? 'autosave' : 'save') + ' failed: network error');
    return;
  }
  const data = await res.json().catch(() => ({}));
  saving = false;
  if (!res.ok) {
    if (res.status === 409) {
      autosaveBlocked = true;
      status('save conflict: reload or Save manually');
    } else {
      status((silent ? 'autosave' : 'save') + ' failed: ' + (data.error || res.status));
    }
    return;
  }
  contentRevision = data.content_revision;
  if (generation === editGeneration) {
    setDirty(false);
    status(silent
      ? 'autosaved ' + new Date().toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})
      : (team ? 'saved locally; Sync pending ' : 'saved ') + String(data.revision).slice(0, 8));
  } else {
    status('unsaved changes');
    if (silent) scheduleAutosave();
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
    if (cell && cell.dataset.kind === 'markdown') {
      event.preventDefault();
      toggleMarkdownEdit(cell);
    } else if (cell) {
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
    if (cell.dataset.kind === 'markdown') {
      addCell(cell);
      return;
    }
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
  syncContractDraft(area);
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
loadEngines();
Promise.all([...document.querySelectorAll('.cell')].map(initCellContext)).then(() => {
  for (const cell of document.querySelectorAll('.md-cell')) mountMarkdown(cell);
  autosizeAll();
});
loadHelpers();
if (nb) recordState(null, null);

const createForm = document.getElementById('create');
if (createForm) createForm.addEventListener('submit', createNotebook);


/* Saved conversation state is server-owned; browser state holds only the draft. */
const chatPanel = document.getElementById('chat-panel');
let conversation = null;
let conversationRead = 0;
let chatBusy = false;
let chatSessions = [];
let activeChatSession = null;

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

function shortDate(ms) {
  const value = Number(ms);
  if (!value) return '';
  return new Date(value).toLocaleDateString(undefined, {month: 'short', day: 'numeric'});
}

function renderChatSessions() {
  const strip = document.getElementById('chat-sessions');
  if (!strip) return;
  strip.replaceChildren();
  for (const session of chatSessions) {
    const tab = document.createElement('button');
    tab.type = 'button';
    tab.className = 'chat-session' + (session.id === activeChatSession ? ' active' : '');
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-selected', String(session.id === activeChatSession));
    const phrase = document.createElement('span');
    phrase.className = 'chat-session-phrase';
    phrase.textContent = session.phrase;
    const date = document.createElement('span');
    date.className = 'chat-session-date';
    date.textContent = shortDate(session.createdAtMs);
    tab.append(phrase, date);
    tab.title = `${session.phrase} · ${new Date(Number(session.createdAtMs)).toLocaleString()}`;
    tab.addEventListener('click', () => {
      if (chatBusy || session.id === activeChatSession) return;
      selectChatSession(session.id);
    });
    strip.appendChild(tab);
  }
  const add = document.createElement('button');
  add.type = 'button';
  add.className = 'chat-session chat-session-add';
  add.textContent = '+ New';
  add.setAttribute('aria-label', 'Start a new session');
  add.addEventListener('click', () => {
    newChatSession().catch((error) => {
      document.getElementById('chat-status').textContent = error.message;
    });
  });
  strip.appendChild(add);
}

function renderConversation() {
  const history = document.getElementById('chat-history');
  history.replaceChildren();
  if (!conversation) return;
  if (!(conversation.messages || []).length) {
    const empty = document.createElement('p');
    empty.className = 'chat-empty';
    empty.textContent = 'No messages yet. Ask a question, or attach a cell below.';
    history.appendChild(empty);
    return;
  }
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
        const target = document.createElement('div');
        target.className = 'suggestion-actions';
        const current = document.createElement('button');
        current.type = 'button';
        current.className = 'btn';
        current.textContent = 'Insert into current cell';
        current.addEventListener('click', () => insertIntoSqlCell(suggested, false));
        const add = document.createElement('button');
        add.type = 'button';
        add.className = 'btn';
        add.textContent = 'Insert as new cell';
        add.addEventListener('click', () => insertIntoSqlCell(suggested, true));
        target.append(current, add);
        entry.appendChild(target);
      }
    }
    history.appendChild(entry);
  }
  history.scrollTop = history.scrollHeight;
}

function insertIntoSqlCell(sql, asNew) {
  let cell = asNew ? null : (currentCell || document.querySelector('.cell:last-child'));
  if (!cell) {
    cell = addCell(null);
  }
  if (cell.dataset.kind === 'markdown') {
    cell = addCell(cell);
  }
  const editor = cell.querySelector('.editor');
  editor.value = sql;
  syncContractDraft(editor);
  resize(editor);
  setDirty(true);
  cell.scrollIntoView({block: 'nearest'});
  editor.focus();
}

async function loadChatSessions() {
  const value = await chatRpc('ListConversations', { notebook: nb.id });
  chatSessions = value.sessions || [];
  renderChatSessions();
  return chatSessions;
}

async function newChatSession() {
  if (chatBusy) return;
  conversationRead += 1;
  const created = await chatRpc('NewConversation', { notebook: nb.id });
  activeChatSession = created.id;
  conversation = created;
  await loadChatSessions();
  renderChatSessions();
  renderConversation();
  document.getElementById('chat-status').textContent = 'New session.';
  document.getElementById('chat-prompt').focus();
}

async function selectChatSession(id) {
  const current = ++conversationRead;
  try {
    const loaded = await chatRpc('GetConversation', { notebook: nb.id, session: id });
    if (current !== conversationRead) return;
    activeChatSession = id;
    conversation = loaded;
    renderChatSessions();
    renderConversation();
    document.getElementById('chat-status').textContent = '';
  } catch (error) {
    document.getElementById('chat-status').textContent = error.message;
  }
}

async function loadConversation() {
  const current = ++conversationRead;
  let loaded;
  try {
    loaded = activeChatSession
      ? await chatRpc('GetConversation', { notebook: nb.id, session: activeChatSession })
      : await chatRpc('GetConversation', { notebook: nb.id });
  } catch (error) {
    if (current !== conversationRead) return false;
    conversation = null;
    document.getElementById('chat-history').replaceChildren();
    throw error;
  }
  if (current !== conversationRead) return false;
  activeChatSession = loaded.id;
  if (!conversation || conversation.id !== loaded.id ||
      BigInt(loaded.revision || '0') >= BigInt(conversation.revision || '0')) {
    conversation = loaded;
    renderConversation();
  }
  return true;
}

async function fetchCellMaterial(kind) {
  const select = document.getElementById('chat-cell');
  const context = document.getElementById('chat-context');
  if (!select || !context || !select.value) return;
  const status = document.getElementById('chat-status');
  try {
    if (kind === 'query') {
      const value = await chatRpc('FetchQuery', { notebook: nb.id, cell: select.value });
      const text = `Cell ${select.value} SQL (revision ${String(value.contentRevision).slice(0, 8)}):\n${value.sql}`;
      context.value = (context.value ? context.value + '\n\n' : '') + text;
    } else {
      const value = await chatRpc('FetchResult', { notebook: nb.id, cell: select.value });
      let text = `Cell ${select.value} last result (${value.rowsJson.length} rows${value.truncated ? ', truncated' : ''}):\n`;
      text += `columns: ${(value.columns || []).join(', ')}\n`;
      text += (value.rowsJson || []).slice(0, 20).join('\n');
      context.value = (context.value ? context.value + '\n\n' : '') + text;
    }
    status.textContent = 'Attached to the next message.';
  } catch (error) {
    status.textContent = error.message;
  }
}

async function openChat() {
  chatPanel.hidden = false;
  document.body.classList.add('chat-open');
  document.getElementById('chat-toggle').setAttribute('aria-expanded', 'true');
  const cellSelect = document.getElementById('chat-cell');
  const selected = cellSelect.value;
  cellSelect.replaceChildren();
  for (const cell of document.querySelectorAll('.cell')) {
    if (cell.dataset.kind === 'markdown') continue;
    const sql = cell.querySelector('.editor').value.split('\n').find((line) => line.trim()) || '';
    cellSelect.add(new Option(`${cell.dataset.id} · ${sql}`, cell.dataset.id));
  }
  if (selected) cellSelect.value = selected;
  document.getElementById('chat-prompt').focus();
  if (!chatBusy) {
    document.getElementById('chat-send').disabled = true;
    try {
      await loadChatSessions();
      if (!chatSessions.some((session) => session.id === activeChatSession)) {
        if (chatSessions.length > 0) activeChatSession = chatSessions[0].id;
        else await newChatSession();
      }
      if (await loadConversation()) {
        await loadChatSessions();
        renderChatSessions();
        document.getElementById('chat-status').textContent = '';
      }
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
  const cellSelect = document.getElementById('chat-cell');
  function refreshChatCellOptions() {
    const current = cellSelect.value;
    cellSelect.replaceChildren();
    for (const cell of document.querySelectorAll('.cell')) {
      if (cell.dataset.kind === 'markdown') continue;
      const sql = cell.querySelector('.editor').value.split('\n').find((line) => line.trim()) || '';
      cellSelect.add(new Option(`${cell.dataset.id} · ${sql}`, cell.dataset.id));
    }
    if (current) cellSelect.value = current;
  }
  refreshChatCellOptions();

  document.getElementById('chat-toggle').addEventListener('click', () => chatPanel.hidden ? openChat() : closeChat());
  document.getElementById('chat-close').addEventListener('click', closeChat);
  document.getElementById('chat-fetch-query').addEventListener('click', () => fetchCellMaterial('query'));
  document.getElementById('chat-fetch-result').addEventListener('click', () => fetchCellMaterial('result'));
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
      if (!conversation) throw new Error('Reload authorized history before sending.');
      const current = conversationRead;
      const sent = await chatRpc('SendMessage', {
        ...contractForTurn(),
        notebook: nb.id, helper: chosen, prompt: prompt.value,
        context: context.value, expectedRevision: conversation.revision || '0',
        ...(activeChatSession ? {session: activeChatSession} : {}),
      });
      if (current !== conversationRead) await loadConversation();
      else { conversation = sent; activeChatSession = sent.id; renderConversation(); }
      await loadChatSessions();
      renderChatSessions();
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

/* One history per cell session, kept in the browser only; the server owns the
 * stored conversation under the notebook+cell key. */
const cellChats = new Map();

function cellChatParts(cell) {
  return {
    panel: cell.querySelector('.cell-chat'),
    sessions: cell.querySelector('.cell-chat-sessions'),
    history: cell.querySelector('.cell-chat-history'),
    status: cell.querySelector('.cell-chat-status'),
    prompt: cell.querySelector('.cell-chat-prompt'),
    context: cell.querySelector('.cell-chat-context'),
    send: cell.querySelector('.cell-chat-send'),
    fetch: cell.querySelector('.cell-chat-fetch'),
  };
}

function cellChatState(id) {
  if (!cellChats.has(id)) {
    cellChats.set(id, {
      busy: false,
      read: 0,
      sessions: [],
      active: null,
      conversation: null,
    });
  }
  return cellChats.get(id);
}

/* The cell's own SQL and last output, plus explicit notebook material the user
 * attached. Never the notebook conversation, never another cell. */
function cellChatContext(cell) {
  const parts = cellChatParts(cell);
  const sql = cellSource(cell.querySelector('.editor').value).trim();
  const out = cell.querySelector('.out-content');
  const output = (out ? out.innerText : '').trim().slice(0, 4000);
  let text = sql ? 'Current cell SQL:\n' + sql : '';
  if (output) text += (text ? '\n\n' : '') + 'Last run output:\n' + output;
  const attached = (parts.context?.value || '').trim();
  if (attached) text += (text ? '\n\n' : '') + attached;
  return text.slice(0, 8192);
}

function renderCellSessions(cell) {
  const parts = cellChatParts(cell);
  const state = cellChatState(cell.dataset.id);
  parts.sessions.replaceChildren();
  for (const session of state.sessions) {
    const tab = document.createElement('button');
    tab.type = 'button';
    tab.className = 'chat-session' + (session.id === state.active ? ' active' : '');
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-selected', String(session.id === state.active));
    const phrase = document.createElement('span');
    phrase.className = 'chat-session-phrase';
    phrase.textContent = session.phrase;
    const date = document.createElement('span');
    date.className = 'chat-session-date';
    date.textContent = shortDate(session.createdAtMs);
    tab.append(phrase, date);
    tab.title = `${session.phrase} · ${new Date(Number(session.createdAtMs)).toLocaleString()}`;
    tab.addEventListener('click', () => {
      if (state.busy || session.id === state.active) return;
      selectCellSession(cell, session.id);
    });
    parts.sessions.appendChild(tab);
  }
  const add = document.createElement('button');
  add.type = 'button';
  add.className = 'chat-session chat-session-add';
  add.textContent = '+ New';
  add.setAttribute('aria-label', 'Start a new cell session');
  add.addEventListener('click', () => newCellSession(cell).catch((error) => {
    cellChatParts(cell).status.textContent = error.message;
  }));
  parts.sessions.appendChild(add);
}

function renderCellChat(cell) {
  const { history } = cellChatParts(cell);
  const state = cellChatState(cell.dataset.id);
  history.replaceChildren();
  if (!(state.conversation?.messages || []).length) {
    const empty = document.createElement('p');
    empty.className = 'chat-empty';
    empty.textContent = 'No messages yet. Ask about this cell, or attach the notebook summary.';
    history.appendChild(empty);
    return;
  }
  for (const message of state.conversation?.messages || []) {
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
        const target = document.createElement('div');
        target.className = 'suggestion-actions';
        const apply = document.createElement('button');
        apply.type = 'button';
        apply.className = 'btn';
        apply.textContent = 'Replace this cell';
        apply.addEventListener('click', () => {
          const editor = cell.querySelector('.editor');
          editor.value = suggested;
          if (cell.dataset.kind === 'markdown') editor.value = '%md\n' + suggested;
          syncContractDraft(editor);
          resize(editor);
          setDirty(true);
        });
        const future = document.createElement('button');
        future.type = 'button';
        future.className = 'btn';
        future.textContent = 'Insert as new cell';
        future.addEventListener('click', () => insertIntoSqlCell(suggested, true));
        target.append(apply, future);
        entry.appendChild(target);
      }
    }
    history.appendChild(entry);
  }
  history.scrollTop = history.scrollHeight;
}

function invalidateCellChat(cell) {
  const state = cellChatState(cell.dataset.id);
  state.read += 1;
  state.conversation = null;
  renderCellChat(cell);
}

async function loadCellChatSessions(cell) {
  const state = cellChatState(cell.dataset.id);
  const value = await chatRpc('ListConversations', { notebook: nb.id, cell: cell.dataset.id });
  state.sessions = value.sessions || [];
  renderCellSessions(cell);
  return state.sessions;
}

async function selectCellSession(cell, id) {
  const state = cellChatState(cell.dataset.id);
  const current = ++state.read;
  try {
    const loaded = await chatRpc('GetConversation', { notebook: nb.id, cell: cell.dataset.id, session: id });
    if (current !== state.read) return;
    state.active = id;
    state.conversation = loaded;
    renderCellSessions(cell);
    renderCellChat(cell);
    cellChatParts(cell).status.textContent = '';
  } catch (error) {
    cellChatParts(cell).status.textContent = error.message;
  }
}

async function newCellSession(cell) {
  const state = cellChatState(cell.dataset.id);
  if (state.busy) return;
  state.read += 1;
  const created = await chatRpc('NewConversation', { notebook: nb.id, cell: cell.dataset.id });
  state.active = created.id;
  state.conversation = created;
  await loadCellChatSessions(cell);
  renderCellSessions(cell);
  renderCellChat(cell);
  cellChatParts(cell).status.textContent = 'New session.';
  cellChatParts(cell).prompt.focus();
}

async function loadCellChat(cell) {
  const state = cellChatState(cell.dataset.id);
  const current = ++state.read;
  let loaded;
  try {
    loaded = state.active
      ? await chatRpc('GetConversation', { notebook: nb.id, cell: cell.dataset.id, session: state.active })
      : await chatRpc('GetConversation', { notebook: nb.id, cell: cell.dataset.id });
  } catch (error) {
    if (current !== state.read) return false;
    invalidateCellChat(cell);
    throw error;
  }
  if (current !== state.read) return false;
  state.active = loaded.id;
  state.conversation = loaded;
  renderCellChat(cell);
  return true;
}

async function openCellChat(cell) {
  const parts = cellChatParts(cell);
  const state = cellChatState(cell.dataset.id);
  parts.panel.hidden = false;
  parts.prompt.focus();
  try {
    await loadCellChatSessions(cell);
    if (!state.sessions.some((session) => session.id === state.active)) {
      if (state.sessions.length > 0) state.active = state.sessions[0].id;
      else await newCellSession(cell);
    }
    if (await loadCellChat(cell)) {
      await loadCellChatSessions(cell);
      renderCellSessions(cell);
      parts.status.textContent = '';
    }
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

async function fetchNotebookSummary(cell) {
  const parts = cellChatParts(cell);
  try {
    const value = await chatRpc('FetchSummary', { notebook: nb.id });
    let text = value.summary ? `Notebook summary:\n${value.summary}` : 'Notebook summary: none';
    const cells = value.cells || [];
    if (cells.length > 0) {
      text += '\n\nCell index:\n' + cells.map((entry) => `- ${entry.id}: ${entry.sql}`).join('\n');
    }
    parts.context.value = (parts.context.value ? parts.context.value + '\n\n' : '') + text;
    parts.status.textContent = 'Attached to the next cell message.';
  } catch (error) {
    parts.status.textContent = error.message;
  }
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
    if (!state.conversation) await loadCellChat(cell);
    if (!state.conversation) throw new Error('Reload authorized history before sending.');
    const current = state.read;
    const sent = await chatRpc('SendMessage', {
      ...contractForTurn(),
      notebook: nb.id, cell: cell.dataset.id, helper: chosen,
      prompt: parts.prompt.value, context: cellChatContext(cell),
      expectedRevision: state.conversation.revision || '0',
      ...(state.active ? {session: state.active} : {}),
    });
    if (current !== state.read) await loadCellChat(cell);
    else {
      state.active = sent.id;
      state.conversation = sent;
      renderCellChat(cell);
    }
    await loadCellChatSessions(cell);
    renderCellSessions(cell);
    parts.prompt.value = '';
    parts.context.value = '';
    parts.status.textContent = 'Saved. Replies do not execute queries.';
  } catch (error) {
    invalidateCellChat(cell);
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
    if (close) {
      const cell = close.closest('.cell');
      if (cell) closeCellChat(cell);
      return;
    }
    const fetch = event.target.closest && event.target.closest('.cell-chat-fetch');
    if (fetch) {
      const cell = fetch.closest('.cell');
      if (cell) fetchNotebookSummary(cell);
    }
  });
  cellsRoot.addEventListener('submit', (event) => {
    if (!event.target.classList.contains('cell-chat-form')) return;
    event.preventDefault();
    const cell = event.target.closest('.cell');
    if (cell) sendCellChat(cell);
  });
}

/* ---- catalog workspace ---------------------------------------------- */

const catalogPage = document.getElementById('catalog-page');
if (catalogPage) {
  const treeRoot = document.getElementById('catalog-tree');
  const detailRoot = document.getElementById('catalog-detail');
  const navStatus = document.getElementById('catalog-nav-status');
  const menuButton = document.getElementById('catalog-menu');
  const rail = document.getElementById('catalog-rail');
  const searchInput = document.getElementById('catalog-search');
  const searchResults = document.getElementById('catalog-search-results');
  const docsEnabled = catalogPage.dataset.docs === 'enabled';
  let selected = { catalog: null, namespace: null, table: null };
  let currentDetail = null;
  let currentTab = 'details';
  let detailSeq = 0;

  async function catalogJson(url) {
    const response = await fetch(url, {cache: 'no-store'});
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(value.error || value.message || 'catalog request failed');
    return value;
  }

  function treeButton(label, className, onClick) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = className;
    button.textContent = label;
    button.addEventListener('click', onClick);
    return button;
  }

  function catalogUrl(catalog, namespace, table) {
    const params = new URLSearchParams();
    if (catalog) params.set('catalog', catalog);
    if (namespace) params.set('namespace', namespace);
    if (table) params.set('table', table);
    return '/catalog?' + params.toString();
  }

  function updateLocation() {
    const url = catalogUrl(selected.catalog, selected.namespace, selected.table);
    history.replaceState(null, '', url);
  }

  async function expandCatalog(node, catalog) {
    const children = node.querySelector('.tree-children');
    if (!children.hidden) return;
    children.hidden = false;
    node.querySelector('.tree-toggle').textContent = '▾';
    if (children.dataset.loaded === '1') return;
    navStatus.textContent = 'Loading schemas…';
    try {
      const value = await catalogJson('/api/catalog/tree?catalog=' + encodeURIComponent(catalog));
      children.replaceChildren();
      for (const namespace of value.namespaces || []) {
        const child = document.createElement('div');
        child.className = 'tree-node schema-node';
        child.dataset.name = namespace.name;
        child.dataset.token = namespace.token;
        const head = document.createElement('div');
        head.className = 'tree-row';
        const toggle = treeButton('▸', 'tree-toggle', () => expandSchema(child, catalog, namespace.token));
        const label = treeButton(namespace.name, 'tree-label', () => selectEntity(catalog, namespace.token, null));
        head.append(toggle, label);
        const grandchildren = document.createElement('div');
        grandchildren.className = 'tree-children';
        grandchildren.hidden = true;
        child.append(head, grandchildren);
        children.appendChild(child);
      }
      children.dataset.loaded = '1';
      navStatus.textContent = '';
    } catch (error) {
      navStatus.textContent = error.message;
    }
  }

  async function expandSchema(node, catalog, namespace) {
    const children = node.querySelector('.tree-children');
    if (!children.hidden) return;
    children.hidden = false;
    node.querySelector('.tree-toggle').textContent = '▾';
    if (children.dataset.loaded === '1') return;
    navStatus.textContent = 'Loading tables…';
    try {
      const value = await catalogJson('/api/catalog/tree?catalog=' + encodeURIComponent(catalog) + '&namespace=' + encodeURIComponent(namespace));
      children.replaceChildren();
      for (const table of value.tables || []) {
        const child = document.createElement('div');
        child.className = 'tree-node table-node';
        child.dataset.name = table.name;
        const head = document.createElement('div');
        head.className = 'tree-row';
        const spacer = document.createElement('span');
        spacer.className = 'tree-spacer';
        const label = treeButton(table.name, 'tree-label', () => selectEntity(catalog, namespace, table.name));
        head.append(spacer, label);
        child.appendChild(head);
        children.appendChild(child);
      }
      children.dataset.loaded = '1';
      navStatus.textContent = '';
    } catch (error) {
      navStatus.textContent = error.message;
    }
  }

  function renderCatalogs(value) {
    treeRoot.replaceChildren();
    for (const catalog of value.catalogs || []) {
      const node = document.createElement('div');
      node.className = 'tree-node catalog-node';
      node.dataset.name = catalog.id;
      const head = document.createElement('div');
      head.className = 'tree-row';
      const toggle = treeButton('▸', 'tree-toggle', () => expandCatalog(node, catalog.id));
      const label = treeButton(catalog.id, 'tree-label', () => selectEntity(catalog.id, null, null));
      const chip = document.createElement('span');
      chip.className = 'chip';
      chip.textContent = catalog.kind;
      head.append(toggle, label, chip);
      const children = document.createElement('div');
      children.className = 'tree-children';
      children.hidden = true;
      node.append(head, children);
      treeRoot.appendChild(node);
    }
    if (!(value.catalogs || []).length) {
      treeRoot.innerHTML = '<p class="status">No visible catalogs.</p>';
    }
  }

  async function selectEntity(catalog, namespace, table, tab = 'details') {
    const sequence = ++detailSeq;
    selected = { catalog, namespace, table };
    updateLocation();
    detailRoot.innerHTML = '<p class="status">Loading…</p>';
    try {
      const params = new URLSearchParams({catalog});
      if (namespace) params.set('namespace', namespace);
      if (table) params.set('table', table);
      const loaded = await catalogJson('/api/catalog/detail?' + params.toString());
      if (sequence !== detailSeq) return;
      currentDetail = loaded;
      currentTab = tab;
      renderDetail();
    } catch (error) {
      detailRoot.innerHTML = '';
      const message = document.createElement('p');
      message.className = 'err';
      message.textContent = error.message;
      detailRoot.appendChild(message);
    }
  }

  function detailTitle() {
    const entity = currentDetail.entity;
    const parts = [entity.catalog];
    if (entity.namespace) parts.push(currentDetail.details.namespace || entity.namespace);
    if (entity.table) parts.push(entity.table);
    return parts.join(' / ');
  }

  function addField(parent, label, value) {
    const row = document.createElement('div');
    row.className = 'detail-field';
    const name = document.createElement('span');
    name.className = 'detail-label';
    name.textContent = label;
    const content = document.createElement('span');
    content.textContent = value === null || value === undefined || value === '' ? '—' : String(value);
    row.append(name, content);
    parent.appendChild(row);
  }

  function renderDetail() {
    const detail = currentDetail;
    detailRoot.replaceChildren();
    const head = document.createElement('div');
    head.className = 'catalog-detail-head';
    const title = document.createElement('h1');
    title.textContent = detailTitle();
    const level = document.createElement('span');
    level.className = 'chip';
    level.textContent = detail.level;
    head.append(title, level);
    detailRoot.appendChild(head);

    const tabs = document.createElement('div');
    tabs.className = 'detail-tabs';
    for (const [id, label] of [['details', 'Details'], ['docs', 'Docs'], ['history', 'History'], ['permissions', 'Permissions']]) {
      const tab = treeButton(label, 'btn detail-tab' + (currentTab === id ? ' active' : ''), () => {
        currentTab = id;
        renderDetail();
      });
      tab.dataset.tab = id;
      tabs.appendChild(tab);
    }
    detailRoot.appendChild(tabs);

    const pane = document.createElement('div');
    pane.className = 'detail-pane';
    detailRoot.appendChild(pane);
    if (currentTab === 'details') renderDetailsPane(pane);
    if (currentTab === 'docs') renderDocsPane(pane);
    if (currentTab === 'history') renderHistoryPane(pane);
    if (currentTab === 'permissions') renderPermissionsPane(pane);
  }

  function renderDetailsPane(pane) {
    const details = currentDetail.details;
    const panel = document.createElement('div');
    panel.className = 'panel';
    const body = document.createElement('div');
    body.className = 'panel-body';
    addField(body, 'Catalog', details.catalog);
    addField(body, 'Kind', details.kind);
    addField(body, 'Health', details.health);
    if (details.namespace) addField(body, 'Namespace', details.namespace);
    if (details.table) {
      addField(body, 'Table', details.table);
      addField(body, 'Format', details.format);
      addField(body, 'Base location', details.baseLocation);
      addField(body, 'Schema available', details.schemaAvailable ? 'yes' : 'no');
    }
    if (details.tableCount !== undefined) addField(body, 'Tables', details.tableCount);
    panel.appendChild(body);
    pane.appendChild(panel);

    if (details.columns) {
      const columns = document.createElement('div');
      columns.className = 'panel';
      const table = document.createElement('table');
      table.className = 'schema';
      table.innerHTML = '<thead><tr><th>column</th><th>type</th><th>nullable</th></tr></thead>';
      const tbody = document.createElement('tbody');
      for (const column of details.columns) {
        const row = document.createElement('tr');
        const name = document.createElement('td');
        name.innerHTML = '<code></code>';
        name.firstChild.textContent = column.name;
        const type = document.createElement('td');
        type.textContent = column.type || column.data_type;
        const nullable = document.createElement('td');
        nullable.textContent = column.nullable ? 'yes' : 'no';
        row.append(name, type, nullable);
        tbody.appendChild(row);
      }
      table.appendChild(tbody);
      columns.appendChild(table);
      pane.appendChild(columns);
    }
  }

  function renderDocsPane(pane) {
    const docs = currentDetail.docs;
    if (!docs.enabled) {
      const note = document.createElement('p');
      note.className = 'empty';
      note.textContent = 'Catalog docs are disabled: set ASTER_CATALOG_ASSETS_DIR to a Git checkout.';
      pane.appendChild(note);
      return;
    }
    const panel = document.createElement('div');
    panel.className = 'panel';
    const body = document.createElement('div');
    body.className = 'panel-body';
    if (docs.viewRevision) {
      const note = document.createElement('p');
      note.className = 'status';
      note.textContent = 'Read-only revision ' + docs.viewRevision.slice(0, 12) + '.';
      body.appendChild(note);
    } else {
      const actions = document.createElement('div');
      actions.className = 'docs-actions';
      const edit = treeButton('Edit', 'btn', () => startDocEdit(pane));
      actions.appendChild(edit);
      body.appendChild(actions);
    }
    const preview = document.createElement('div');
    preview.className = 'md-preview docs-preview';
    renderMarkdownInto(preview, docs.markdown || '');
    body.appendChild(preview);
    const editor = document.createElement('div');
    editor.className = 'docs-editor';
    editor.hidden = true;
    const area = document.createElement('textarea');
    area.className = 'editor';
    area.rows = 16;
    area.value = docs.markdown || '';
    const save = treeButton('Save docs', 'btn primary', () => saveDocs(pane, area.value, docs.revision));
    const cancel = treeButton('Cancel', 'btn', () => startDocView(pane));
    const status = document.createElement('p');
    status.className = 'status';
    editor.append(area, save, cancel, status);
    body.appendChild(editor);
    panel.appendChild(body);
    pane.appendChild(panel);

    if (docs.path) {
      const meta = document.createElement('p');
      meta.className = 'status';
      meta.textContent = 'Git path: ' + docs.path + (docs.revision ? ' · blob ' + docs.revision.slice(0, 12) : '');
      pane.appendChild(meta);
    }
    if ((currentDetail.assets || []).length) {
      const gallery = document.createElement('div');
      gallery.className = 'asset-gallery';
      for (const asset of currentDetail.assets) {
        const params = new URLSearchParams({catalog: currentDetail.entity.catalog, name: asset.name});
        if (currentDetail.entity.namespace) params.set('namespace', currentDetail.entity.namespace);
        if (currentDetail.entity.table) params.set('table', currentDetail.entity.table);
        const link = document.createElement('a');
        link.href = '/api/catalog/asset?' + params.toString();
        link.target = '_blank';
        link.rel = 'noopener';
        const image = document.createElement('img');
        image.loading = 'lazy';
        image.src = link.href;
        image.alt = asset.name;
        link.appendChild(image);
        gallery.appendChild(link);
      }
      pane.appendChild(gallery);
    }
  }

  function startDocEdit(pane) {
    const preview = pane.querySelector('.docs-preview');
    const editor = pane.querySelector('.docs-editor');
    if (preview) preview.hidden = true;
    if (editor) editor.hidden = false;
  }

  function startDocView(pane) {
    const preview = pane.querySelector('.docs-preview');
    const editor = pane.querySelector('.docs-editor');
    if (preview) preview.hidden = false;
    if (editor) editor.hidden = true;
  }

  async function saveDocs(pane, markdown, expectedRevision) {
    const status = pane.querySelector('.docs-editor .status');
    status.textContent = 'Saving…';
    const body = {catalog: currentDetail.entity.catalog, markdown, expectedRevision: expectedRevision || null};
    if (currentDetail.entity.namespace) body.namespace = currentDetail.entity.namespace;
    if (currentDetail.entity.table) body.table = currentDetail.entity.table;
    const response = await fetch('/api/catalog/docs', {
      method: 'PUT',
      headers: {'content-type': 'application/json'},
      body: JSON.stringify(body),
    });
    const value = await response.json().catch(() => ({}));
    if (!response.ok) {
      status.textContent = response.status === 409
        ? 'Document changed; reload before saving.'
        : (value.error || 'save failed');
      return;
    }
    await selectEntity(selected.catalog, selected.namespace, selected.table, 'docs');
    navStatus.textContent = 'Docs saved.';
  }

  function renderHistoryPane(pane) {
    const history = currentDetail.history;
    const docsPanel = document.createElement('div');
    docsPanel.className = 'panel';
    const docsHead = document.createElement('div');
    docsHead.className = 'panel-head';
    docsHead.textContent = 'Document history';
    const docsBody = document.createElement('div');
    docsBody.className = 'panel-body';
    if (!(history.docs || []).length) {
      docsBody.innerHTML = '<p class="status">No document commits.</p>';
    } else {
      const list = document.createElement('ul');
      list.className = 'history-list';
      for (const commit of history.docs) {
        const item = document.createElement('li');
        const button = treeButton(commit.message || commit.oid.slice(0, 12), 'tree-label', () => {
          viewDocRevision(commit.oid);
        });
        const meta = document.createElement('span');
        meta.className = 'status';
        meta.textContent = `${commit.author} · ${commit.date} · ${commit.oid.slice(0, 12)}`;
        item.append(button, meta);
        list.appendChild(item);
      }
      docsBody.appendChild(list);
    }
    docsPanel.append(docsHead, docsBody);
    pane.appendChild(docsPanel);

    const queryPanel = document.createElement('div');
    queryPanel.className = 'panel';
    const queryHead = document.createElement('div');
    queryHead.className = 'panel-head';
    queryHead.textContent = 'Recent queries by you';
    const queryBody = document.createElement('div');
    queryBody.className = 'panel-body';
    if (!(history.queries || []).length) {
      queryBody.innerHTML = '<p class="status">No recorded queries for this scope.</p>';
    } else {
      const list = document.createElement('ul');
      list.className = 'history-list';
      for (const query of history.queries) {
        const item = document.createElement('li');
        const sql = document.createElement('code');
        sql.textContent = query.sql;
        const meta = document.createElement('span');
        meta.className = 'status';
        meta.textContent = `${query.engine} · ${query.ok ? 'ok' : 'failed'} · ${query.rowCount} row(s)`;
        item.append(sql, meta);
        list.appendChild(item);
      }
      queryBody.appendChild(list);
    }
    queryPanel.append(queryHead, queryBody);
    pane.appendChild(queryPanel);
  }

  async function viewDocRevision(oid) {
    const params = new URLSearchParams({catalog: currentDetail.entity.catalog, revision: oid});
    if (currentDetail.entity.namespace) params.set('namespace', currentDetail.entity.namespace);
    if (currentDetail.entity.table) params.set('table', currentDetail.entity.table);
    try {
      const value = await catalogJson('/api/catalog/docs?' + params.toString());
      if (!value.document) return;
      currentTab = 'docs';
      currentDetail.docs = {
        ...currentDetail.docs,
        markdown: value.document.markdown,
        viewRevision: oid,
      };
      renderDetail();
    } catch (error) {
      navStatus.textContent = error.message;
    }
  }

  function renderPermissionsPane(pane) {
    const permissions = currentDetail.permissions;
    const panel = document.createElement('div');
    panel.className = 'panel';
    const body = document.createElement('div');
    body.className = 'panel-body';
    addField(body, 'Metadata', permissions.metadata);
    if (permissions.contract) {
      addField(body, 'Contract', `${permissions.contract.id} ${permissions.contract.version}`);
      addField(body, 'Contract path', permissions.contract.path);
      addField(body, 'Contract admission', permissions.contract.admission);
    } else {
      addField(body, 'Contract', 'not declared in the compiled bundle');
    }
    const engineTitle = document.createElement('h2');
    engineTitle.textContent = 'Bound engines';
    body.appendChild(engineTitle);
    if (!(permissions.engines || []).length) {
      body.innerHTML += '<p class="status">No bound engines.</p>';
    } else {
      const table = document.createElement('table');
      table.className = 'schema';
      table.innerHTML = '<thead><tr><th>engine</th><th>grant</th><th>health</th></tr></thead>';
      const tbody = document.createElement('tbody');
      for (const engine of permissions.engines) {
        const row = document.createElement('tr');
        const id = document.createElement('td');
        id.textContent = engine.id;
        const grant = document.createElement('td');
        const badge = document.createElement('span');
        badge.className = engine.granted ? 'badge ok' : 'badge warn';
        badge.textContent = engine.granted ? 'granted' : 'not granted';
        grant.appendChild(badge);
        const health = document.createElement('td');
        health.textContent = engine.health;
        row.append(id, grant, health);
        tbody.appendChild(row);
      }
      table.appendChild(tbody);
      body.appendChild(table);
    }
    panel.appendChild(body);
    pane.appendChild(panel);
  }

  async function initCatalog() {
    try {
      const value = await catalogJson('/api/catalog/tree');
      renderCatalogs(value);
      const params = new URLSearchParams(location.search);
      const catalog = params.get('catalog');
      if (!catalog) return;
      const node = treeRoot.querySelector(`.catalog-node[data-name="${CSS.escape(catalog)}"]`);
      if (!node) return;
      await expandCatalog(node, catalog);
      const namespace = params.get('namespace');
      const table = params.get('table');
      if (namespace) {
        const schema = node.querySelector(`.schema-node[data-token="${CSS.escape(namespace)}"]`);
        if (schema) await expandSchema(schema, catalog, namespace);
      }
      await selectEntity(catalog, namespace, table);
    } catch (error) {
      navStatus.textContent = error.message;
    }
  }

  function setMenu(open, persist = true) {
    catalogPage.classList.toggle('rail-closed', !open);
    if (rail) rail.hidden = !open;
    menuButton.textContent = open ? '☰ Hide menu' : '☰ Show menu';
    menuButton.setAttribute('aria-expanded', String(open));
    if (persist) localStorage.setItem('aster.catalog.menu', open ? 'open' : 'closed');
  }

  menuButton.addEventListener('click', () => {
    setMenu(catalogPage.classList.contains('rail-closed'));
  });

  function clearSearch() {
    searchResults.hidden = true;
    searchResults.replaceChildren();
  }

  function showSearchResults(results) {
    searchResults.replaceChildren();
    if (!results.length) {
      const empty = document.createElement('p');
      empty.className = 'status';
      empty.textContent = 'No matching catalogs, schemas or tables.';
      searchResults.appendChild(empty);
    } else {
      for (const result of results) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'search-result';
        const label = document.createElement('span');
        label.textContent = result.label;
        const kind = document.createElement('span');
        kind.className = 'status';
        kind.textContent = result.kind;
        button.append(label, kind);
        button.addEventListener('click', () => selectSearchResult(result));
        searchResults.appendChild(button);
      }
    }
    searchResults.hidden = false;
  }

  async function selectSearchResult(result) {
    clearSearch();
    const node = treeRoot.querySelector(`.catalog-node[data-name="${CSS.escape(result.catalog)}"]`);
    if (!node) return;
    await expandCatalog(node, result.catalog);
    if (result.namespace) {
      const schema = node.querySelector(`.schema-node[data-token="${CSS.escape(result.namespace)}"]`);
      if (schema) await expandSchema(schema, result.catalog, result.namespace);
    }
    await selectEntity(result.catalog, result.namespace, result.table);
  }

  let searchTimer = null;
  let searchIndex = null;
  let searchIndexPromise = null;

  async function ensureSearchIndex() {
    if (searchIndex) return searchIndex;
    searchIndexPromise ||= catalogJson('/api/catalog/tree?all=true').then((value) => {
      const entries = [];
      for (const catalog of value.catalogs || []) {
        entries.push({catalog: catalog.id, namespace: null, table: null, kind: 'catalog', label: catalog.id});
        for (const namespace of catalog.namespaces || []) {
          entries.push({catalog: catalog.id, namespace: namespace.token, table: null, kind: 'schema', label: `${catalog.id} / ${namespace.name}`});
          for (const table of namespace.tables || []) {
            entries.push({catalog: catalog.id, namespace: namespace.token, table: table.name, kind: 'table', label: `${catalog.id} / ${namespace.name} / ${table.name}`});
          }
        }
      }
      searchIndex = entries;
      return entries;
    });
    return searchIndexPromise;
  }

  searchInput.addEventListener('input', () => {
    clearTimeout(searchTimer);
    const query = searchInput.value.trim().toLowerCase();
    if (query.length < 2) {
      clearSearch();
      return;
    }
    searchTimer = setTimeout(async () => {
      try {
        navStatus.textContent = 'Searching…';
        const entries = await ensureSearchIndex();
        const matches = entries
          .filter((entry) => entry.label.toLowerCase().includes(query))
          .slice(0, 50);
        showSearchResults(matches);
        navStatus.textContent = '';
      } catch (error) {
        navStatus.textContent = error.message;
      }
    }, 120);
  });

  searchInput.addEventListener('blur', () => setTimeout(clearSearch, 150));

  const savedMenu = localStorage.getItem('aster.catalog.menu');
  setMenu(savedMenu ? savedMenu === 'open' : !window.matchMedia('(max-width: 900px)').matches, false);

  initCatalog();
}
