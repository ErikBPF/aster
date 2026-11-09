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
const notebookData = document.getElementById('notebook-data');
let legacyContextMixed = false;
let computeButton = null;
if (notebookEngine) {
  computeButton = cellButton('Compute: loading…', 'btn', '');
  notebookEngine.closest('label').before(computeButton);
  const popup = notebookPopover(computeButton, 'Notebook compute');
  popup.append(notebookEngine.closest('label'));
  const note = document.createElement('p'); note.className = 'status';
  note.textContent = 'Applies to every SQL cell. Changing compute does not run queries or change data context.';
  popup.append(note);
}
function updateComputeLabel() {
  if (computeButton) computeButton.textContent = 'Compute: ' + (notebookEngine.selectedOptions[0]?.textContent || 'unavailable');
}
const legacyEngineMode = !nb?.engine && (nb?.cells || []).some((cell) => cell.engine);

function markLegacyEngine() {
  if (!legacyEngineMode || !notebookEngine) return;
  const option = new Option('legacy per-cell engines', '__legacy');
  notebookEngine.prepend(option);
  notebookEngine.value = '__legacy';
}

async function loadEngines() {
  if (!notebookEngine) return;
  const current = nb?.engine || notebookEngine.dataset.current || '';
  fillEngines(notebookEngine, current);
  markLegacyEngine();
  notebookEngine.disabled = true;
  try {
    const response = await fetch('/api/engines', {cache: 'no-store'});
    if (!response.ok) throw new Error('Engine choices unavailable');
    const choices = await response.json();
    fillEngines(notebookEngine, current, new Map(choices.map((entry) => [entry.id, entry])));
    markLegacyEngine();
    for (const option of notebookEngine.options) {
      if (option.value && !choices.some((entry) => entry.id === option.value)) option.disabled = true;
    }
    notebookEngine.disabled = choices.length === 0;
  } catch (error) {
    status(error.message);
  } finally { updateComputeLabel(); }
}

function notebookEngineId() {
  if (!notebookEngine || notebookEngine.value === '__legacy') return null;
  return notebookEngine.value || null;
}

notebookEngine?.addEventListener('change', () => { setDirty(true); updateComputeLabel(); });

function fillEngines(select, current, engineInfo = new Map()) {
  select.replaceChildren();
  const fallback = document.createElement('option');
  fallback.value = '';
  fallback.textContent = 'Default (unresolved)';
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

function wireNotebookPopover(button, popup) {
  popup.addEventListener('keydown', event => {
    if (event.key !== 'Escape') return;
    // Search inputs otherwise consume Escape to clear text before dismissing.
    event.preventDefault(); event.stopPropagation();
    popup.hidePopover(); button.focus();
  });
  button.popoverTargetElement = popup;
  button.setAttribute('aria-expanded', 'false');
  popup.addEventListener('beforetoggle', event => {
    if (event.newState !== 'open') return;
    const rect = button.getBoundingClientRect(), width = Math.min(368, innerWidth - 24);
    popup.style.width = width + 'px';
    popup.style.left = Math.max(12, Math.min(rect.left, innerWidth - width - 12)) + 'px';
    const above = rect.top > innerHeight / 2;
    popup.style.top = above ? 'auto' : (rect.bottom + 8) + 'px';
    popup.style.bottom = above ? (innerHeight - rect.top + 8) + 'px' : 'auto';
    popup.style.maxHeight = Math.max(80, Math.min(440, above ? rect.top - 20 : innerHeight - rect.bottom - 20)) + 'px';
  });
  popup.addEventListener('toggle', event => {
    button.setAttribute('aria-expanded', String(event.newState === 'open'));
    if (event.newState === 'open') popup.querySelector('input[type="search"]')?.focus();
  });
}

function notebookPopover(button, label) {
  const popup = document.createElement('div');
  popup.className = 'composer-popover notebook-popover';
  popup.popover = 'auto'; popup.setAttribute('role', 'region'); popup.setAttribute('aria-label', label);
  button.after(popup);
  wireNotebookPopover(button, popup);
  return popup;
}

function initCellToolbar(cell) {
  cell.tabIndex = -1;
  cell.setAttribute('aria-label', 'Notebook cell');
  const head = cell.querySelector('.cell-head');
  const type = document.createElement('span'); type.className = 'cell-type';
  type.textContent = cell.dataset.kind === 'markdown' ? 'Markdown' : 'SQL';
  head.prepend(type);
  const actions = cell.querySelector('.cell-actions');
  const more = cellButton('⋯', 'btn ghost', ''); more.setAttribute('aria-label', 'Cell actions');
  actions.append(more);
  const popup = notebookPopover(more, 'Cell actions');
  popup.append(cell.querySelector('.cell-id'));
  const remove = actions.querySelector('[data-action=delete]');
  remove.textContent = 'Delete cell'; remove.setAttribute('aria-label', 'Delete cell');
  remove.addEventListener('click', () => { popup.hidePopover(); more.focus(); });
  popup.append(remove);
  const insert = cell.querySelector('.insert-bar');
  // Position insertion between cards while keeping its owning cell explicit.
  cell.append(insert);
  for (const [action, label] of [['up', 'Move cell up'], ['down', 'Move cell down']]) {
    actions.querySelector('[data-action="' + action + '"]').setAttribute('aria-label', label);
  }
  cell.metadata = {...nb?.cells.find(entry => entry.id === cell.dataset.id)?.metadata};
  cell.legacyEngine = nb?.cells.find(entry => entry.id === cell.dataset.id)?.engine || null;
  const select = document.createElement('input'); select.type = 'checkbox'; select.className = 'cell-select';
  select.setAttribute('aria-label', 'Select cell');
  select.addEventListener('click', event => {
    cellNavigation = true;
    if (event.shiftKey) selectCell(cell, true);
    else {cell.classList.toggle('selected', select.checked); currentCell = cell; selectionAnchor = cell; syncCellSelection();}
  });
  cell.querySelector('.cell-gutter').prepend(select);
}

function updateDataLabel(root) {
  const button = root.querySelector('.cell-data');
  if (!button) return;
  const catalog = root.querySelector('.catalog-context');
  const schema = root.querySelector('.query-schema');
  button.textContent = legacyContextMixed ? 'Data: mixed legacy contexts — choose notebook context' : 'Data: ' + (catalog.value || 'No catalog selected') + ' / ' + (schema.value || 'No schema selected');
  button.title = button.textContent;
}

function searchableContext(select, name) {
  const search = document.createElement('input');
  search.type = 'search'; search.placeholder = 'Search ' + name.toLowerCase();
  search.setAttribute('aria-label', 'Search ' + name.toLowerCase());
  select.before(search);
  select.size = 5;
  const filter = () => {
    const query = search.value.trim().toLowerCase();
    for (const option of select.options) option.hidden = Boolean(query && !option.text.toLowerCase().includes(query));
  };
  search.addEventListener('input', filter);
  new MutationObserver(filter).observe(select, {childList: true});
}

function contextSelect(root, name, className) {
  const label = document.createElement('label');
  label.className = 'select-wrap';
  const title = document.createElement('span');
  title.textContent = name;
  const select = document.createElement('select');
  select.className = className;
  select.setAttribute('aria-label', name);
  label.append(title, select);
  const context = root;
  (context.querySelector('[popover]') || context).append(label);
  return select;
}

async function initNotebookContext() {
  if (!notebookData || !nb) return;
  const root = notebookData;
  const legacy = new Map(nb.cells.filter(entry => cellKind(entry.sql) === 'sql').map(entry => {
    const pair = {catalog_context: entry.metadata?.catalog_context || null, schema: entry.metadata?.schema || null};
    return [JSON.stringify(pair), pair];
  }));
  legacyContextMixed = !nb.context && legacy.size > 1;
  const saved = nb.context || (legacy.size === 1 ? [...legacy.values()][0] : {});
  const context = notebookData;
  const data = cellButton('Data: loading…', 'btn ghost cell-data', '');
  context.append(data);
  const popup = notebookPopover(data, 'Data context');
  const feedback = document.createElement('p'); feedback.className = 'status data-status'; feedback.setAttribute('role', 'status');
  feedback.textContent = 'Loading catalogs…';
  popup.append(feedback);
  const catalog = contextSelect(root, 'Catalog', 'catalog-context');
  const schema = contextSelect(root, 'Schema', 'query-schema');
  searchableContext(catalog, 'Catalogs');
  searchableContext(schema, 'Schemas');
  catalog.add(new Option('Select a catalog', ''));
  schema.add(new Option('Select a schema', ''));
  // Keep saved context serializable while discovery is pending or unavailable.
  if (saved.catalog_context) catalog.add(new Option(saved.catalog_context, saved.catalog_context, false, true));
  if (saved.schema) schema.add(new Option(saved.schema, saved.schema, false, true));
  catalog.disabled = schema.disabled = true;
  schema.addEventListener('change', () => { legacyContextMixed = false; setDirty(true); updateDataLabel(root); });
  catalog.addEventListener('change', () => {
    legacyContextMixed = false;
    setDirty(true);
    loadNotebookContext(root, '', true);
  });
  try {
    catalogsPromise ||= fetch('/api/catalogs', {cache: 'no-store'}).then(async (response) => {
      if (!response.ok) throw new Error(response.status === 403 ? 'Catalog access denied.' : 'Catalog choices unavailable.');
      return response.json();
    });
    const catalogs = await catalogsPromise;
    catalog.replaceChildren(new Option('Select a catalog', ''));
    for (const entry of catalogs) catalog.add(new Option(entry.id, entry.id));
    const current = saved.catalog_context || (!nb.context && !legacyContextMixed && catalogs.length === 1 ? catalogs[0].id : '');
    if (current && !catalogs.some((entry) => entry.id === current)) {
      const missing = new Option(current + ' (unavailable)', current);
      missing.disabled = true;
      catalog.add(missing);
    }
    catalog.value = current;
    catalog.disabled = catalogs.length === 0;
    feedback.textContent = catalogs.length ? '' : 'No catalogs available.';
    if (catalogs.length) await loadNotebookContext(root, saved.schema || '');
  } catch (error) {
    feedback.textContent = error.message;
    status(error.message);
  } finally {
    if (legacyContextMixed) feedback.textContent = 'Cells have different legacy data contexts. Select a notebook catalog before running; existing cell metadata is preserved until then.';
    updateDataLabel(root);
  }
}

async function loadNotebookContext(root, currentSchema, changed = false) {
  const generation = (contextGeneration.get(root) || 0) + 1;
  contextGeneration.set(root, generation);
  const catalog = root.querySelector('.catalog-context');
  const schema = root.querySelector('.query-schema');
  if (!catalog || !schema) return;
  schema.disabled = true;
  schema.replaceChildren(new Option('Select a schema', ''));
  if (currentSchema) schema.add(new Option(currentSchema, currentSchema));
  schema.value = currentSchema;
  const feedback = root.querySelector('.data-status');
  const cleared = changed ? 'Schema cleared after catalog change. ' : '';
  feedback.textContent = cleared + (catalog.value ? 'Loading schemas…' : 'Select a catalog to browse schemas.');
  updateDataLabel(root);
  if (!catalog.value) return;
  try {
    const namespaces = await fetch('/api/catalogs/' + encodeURIComponent(catalog.value) + '/namespaces', {cache: 'no-store'});
    if (!namespaces.ok) throw new Error(namespaces.status === 403 ? 'Schema access denied.' : 'Schemas unavailable.');
    const schemas = await namespaces.json();
    if (contextGeneration.get(root) !== generation) return;
    schema.replaceChildren(new Option('Select a schema', ''));
    for (const entry of schemas) schema.add(new Option(entry.name, entry.name));
    if (currentSchema && !schemas.some((entry) => entry.name === currentSchema)) {
      const missing = new Option(currentSchema + ' (unavailable)', currentSchema);
      missing.disabled = true;
      schema.add(missing);
    }
    schema.value = currentSchema;
    schema.disabled = schemas.length === 0;
    feedback.textContent = cleared + (schemas.length ? '' : 'No schemas available.');
    updateDataLabel(root);
  } catch (error) {
    if (contextGeneration.get(root) === generation) { feedback.textContent = cleared + error.message; status(error.message); }
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
  const type = cell.querySelector('.cell-type');
  if (type) type.textContent = kind === 'markdown' ? 'Markdown' : 'SQL';
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

function notebookContextFromDom() {
  if (legacyContextMixed) return null;
  return {catalog_context: notebookData.querySelector('.catalog-context').value || null,
    schema: notebookData.querySelector('.query-schema').value || null};
}

function cellsFromDom() {
  const selectedEngine = notebookEngineId();
  return [...document.querySelectorAll('.cell')].map((cell) => {
    const metadata = {...cell.metadata};
    if (!legacyContextMixed) { delete metadata.catalog_context; delete metadata.schema; }
    return {
      id: cell.dataset.id,
      sql: cell.querySelector('.editor').value,
      engine: selectedEngine ? null : cell.legacyEngine,
      metadata,
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
    cellRedo.length = 0;
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
  initCellToolbar(section);
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
  if (!target || !target.dataset.action) return;
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
  if (cell) {
    if (event.target.matches('.editor')) {cellNavigation = false; selectCell(cell);}
    else {
      if (event.target === cell || event.target.matches('.cell-select')) cellNavigation = true;
      currentCell = cell; syncCellSelection();
    }
  }
});

function addCell(after, insertBefore = false, sql = '') {
  const before = cellSnapshot();
  const id = 'c' + crypto.randomUUID();
  const section = cellTemplate(id, sql);
  if (after) {if (insertBefore) after.before(section); else after.after(section);}
  else cellsRoot.appendChild(section);
  currentCell = section;
  section.querySelector('.editor').focus();
  selectCell(section);
  rememberCellOperation(before);
  return section;
}

function removeCell(cell) {
  const cells = [...document.querySelectorAll('.cell')];
  if (cells.length === 1) {
    status('a notebook keeps at least one cell');
    return;
  }
  if (cell.classList.contains('running')) return status('Wait for the query to finish.');
  const before = cellSnapshot();
  cell.remove();
  selectCell(cells.find(entry => entry !== cell));
  rememberCellOperation(before);
}

function moveCell(cell, delta) {
  const sibling = delta < 0 ? cell.previousElementSibling : cell.nextElementSibling;
  if (!sibling) return;
  const before = cellSnapshot();
  if (delta < 0) sibling.before(cell);
  else sibling.after(cell);
  rememberCellOperation(before);
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
  outBox(out).assistantText = text;
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
  outBox(out).assistantText = message;
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
  outBox(out).assistantText += '\n' + table.innerText + (result.truncated ? '\nResult truncated.' : '');
  outputControls(out, result);

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
  if (cell.classList.contains('running')) return;
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
  if (legacyContextMixed) {
    renderError(cell.querySelector('.out'), 'Select a notebook data context before running.');
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
  const engine = notebookEngineId() ?? cell.legacyEngine;
  const catalog = notebookData.querySelector('.catalog-context');
  const schema = notebookData.querySelector('.query-schema');
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

/* All queues share admission; failed cells retain the existing continue policy. */
async function runAll() {
  return runSelectedCells(notebookCells());
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
      body: JSON.stringify({ ...nb, engine: notebookEngineId(), context: notebookContextFromDom(), cells: cellsFromDom() }),
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
      try {
        const fresh = await fetch(notebookPath(), {cache: 'no-store', headers: workspaceHeader()});
        const etag = fresh.ok ? fresh.headers.get('etag') : null;
        if (etag) {
          contentRevision = etag.replace(/^"|"$/g, '');
          status('conflict: remote changed; Save overwrites with the refreshed revision');
        } else {
          status('save conflict: reload before saving');
        }
      } catch (_) {
        status('save conflict: reload before saving');
      }
    } else {
      status((silent ? 'autosave' : 'save') + ' failed: ' + (data.error || res.status));
    }
    return;
  }
  contentRevision = data.content_revision;
  autosaveBlocked = false;
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

function updateModelLabels() {
  for (const button of document.querySelectorAll('.model-choice')) {
    button.textContent = 'AI model: ' + (helperSelect?.selectedOptions[0]?.textContent || 'none selected');
  }
}
function initModelChoice(panel) {
  if (!helperSelect || panel.querySelector('.model-choice')) return;
  const button = cellButton('AI model', 'btn ghost model-choice', '');
  panel.querySelector('.chat-heading').append(button);
  const popup = notebookPopover(button, 'AI model for this notebook');
  popup.addEventListener('beforetoggle', event => {
    if (event.newState !== 'open') return;
    const control = helperSelect.closest('label'); control.hidden = false; popup.prepend(control);
  });
  const note = document.createElement('p'); note.className = 'status';
  note.textContent = 'Shared by notebook and cell assistants. Changing AI model sends this history to that model. If no models are available, ask an administrator for access or register one if permitted.';
  const link = document.createElement('a'); link.href = '/settings/llm#ai-models'; link.textContent = 'Manage models';
  popup.append(note, link);
  updateModelLabels();
}


/* The selection belongs to this user and notebook, outside the Git file. */
async function loadHelpers() {
  if (!helperSelect || !nb) return;
  helperSelect.disabled = true;
  helperSelect.replaceChildren(new Option('Loading AI models…', ''));
  try {
    const url = notebookPath() + '/helper';
    const [list, choice] = await Promise.all([fetch('/api/llm'), fetch(url, { headers: workspaceHeader() })]);
    if (!list.ok || !choice.ok) throw new Error('AI model selection unavailable');
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
    status('AI model selection unavailable');
    helperSelect.replaceChildren(new Option('AI model selection unavailable', ''));
    helperSelect.disabled = true;
  }
  updateModelLabels();
  updateChatSend();
}

function fillHelpers(current) {
  helperSelect.replaceChildren();
  const placeholder = new Option(helpers.length ? 'Select an AI model' : 'No models available', '');
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
  updateModelLabels();
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
  if (send) send.disabled = chatBusy || !helperValue() || Boolean(conversation?.pendingTool);
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
      status('AI model saved');
      recordNotebook();
    } catch (error) {
      fillHelpers(savedHelper);
      status('AI model selection failed');
    } finally {
      helperSaving = false;
      helperSelect.disabled = helpers.length === 0;
      updateModelLabels();
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
let completeRequest = 0;

function tokenBefore(text, caret) {
  const match = text.slice(0, caret).match(/[A-Za-z0-9_."-]*$/);
  return match ? match[0] : '';
}

function completeOpen() {
  return !completeBox.classList.contains('hidden');
}

function closeComplete() {
  completeRequest += 1;
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
  if (document.activeElement !== area || !area.isConnected) return;
  const request = ++completeRequest, source = area.value, caret = area.selectionStart;
  const current = () => request === completeRequest && document.activeElement === area && area.isConnected && area.value === source && area.selectionStart === caret;
  const token = tokenBefore(source, caret);
  let items;
  try {
    const res = await fetch(
      '/api/sql/complete?sql=' + encodeURIComponent(source.slice(0, caret)),
    );
    if (!current()) return;
    if (!res.ok) return closeComplete();
    items = await res.json();
  } catch (error) {
    if (current()) closeComplete();
    return;
  }
  if (!current()) return;
  if (!Array.isArray(items) || items.length === 0) return closeComplete();
  /* Replace only the segment after the last dot, so accepting `sales` after
     `polaris.` yields `polaris.sales` rather than dropping the qualifier. A
     token with no dot (a bare or quoted name) is replaced whole. */
  const segment = token.slice(token.lastIndexOf('.') + 1);
  showComplete(area, items, caret - segment.length);
}

function scheduleComplete(area) {
  completeRequest += 1;
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

for (const cell of document.querySelectorAll('.cell')) initCellToolbar(cell);
for (const area of document.querySelectorAll('.editor')) wireEditor(area);
loadEngines();
initNotebookContext().then(() => {
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

function sessionTabEntry(tab, session, cell = null) {
  const entry = document.createElement('div'); entry.className = 'chat-session-entry'; entry.setAttribute('role', 'presentation');
  const remove = document.createElement('button'); remove.type = 'button'; remove.className = 'session-delete';
  remove.textContent = '×'; remove.title = 'Delete session'; remove.setAttribute('aria-label', 'Delete session: ' + session.phrase);
  remove.dataset.sessionId = session.id;
  const state = cell ? cellChatState(cell.dataset.id) : null;
  const current = cell ? state.conversation : conversation;
  remove.disabled = (cell ? state.busy : chatBusy) || Boolean(current?.id === session.id && current.pendingTool);
  remove.addEventListener('click', () => deleteChatSession(session, cell));
  entry.append(tab, remove);
  return entry;
}

async function deleteChatSession(session, cell = null) {
  const state = cell ? cellChatState(cell.dataset.id) : null;
  if (cell ? state.busy : chatBusy) return;
  if (!window.confirm(`Delete session “${session.phrase}” and all its messages? This cannot be undone. Any draft in this session will be discarded. The notebook and its cells are kept.`)) return;
  const panel = cell ? cellChatParts(cell).panel : chatPanel;
  const status = cell ? cellChatParts(cell).status : document.getElementById('chat-status');
  if (cell) { state.busy = true; state.read += 1; }
  else { chatBusy = true; conversationRead += 1; updateChatSend(); }
  const prompt = panel.querySelector('#chat-prompt, .cell-chat-prompt');
  const context = panel.querySelector('#chat-context, .cell-chat-context');
  const send = panel.querySelector('[type=submit]');
  prompt.disabled = context.disabled = send.disabled = true;
  setAiControlsBusy(panel, true);
  let deleted = false;
  try {
    await chatRpc('DeleteConversation', {notebook:nb.id, session:session.id, expectedRevision:String(session.revision || '0'), ...(cell ? {cell:cell.dataset.id} : {})});
    deleted = true;
    if (cell) state.sessions = state.sessions.filter(item => item.id !== session.id);
    else chatSessions = chatSessions.filter(item => item.id !== session.id);
    const active = cell ? state.active : activeChatSession;
    if (active === session.id) {
      if (cell) { state.active = null; state.conversation = null; }
      else { activeChatSession = null; conversation = null; }
      prompt.value = ''; context.value = ''; panel.composer?.syncContext();
      if (cell) renderCellChat(cell); else renderConversation();
    }
    if (cell) renderCellSessions(cell); else renderChatSessions();
    const sessions = cell ? await loadCellChatSessions(cell) : await loadChatSessions();
    if (active === session.id) {
      if (sessions.length) {
        if (cell) await selectCellSession(cell, sessions[0].id);
        else await selectChatSession(sessions[0].id);
      } else if (cell) renderCellChat(cell);
      else renderConversation();
    }
    status.textContent = 'Session deleted.';
  } catch (error) {
    status.textContent = deleted ? 'Session deleted. Could not refresh remaining sessions; reopen the assistant to reload them.' : error.message + ' Review the refreshed session list before trying again.';
    if (!deleted) {
      try { if (cell) await loadCellChatSessions(cell); else await loadChatSessions(); } catch (_) { /* Keep the deletion error visible. */ }
    }
  } finally {
    if (cell) { state.busy = false; renderCellSessions(cell); cellChatParts(cell).send.disabled = Boolean(state.conversation?.pendingTool); }
    else { chatBusy = false; renderChatSessions(); updateChatSend(); }
    setAiControlsBusy(panel, Boolean((cell ? state.conversation : conversation)?.pendingTool));
    prompt.disabled = context.disabled = false;
  }
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
    strip.appendChild(sessionTabEntry(tab, session));
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
  syncSkillPicker(chatPanel, conversation);
  syncNotebookMcp(chatPanel, conversation);
  const history = document.getElementById('chat-history');
  history.replaceChildren();
  if (!(conversation?.messages || []).length) {
    const empty = document.createElement('p');
    empty.className = 'chat-empty';
    empty.textContent = 'No messages yet. Ask a question, or attach a cell below.';
    history.appendChild(empty);
    scrollConversation(history);
    return;
  }
  for (const message of conversation.messages || []) {
    const entry = document.createElement('article');
    entry.className = 'chat-message ' + (message.role === 'assistant' ? 'assistant' : 'user');
    const label = document.createElement('strong');
    label.textContent = message.role === 'assistant' ? message.helper || 'Assistant' : 'You';
    if (message.skills?.length) label.textContent += ' · ' + message.skills.length + (message.skills.length === 1 ? ' skill' : ' skills');
    const text = document.createElement('div');
    text.className = 'chat-text';
    text.textContent = message.content;
    entry.append(label, text);
    if (message.role === 'assistant') {
      text.remove();
      appendSqlSuggestions(entry, message.content, currentCell);
    }
    history.appendChild(entry);
  }
  scrollConversation(history);
}

function appendSqlSuggestions(entry, content, preferredCell) {
  let offset = 0, index = 0;
  const prose = value => {
    if (!value) return;
    const text = document.createElement('div');
    text.className = 'chat-text'; text.textContent = value; entry.append(text);
  };
  for (const match of content.matchAll(/```sql[ \t]*\r?\n([\s\S]*?)```/gi)) {
    prose(content.slice(offset, match.index)); offset = match.index + match[0].length;
    const sql = match[1].trim();
    if (!sql) continue;
    const group = document.createElement('section'); group.className = 'suggestion-actions';
    const label = document.createElement('strong'); label.textContent = 'Query ' + (++index);
    const preview = document.createElement('pre'); preview.textContent = sql;
    const destination = document.createElement('select');
    destination.setAttribute('aria-label', 'Destination for query ' + index);
    destination.add(new Option('New SQL cell', 'new'));
    const targets = new Map();
    const targetLabel = cell => `Cell ${notebookCells().indexOf(cell) + 1} · ${cell.dataset.kind === 'markdown' ? 'Markdown' : 'SQL'}`;
    notebookCells().forEach(cell => {
      targets.set(cell.dataset.id, cell);
      destination.add(new Option(targetLabel(cell), cell.dataset.id));
    });
    if (preferredCell?.isConnected) destination.value = preferredCell.dataset.id;
    const send = document.createElement('button'); send.type = 'button'; send.className = 'btn';
    const message = document.createElement('p'); message.setAttribute('role', 'status');
    const refresh = () => {
      const cell = targets.get(destination.value);
      const creates = !cell || cell.dataset.kind === 'markdown';
      send.textContent = creates ? 'Create SQL cell' : 'Replace cell SQL';
      message.textContent = cell?.dataset.kind === 'markdown'
        ? 'Keeps Markdown; creates a SQL cell after it. Does not run.'
        : creates ? 'Creates a SQL cell at the end. Does not run.' : 'Replaces the selected cell SQL. Undo is available. Does not run.';
    };
    destination.addEventListener('change', refresh); refresh();
    send.addEventListener('click', () => {
      const cell = targets.get(destination.value);
      if (destination.value !== 'new' && !cell?.isConnected) {
        message.textContent = 'Destination was removed. Choose New SQL cell or another destination.';
        return;
      }
      if (cell && destination.selectedOptions[0].textContent !== targetLabel(cell)) {
        destination.selectedOptions[0].textContent = targetLabel(cell);
        refresh();
        message.textContent = 'Destination changed. Review the updated destination and action, then send again.';
        return;
      }
      if (cell?.classList.contains('running')) {
        message.textContent = 'Wait for the destination query to finish.'; return;
      }
      insertIntoSqlCell(sql, cell || null);
      message.textContent = 'Query sent to cell. Undo is available. Click Run when ready.';
    });
    group.append(label, preview, destination, send, message); entry.append(group);
  }
  prose(content.slice(offset));
}

function insertIntoSqlCell(sql, target) {
  let cell = target;
  if (!cell || cell.dataset.kind === 'markdown') {
    cell = addCell(cell, false, sql);
  } else {
    const before = cellSnapshot();
    cell.querySelector('.editor').value = sql;
    rememberCellOperation(before);
  }
  const editor = cell.querySelector('.editor');
  syncContractDraft(editor);
  resize(editor);
  cell.scrollIntoView({block: 'nearest'});
  editor.focus();
}

async function loadChatSessions() {
  const current = conversationRead;
  const value = await chatRpc('ListConversations', { notebook: nb.id });
  if (current !== conversationRead) return chatSessions;
  chatSessions = value.sessions || [];
  renderChatSessions();
  return chatSessions;
}

async function newChatSession() {
  if (chatBusy) return;
  const current = ++conversationRead;
  const created = await chatRpc('NewConversation', { notebook: nb.id });
  if (current !== conversationRead) return;
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
    chatPanel.composer?.syncContext();
    closeComposerPopovers(chatPanel);
    status.textContent = 'Attached to the next message.';
  } catch (error) {
    status.textContent = error.message;
  }
}

async function openChat() {
  initModelChoice(chatPanel);
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
      if (!chatSessions.length) {
        activeChatSession = null; conversation = null;
        renderChatSessions(); renderConversation();
        document.getElementById('chat-status').textContent = '';
        return;
      }
      if (!chatSessions.some((session) => session.id === activeChatSession)) activeChatSession = chatSessions[0].id;
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
  closeComposerPopovers(chatPanel);
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
    if (event.key === 'Escape' && !chatPanel.querySelector(':popover-open')) { event.preventDefault(); closeChat(); }
  });
  document.getElementById('chat-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    if (chatBusy) return;
    const prompt = document.getElementById('chat-prompt');
    const context = document.getElementById('chat-context');
    const status = document.getElementById('chat-status');
    const chosen = helperValue();
    if (!chosen) { status.textContent = 'Register and Select an AI model first.'; return; }
    chatBusy = true;
    setAiControlsBusy(chatPanel,true);
    document.getElementById('chat-send').disabled = true;
    prompt.disabled = context.disabled = true;
    status.textContent = 'Waiting for the helper…';
    try {
      const selectedSkills = skillForTurn(chatPanel), selectedMcp = mcpForTurn(chatPanel);
      if (!conversation && !activeChatSession) {
        conversation = await chatRpc('NewConversation', {notebook:nb.id});
        activeChatSession = conversation.id;
      } else if (!conversation) await loadConversation();
      if (!conversation) throw new Error('Reload authorized history before sending.');
      const current = conversationRead;
      const sent = await chatRpc('SendMessage', {
        ...contractForTurn(),
        ...selectedSkills,
        ...selectedMcp,
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
      chatPanel.composer?.syncContext();
      status.textContent = conversation?.pendingTool ? 'Resolve the tool turn before sending another message.' : 'Saved.';
    } catch (error) {
      status.textContent = error.message + ' Your draft is unchanged; reload history before resending.';
      if (activeChatSession) {
        try { await loadConversation(); } catch (_) { conversation = null; }
      }
    } finally {
      chatBusy = false;
      setAiControlsBusy(chatPanel,Boolean(conversation?.pendingTool));
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
  const output = (out ? (out.assistantText ?? out.innerText) : '').trim().slice(0, 4000);
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
    parts.sessions.appendChild(sessionTabEntry(tab, session, cell));
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
  cellChatParts(cell).send.disabled = state.busy || Boolean(state.conversation?.pendingTool);
  syncSkillPicker(cellChatParts(cell).panel, state.conversation);
  syncNotebookMcp(cellChatParts(cell).panel, state.conversation);
  history.replaceChildren();
  if (!(state.conversation?.messages || []).length) {
    const empty = document.createElement('p');
    empty.className = 'chat-empty';
    empty.textContent = 'No messages yet. Ask about this cell, or attach the notebook summary.';
    history.appendChild(empty);
    scrollConversation(history);
    return;
  }
  for (const message of state.conversation?.messages || []) {
    const entry = document.createElement('article');
    entry.className = 'chat-message ' + (message.role === 'assistant' ? 'assistant' : 'user');
    const label = document.createElement('strong');
    label.textContent = message.role === 'assistant' ? message.helper || 'Assistant' : 'You';
    if (message.skills?.length) label.textContent += ' · ' + message.skills.length + (message.skills.length === 1 ? ' skill' : ' skills');
    const text = document.createElement('div');
    text.className = 'chat-text';
    text.textContent = message.content;
    entry.append(label, text);
    if (message.role === 'assistant') {
      text.remove();
      appendSqlSuggestions(entry, message.content, cell);
    }
    history.appendChild(entry);
  }
  scrollConversation(history);
}

function invalidateCellChat(cell) {
  const state = cellChatState(cell.dataset.id);
  state.read += 1;
  state.conversation = null;
  renderCellChat(cell);
}

async function loadCellChatSessions(cell) {
  const state = cellChatState(cell.dataset.id);
  const current = state.read;
  const value = await chatRpc('ListConversations', { notebook: nb.id, cell: cell.dataset.id });
  if (current !== state.read) return state.sessions;
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
  const current = ++state.read;
  const created = await chatRpc('NewConversation', { notebook: nb.id, cell: cell.dataset.id });
  if (current !== state.read) return;
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
  initModelChoice(parts.panel);
  parts.prompt.focus();
  try {
    await loadCellChatSessions(cell);
    if (!state.sessions.length) {
      state.active = null; state.conversation = null;
      renderCellSessions(cell); renderCellChat(cell);
      parts.status.textContent = '';
      return;
    }
    if (!state.sessions.some((session) => session.id === state.active)) state.active = state.sessions[0].id;
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
  closeComposerPopovers(cellChatParts(cell).panel);
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
    parts.panel.composer?.syncContext();
    closeComposerPopovers(parts.panel);
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
  if (!chosen) { parts.status.textContent = 'Register and Select an AI model first.'; return; }
  state.busy = true;
  setAiControlsBusy(parts.panel,true);
  parts.send.disabled = true;
  parts.prompt.disabled = true;
  parts.status.textContent = 'Waiting for the helper…';
  try {
    const selectedSkills = skillForTurn(parts.panel), selectedMcp = mcpForTurn(parts.panel);
    if (!state.conversation && !state.active) {
      state.conversation = await chatRpc('NewConversation', {notebook:nb.id, cell:cell.dataset.id});
      state.active = state.conversation.id;
    } else if (!state.conversation) await loadCellChat(cell);
    if (!state.conversation) throw new Error('Reload authorized history before sending.');
    const current = state.read;
    const sent = await chatRpc('SendMessage', {
      ...contractForTurn(),
      ...selectedSkills,
      ...selectedMcp,
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
    parts.panel.composer?.syncContext();
    parts.status.textContent = state.conversation?.pendingTool ? 'Resolve the tool turn before sending another message.' : 'Saved.';
  } catch (error) {
    invalidateCellChat(cell);
    parts.status.textContent = error.message + ' Your draft is unchanged; reopen to reload history.';
  } finally {
    state.busy = false;
    setAiControlsBusy(parts.panel,Boolean(state.conversation?.pendingTool));
    parts.send.disabled = Boolean(state.conversation?.pendingTool);
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

  function setExpanded(node, expanded) {
    node.querySelector('.tree-children').hidden = !expanded;
    const toggle = node.querySelector('.tree-toggle');
    toggle.textContent = expanded ? '▾' : '▸';
    toggle.setAttribute('aria-expanded', String(expanded));
    toggle.setAttribute('aria-label', (expanded ? 'Collapse ' : 'Expand ') + node.dataset.name);
  }

  function toggleBranch(node, expand) {
    if (!node.querySelector('.tree-children').hidden) setExpanded(node, false);
    else expand();
  }

  async function expandCatalog(node, catalog) {
    const children = node.querySelector('.tree-children');
    if (!children.hidden) return;
    setExpanded(node, true);
    if (children.dataset.loaded === '1' || children.dataset.loading === '1') return;
    children.dataset.loading = '1';
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
        const toggle = treeButton('▸', 'tree-toggle', () => toggleBranch(child, () => expandSchema(child, catalog, namespace.token)));
        const label = treeButton(namespace.name, 'tree-label', () => selectEntity(catalog, namespace.token, null));
        head.append(toggle, label);
        const grandchildren = document.createElement('div');
        grandchildren.className = 'tree-children';
        grandchildren.hidden = true;
        child.append(head, grandchildren);
        setExpanded(child, false);
        children.appendChild(child);
      }
      children.dataset.loaded = '1';
      navStatus.textContent = '';
    } catch (error) {
      setExpanded(node, false);
      navStatus.textContent = error.message;
    } finally {
      delete children.dataset.loading;
    }
  }

  async function expandSchema(node, catalog, namespace) {
    const children = node.querySelector('.tree-children');
    if (!children.hidden) return;
    setExpanded(node, true);
    if (children.dataset.loaded === '1' || children.dataset.loading === '1') return;
    children.dataset.loading = '1';
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
      setExpanded(node, false);
      navStatus.textContent = error.message;
    } finally {
      delete children.dataset.loading;
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
      const toggle = treeButton('▸', 'tree-toggle', () => toggleBranch(node, () => expandCatalog(node, catalog.id)));
      const label = treeButton(catalog.id, 'tree-label', () => selectEntity(catalog.id, null, null));
      const chip = document.createElement('span');
      chip.className = 'chip';
      chip.textContent = catalog.kind;
      head.append(toggle, label, chip);
      const children = document.createElement('div');
      children.className = 'tree-children';
      children.hidden = true;
      node.append(head, children);
      setExpanded(node, false);
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
      if (sequence !== detailSeq) return;
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
    const sequence = detailSeq;
    const entity = {...currentDetail.entity};
    const status = pane.querySelector('.docs-editor .status');
    status.textContent = 'Saving…';
    const body = {...entity, markdown, expectedRevision: expectedRevision || null};
    try {
      const response = await fetch('/api/catalog/docs', {
        method: 'PUT',
        headers: {'content-type': 'application/json'},
        body: JSON.stringify(body),
      });
      const value = await response.json().catch(() => ({}));
      if (sequence !== detailSeq) return;
      if (!response.ok) {
        status.textContent = response.status === 409
          ? 'Document changed; reload before saving.'
          : (value.error || 'save failed');
        return;
      }
      await selectEntity(entity.catalog, entity.namespace, entity.table, 'docs');
      if (detailSeq === sequence + 1) navStatus.textContent = 'Docs saved.';
    } catch (error) {
      if (sequence === detailSeq) status.textContent = error.message;
    }
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
    const sequence = ++detailSeq;
    const params = new URLSearchParams({catalog: currentDetail.entity.catalog, revision: oid});
    if (currentDetail.entity.namespace) params.set('namespace', currentDetail.entity.namespace);
    if (currentDetail.entity.table) params.set('table', currentDetail.entity.table);
    try {
      const value = await catalogJson('/api/catalog/docs?' + params.toString());
      if (sequence !== detailSeq || !value.document) return;
      currentTab = 'docs';
      currentDetail.docs = {
        ...currentDetail.docs,
        markdown: value.document.markdown,
        viewRevision: oid,
      };
      renderDetail();
    } catch (error) {
      if (sequence !== detailSeq) return;
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

/* ---- AI registry ----------------------------------------------------- */
async function aiRpc(method, body = {}) {
  const response = await fetch('/aster.v1.Aster/' + method, {
    method: 'POST', cache: 'no-store',
    headers: {'content-type': 'application/json', 'connect-protocol-version': '1'},
    body: JSON.stringify(body),
  });
  const value = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(value.message || 'AI registry request failed');
  return value;
}

const aiRegistry = document.getElementById('ai-registry');
if (aiRegistry) {
  let policy = {canRegister: false, isAdmin: false, revision: '0'};
  let skill = null;
  let latestRevision = '0';
  let read = 0;
  let sharedRevision = null;
  const tabs = new Map();
  function element(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  }
  function button(text, run, className = 'btn') {
    const node = element('button', text, className);
    node.type = 'button';
    if (run) node.addEventListener('click', run);
    return node;
  }
  function field(parent, name, id, type = 'text') {
    const label = element('label', name);
    const input = document.createElement(type === 'textarea' ? 'textarea' : 'input');
    if (type !== 'textarea') input.type = type;
    input.id = id;
    input.name = id;
    if (type === 'textarea') input.rows = 5;
    label.append(input);
    parent.append(label);
    return input;
  }
  function status(parent, id) {
    const node = element('p', '', 'status');
    node.id = id;
    node.setAttribute('role', 'status');
    parent.append(node);
    return node;
  }
  function lines(value) {
    return [...new Set(value.split('\n').map(v => v.trim()).filter(Boolean))];
  }
  async function json(url, options = {}) {
    const response = await fetch(url, {cache: 'no-store', ...options});
    const value = await response.json().catch(() => ({}));
    if (!response.ok) throw new Error(value.error || value.message || 'Model request failed');
    return value;
  }
  function mutation(body) {
    return {method: 'PUT', headers: {'content-type': 'application/json'}, body: JSON.stringify(body)};
  }
  aiRegistry.replaceChildren(element('h1', 'AI workspace'));
  aiRegistry.append(element('p', 'Skills provide instructions and references. Model access is managed separately.', 'status'));
  const tablist = element('div', undefined, 'ai-tabs');
  tablist.setAttribute('role', 'tablist');
  tablist.setAttribute('aria-label', 'AI workspace');
  aiRegistry.append(tablist);
  function tab(name, id) {
    const pane = element('section', undefined, 'panel ai-pane');
    pane.id = id;
    pane.setAttribute('role', 'tabpanel');
    const trigger = button(name, () => selectTab(id));
    trigger.id = id + '-tab';
    trigger.setAttribute('role', 'tab');
    trigger.setAttribute('aria-controls', id);
    pane.setAttribute('aria-labelledby', trigger.id);
    tablist.append(trigger);
    aiRegistry.append(pane);
    tabs.set(id, {trigger, pane});
    return pane;
  }
  function selectTab(id) {
    for (const [key, value] of tabs) {
      value.pane.hidden = key !== id;
      value.trigger.setAttribute('aria-selected', String(key === id));
      value.trigger.tabIndex = key === id ? 0 : -1;
    }
  }
  tablist.addEventListener('keydown', event => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    const visible = [...tabs.values()].filter(value => !value.trigger.hidden);
    let index = visible.findIndex(value => value.trigger === document.activeElement);
    if (index < 0) return;
    event.preventDefault();
    index = event.key === 'Home' ? 0 : event.key === 'End' ? visible.length - 1 :
      (index + (event.key === 'ArrowRight' ? 1 : -1) + visible.length) % visible.length;
    visible[index].trigger.click();
    visible[index].trigger.focus();
  });

  const defaultsPane = tab('My defaults', 'ai-defaults');
  defaultsPane.append(element('h2', 'My skill defaults'), element('p', 'Choose skills and exact versions for new notebook and cell conversations. Existing conversations keep their selections. These are personal preferences, not sharing permissions.', 'status'));
  const defaultsControls = createSkillControls(defaultsPane);
  defaultsPane.append(defaultsControls.box);
  const defaultsStatus = status(defaultsPane, 'skill-defaults-status');
  let defaultsRevision = null;
  const saveDefaults = button('Save skill defaults', async () => {
    if (defaultsRevision === null) return;
    saveDefaults.disabled = true; defaultsControls.box.disabled = true;
    try {
      const saved = await aiRpc('PutSkillDefaults', {expectedRevision:defaultsRevision,skills:defaultsControls.pins});
      defaultsRevision = String(saved.revision); defaultsControls.set(saved.skills || []);
      defaultsStatus.textContent = 'Skill defaults saved. Existing conversations are unchanged.';
    } catch (error) { defaultsStatus.textContent = error.message; }
    finally { saveDefaults.disabled = false; defaultsControls.box.disabled = false; }
  });
  saveDefaults.disabled = true;
  async function loadDefaults() {
    defaultsControls.box.disabled = true; saveDefaults.disabled = true;
    try {
      const result = await aiRpc('GetSkillDefaults');
      defaultsRevision = String(result.revision || '0'); defaultsControls.set(result.skills || []);
      await defaultsControls.load(); saveDefaults.disabled = !defaultsControls.ready;
      defaultsStatus.textContent = '';
    } catch (error) { defaultsStatus.textContent = error.message; }
    finally { defaultsControls.box.disabled = false; }
  }
  defaultsPane.append(saveDefaults, button('Reload defaults', loadDefaults));
  loadDefaults();

  const skillsPane = tab('Skills', 'ai-skills');
  let listedSkills = [];
  let dirty = false;
  let savedDraft = '';
  const skillActions = element('div', undefined, 'ai-actions');
  const search = field(skillActions, 'Search skills', 'skill-search', 'search');
  search.placeholder = 'Search by name, description or owner';
  const filterLabel = element('label', 'Filter sharing scope');
  const scopeFilter = document.createElement('select');
  scopeFilter.setAttribute('aria-label', 'Filter sharing scope');
  for (const [label, value] of [['All scopes', ''], ['Private', 'private'], ['Team', 'team'], ['Organization', 'organization']]) scopeFilter.add(new Option(label, value));
  filterLabel.append(scopeFilter);
  skillActions.append(filterLabel);
  const skillList = element('ul', undefined, 'skill-library');
  skillList.id = 'skill-list';
  const skillListStatus = status(skillsPane, 'skill-list-status');
  const editor = element('form', undefined, 'ai-form skill-editor');
  editor.id = 'skill-editor';
  editor.noValidate = true;
  editor.hidden = true;
  function discardDraft() { return !dirty || window.confirm('Discard unsaved skill changes?'); }
  skillActions.append(button('New skill', () => { if (discardDraft()) { ++read; editSkill(null); } }), button('Refresh skills', () => loadSkills()));
  const importLabel = element('label', 'Import skill', 'btn skill-import-label');
  const importFile = document.createElement('input');
  importFile.type = 'file'; importFile.id = 'skill-import'; importFile.accept = '.md,.json';
  importFile.setAttribute('aria-label', 'Import skill');
  importLabel.append(importFile); skillActions.append(importLabel);
  skillsPane.append(skillActions, skillList, editor);
  const editorActions = element('div', undefined, 'ai-actions');
  editorActions.append(button('Back to library', () => { if (discardDraft()) { ++read; dirty = false; editor.hidden = true; skillList.hidden = false; } }));
  const shareButton = button('Permissions', () => sharing.showModal());
  const exportButton = button('Export version', () => {
    if (!skill) return;
    const files = [{path: 'SKILL.md', content: skill.instructions}, ...(skill.references || []).map(ref => ({path: ref.name, content: ref.content}))];
    const blob = new Blob([JSON.stringify({name: skill.title, description: skill.description || '', sourceRevision: String(skill.revision), files}, null, 2)], {type: 'application/json'});
    const url = URL.createObjectURL(blob), link = document.createElement('a');
    link.href = url; link.download = 'skill-' + skill.id + '-v' + skill.revision + '.json'; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  editorActions.append(exportButton, shareButton); editor.append(editorActions);
  const title = field(editor, 'Skill name', 'skill-name');
  title.required = true; title.maxLength = 160;
  const description = field(editor, 'Skill description', 'skill-description');
  description.maxLength = 512; description.placeholder = 'When should this skill be used?';
  const versionLabel = element('label', 'Skill version');
  const versions = document.createElement('select');
  versions.id = 'skill-version'; versions.setAttribute('aria-label', 'Skill version');
  versionLabel.append(versions); editorActions.append(versionLabel);
  const versionsPanel = element('section', undefined, 'skill-versions-panel');
  const versionList = element('ul', undefined, 'skill-version-list');
  versionsPanel.append(element('h3', 'Versions'), element('p', 'Open an earlier version to review or export it. Edit the current version to save a new one. Conversations keep their selected version.', 'status'), versionList);
  function renderVersions() {
    versionsPanel.hidden = !skill;
    versionList.replaceChildren();
    for (const option of versions.options) {
      const revision = option.value;
      const row = element('li');
      row.append(element('span', 'Version ' + revision + (revision === latestRevision ? ' · Current' : ' · Read-only')),
        button('Open version ' + revision, () => { if (discardDraft()) openSkill(skill.id, revision); }));
      versionList.append(row);
    }
  }
  const owner = element('p', '', 'status'); editor.append(owner);
  const filesLayout = element('div', undefined, 'skill-files');
  const fileTree = element('nav', undefined, 'skill-file-tree'); fileTree.setAttribute('aria-label', 'Skill files');
  const fileToolbar = element('div', undefined, 'skill-file-toolbar');
  const fileEntries = element('div', undefined, 'skill-file-entries');
  fileTree.append(fileToolbar, fileEntries);
  const folderState = new Map();
  const fileContent = element('div', undefined, 'skill-file-content');
  filesLayout.append(fileTree, fileContent);
  editor.append(element('h3', 'Instructions and reference files'), element('p', 'SKILL.md contains the instructions. Add supporting text files such as checklists, examples or schemas. These files are part of this skill package, not paths on your computer. They cannot run code.', 'status'), filesLayout);
  const instructions = field(fileContent, 'Instructions', 'skill-instructions', 'textarea');
  instructions.required = true; instructions.rows = 18; instructions.maxLength = 32768; instructions.spellcheck = false;
  const refs = element('div', undefined, 'skill-references'); fileContent.append(refs);
  let activeFile = null;
  function showFile(row) {
    activeFile = row;
    instructions.parentElement.hidden = !!row;
    for (const child of refs.children) child.hidden = child !== row;
    if (row) {
      const parts = row.querySelector('[data-reference-folder]').value.trim().split('/').filter(Boolean);
      parts.forEach((_, index) => folderState.set(parts.slice(0, index + 1).join('/'), true));
    }
    renderFiles();
  }
  function selectFile(row) {
    showFile(row);
    fileEntries.querySelector('[aria-current="true"]')?.focus({preventScroll: true});
  }
  function restoreFile(path) {
    showFile(path ? [...refs.children].find(row => referencePath(row) === path) || null : null);
  }
  function renderFiles() {
    fileEntries.replaceChildren();
    const root = button('SKILL.md', () => selectFile(null), 'skill-file');
    root.setAttribute('aria-current', activeFile ? 'false' : 'true'); fileEntries.append(root);
    const tree = {folders: new Map(), files: []};
    for (const row of refs.children) {
      let node = tree;
      for (const part of row.querySelector('[data-reference-folder]').value.trim().split('/').filter(Boolean)) {
        if (!node.folders.has(part)) node.folders.set(part, {folders: new Map(), files: []});
        node = node.folders.get(part);
      }
      node.files.push(row);
    }
    function branch(node, parent, prefix = '') {
      const list = element('ul', undefined, 'skill-file-list'); parent.append(list);
      for (const [name, child] of [...node.folders].sort(([a], [b]) => a.localeCompare(b))) {
        const path = prefix ? prefix + '/' + name : name;
        const item = element('li'), folder = element('details', undefined, 'skill-folder');
        folder.open = folderState.get(path) ?? true;
        const summary = element('summary', name); summary.title = path; folder.append(summary);
        folder.addEventListener('toggle', () => { if (folder.isConnected) folderState.set(path, folder.open); });
        branch(child, folder, path); item.append(folder); list.append(item);
      }
      for (const row of [...node.files].sort((a, b) => referencePath(a).localeCompare(referencePath(b)))) {
        const name = row.querySelector('[data-reference-name]').value.trim() || 'New file';
        const path = referencePath(row) || 'New reference file';
        const item = element('li'), entry = button(name, () => selectFile(row), 'skill-file');
        entry.title = path; entry.setAttribute('aria-label', path);
        entry.setAttribute('aria-current', String(row === activeFile)); item.append(entry); list.append(item);
      }
    }
    branch(tree, fileEntries);
  }
  const addReference = button('Add file', () => { const row = reference(); if (row) { dirty = true; showFile(row); row.querySelector('input').focus(); } });
  addReference.setAttribute('aria-label', 'Add reference file');
  fileToolbar.append(element('strong', 'Files'), addReference);
  const saveSkill = button('Save new version'); saveSkill.type = 'submit';
  const footer = element('div', undefined, 'skill-editor-footer');
  footer.append(element('span', 'Text files only · 16 references · 32 KiB total', 'status'), saveSkill);
  editor.append(footer, versionsPanel);
  const deleteDialog = element('dialog', undefined, 'ai-dialog');
  deleteDialog.setAttribute('aria-label', 'Delete skill');
  const deleteDescription = element('p');
  const deleteStatus = status(deleteDialog, 'skill-delete-status');
  let deleting = false, deleteSelection = null;
  const cancelDelete = button('Cancel', () => deleteDialog.close());
  const confirmDelete = button('Delete all versions', async () => {
    if (deleting || !deleteSelection) return;
    const selected = deleteSelection, current = read;
    deleting = true; confirmDelete.disabled = cancelDelete.disabled = true;
    try {
      await aiRpc('DeleteSkill', {id: selected.id, expectedRevision: String(selected.revision), expectedSharingRevision: String(selected.sharingRevision || '0')});
      deleteDialog.close();
      if (current !== read) return;
      ++read; dirty = false; skill = null; editor.hidden = true; skillList.hidden = false;
      await loadSkills();
      skillListStatus.textContent = 'Skill deleted. All versions are unavailable for future use.';
    } catch (error) { if (current === read) deleteStatus.textContent = error.message; }
    finally { deleting = false; confirmDelete.disabled = cancelDelete.disabled = false; }
  }, 'btn danger');
  deleteDialog.addEventListener('cancel', event => { if (deleting) event.preventDefault(); });
  deleteDialog.append(element('h2', 'Delete skill'), deleteDescription, element('p', 'This deletes all versions and reference files for everyone using the skill. Existing messages remain visible. Conversations that used this skill cannot continue; start a new conversation after deleting it. This cannot be undone.'), confirmDelete, cancelDelete);
  skillsPane.append(deleteDialog);
  const deleteSkill = button('Delete skill', () => {
    if (!skill || deleteSkill.disabled) return;
    deleteSelection = {...skill}; deleteStatus.textContent = '';
    deleteDescription.textContent = 'Delete “' + skill.title + '”?'; deleteDialog.showModal();
  }, 'btn danger');
  const dangerZone = element('section', undefined, 'skill-delete-section');
  dangerZone.append(element('h3', 'Delete skill'), element('p', 'Remove this skill and every saved version. Only the owner can delete the current version.', 'status'), deleteSkill);
  editor.append(dangerZone);
  const sharing = element('dialog', undefined, 'ai-dialog'); sharing.setAttribute('aria-label', 'Skill sharing');
  sharing.append(element('h2', 'Share skill'), element('p', 'Sharing controls who can use every version. Editing remains with the owner.', 'status'));
  const audienceLabel = element('label', 'Sharing scope');
  const audience = document.createElement('select'); audience.id = 'skill-audience'; audience.setAttribute('aria-label', 'Sharing scope');
  audience.add(new Option('Private', 'private')); audience.add(new Option('Team', 'team'));
  audienceLabel.append(audience); sharing.append(audienceLabel);
  const teamInput = field(sharing, 'Team ID', 'skill-team'); teamInput.parentElement.hidden = true;
  audience.addEventListener('change', () => { teamInput.parentElement.hidden = audience.value !== 'team'; });
  const sharingStatus = status(sharing, 'skill-sharing-status');
  const saveSharing = button('Update sharing', async () => {
    if (!skill || saveSharing.disabled) return;
    const current = read, selected = skill;
    saveSharing.disabled = true;
    try {
      const updated = await aiRpc('ShareSkill', {id: selected.id, expectedSharingRevision: String(selected.sharingRevision || '0'), audience: audience.value, ...(audience.value === 'team' ? {team: teamInput.value.trim()} : {})});
      if (current !== read) return;
      // Sharing must not replace historical content or unsaved editor changes.
      Object.assign(skill, {sharingRevision: updated.sharingRevision, audience: updated.audience, team: updated.team});
      skillStatus.textContent = 'Sharing updated. Model access is unchanged.';
      sharing.close(); await loadSkills();
    } catch (error) { if (current === read) sharingStatus.textContent = error.message; }
    finally { saveSharing.disabled = !skill?.canEdit || String(skill?.revision) !== latestRevision; }
  });
  sharing.append(saveSharing, button('Close sharing', () => sharing.close())); skillsPane.append(sharing);
  const skillStatus = status(editor, 'skill-status');
  let referenceSequence = 0;
  function referencePath(row) {
    const name = row.querySelector('[data-reference-name]').value.trim();
    const folder = row.querySelector('[data-reference-folder]').value.trim();
    return name ? (folder ? folder + '/' : '') + name : '';
  }
  function reference(value = {}) {
    if (refs.children.length >= 16) { skillStatus.textContent = 'A skill can contain up to 16 references.'; return; }
    const row = element('fieldset');
    const number = ++referenceSequence;
    const path = value.name || '', separator = path.lastIndexOf('/');
    const location = element('div', undefined, 'skill-file-location'); row.append(location);
    const name = field(location, 'File name', 'skill-reference-name-' + number);
    name.value = path.slice(separator + 1); name.dataset.referenceName = 'true'; name.required = true; name.maxLength = 128;
    name.placeholder = 'checklist.md'; name.pattern = '[A-Za-z0-9._-]+';
    const folder = field(location, 'Folder', 'skill-reference-folder-' + number);
    folder.value = value.name === undefined ? 'references' : path.slice(0, Math.max(0, separator));
    folder.dataset.referenceFolder = 'true'; folder.maxLength = 128;
    folder.placeholder = 'Package root';
    folder.title = 'Relative folder within this skill. Leave blank for the package root.';
    const preview = element('p', undefined, 'skill-path-preview');
    preview.setAttribute('aria-live', 'polite'); row.append(preview);
    const updatePath = () => { preview.textContent = referencePath(row) || 'Enter a file name'; renderFiles(); };
    name.addEventListener('input', updatePath); folder.addEventListener('input', updatePath);
    const content = field(row, 'File content', 'skill-reference-content-' + number, 'textarea');
    content.value = value.content || ''; content.rows = 18; content.maxLength = 32768; content.dataset.referenceContent = 'true'; content.spellcheck = false;
    row.append(button('Remove file', () => { row.remove(); dirty = true; showFile(null); })); refs.append(row); updatePath(); return row;
  }
  function renderSkills() {
    const query = search.value.trim().toLocaleLowerCase();
    const matching = listedSkills.filter(item => (!scopeFilter.value || item.audience === scopeFilter.value) && [item.title, item.description, item.owner].some(value => (value || '').toLocaleLowerCase().includes(query)));
    skillList.replaceChildren();
    for (const item of matching) {
      const row = element('li', undefined, 'skill-library-row');
      const summary = element('div');
      summary.append(button(item.title, () => { if (discardDraft()) openSkill(item.id); }, 'skill-title'), element('p', item.description || 'No description', 'status'));
      const meta = element('div', undefined, 'skill-library-meta');
      meta.append(element('span', item.audience + (item.team ? ' / ' + item.team : ''), 'badge'), element('span', 'v' + item.revision), element('span', item.owner === aiRegistry.dataset.subject ? 'You' : item.owner));
      const actions = element('div', undefined, 'ai-actions skill-row-actions');
      const edit = button('Edit', () => { if (discardDraft()) openSkill(item.id, undefined, 'edit'); });
      edit.disabled = !item.canEdit;
      actions.append(edit,
        button('Permissions', () => { if (discardDraft()) openSkill(item.id, undefined, 'permissions'); }));
      row.append(summary, meta, actions); skillList.append(row);
    }
    skillListStatus.textContent = !listedSkills.length ? 'No skills available. Register your first skill or import a text package.' : !matching.length ? 'No matching skills. Try another search or scope.' : matching.length + ' skill' + (matching.length === 1 ? '' : 's');
  }
  search.addEventListener('input', renderSkills); scopeFilter.addEventListener('change', renderSkills);
  let listRead = 0;
  async function loadSkills() {
    const current = ++listRead;
    try { const value = await aiRpc('ListSkills'); if (current !== listRead) return; listedSkills = value.skills || []; renderSkills(); }
    catch (error) { if (current === listRead) skillListStatus.textContent = error.message; }
  }
  async function openSkill(id, revision, action) {
    const current = ++read, snapshot = JSON.stringify(draftPayload()); skillStatus.textContent = 'Loading skill…';
    exportButton.disabled = true;
    try {
      const [selected, history] = await Promise.all([aiRpc('GetSkill', {id, ...(revision ? {revision} : {})}), aiRpc('ListSkillVersions', {id})]);
      if (current !== read) return;
      if (JSON.stringify(draftPayload()) !== snapshot) { versions.value = String(skill?.revision || ''); skillStatus.textContent = 'Your newer edits were kept. Save or discard them before loading another version.'; return; }
      latestRevision = String((history.skills || []).reduce((latest, item) => BigInt(item.revision) > BigInt(latest) ? String(item.revision) : latest, '0'));
      editSkill(selected, history.skills || []);
      if (action === 'permissions') sharing.showModal();
      else if (action === 'edit') title.focus();
    } catch (error) { if (current === read) { versions.value = String(skill?.revision || ''); skillListStatus.textContent = error.message; } }
    finally { if (current === read) exportButton.disabled = !skill; }
  }
  function draftPayload() {
    return {id: skill?.id || '', expectedRevision: String(skill?.revision || '0'), title: title.value.trim(), description: description.value.trim(), instructions: instructions.value,
      references: [...refs.children].map(row => ({name: referencePath(row), content: row.querySelector('[data-reference-content]').value}))};
  }
  function editSkill(value, history = []) {
    const previousPath = skill?.id === value?.id && activeFile ? referencePath(activeFile) : null;
    if (skill?.id !== value?.id) folderState.clear();
    skill = value; editor.hidden = false; skillList.hidden = true;
    title.value = value?.title || ''; description.value = value?.description || ''; instructions.value = value?.instructions || '';
    owner.textContent = value ? `Owner: ${value.owner} · Version ${value.revision} · Existing conversations keep their selected version.` : 'New private draft. Review the files, then save to register it.';
    versions.replaceChildren();
    for (const item of history) versions.add(new Option('Version ' + item.revision, String(item.revision)));
    versions.value = String(value?.revision || ''); versions.disabled = !value;
    refs.replaceChildren(); referenceSequence = 0;
    for (const ref of value?.references || []) reference(ref);
    const editable = !value || (value.canEdit && String(value.revision) === latestRevision);
    title.disabled = description.disabled = instructions.disabled = addReference.disabled = saveSkill.disabled = !editable;
    for (const input of refs.querySelectorAll('input, textarea, button')) input.disabled = !editable;
    audience.value = value?.audience || 'private'; teamInput.value = value?.team || '';
    teamInput.parentElement.hidden = audience.value !== 'team';
    audience.disabled = teamInput.disabled = saveSharing.disabled = !value?.canEdit || String(value.revision) !== latestRevision;
    shareButton.disabled = exportButton.disabled = !value;
    deleteSkill.disabled = !value?.canEdit || String(value.revision) !== latestRevision;
    dangerZone.hidden = !value; renderVersions();
    sharingStatus.textContent = ''; sharing.close(); deleteDialog.close(); restoreFile(previousPath);
    skillStatus.textContent = value && !editable ? 'Read-only version. Choose the current version to edit if you own this skill.' : '';
    savedDraft = JSON.stringify(draftPayload()); dirty = false;
  }
  editor.addEventListener('input', () => { dirty = JSON.stringify(draftPayload()) !== savedDraft; });
  window.addEventListener('beforeunload', event => { if (dirty) { event.preventDefault(); event.returnValue = ''; } });
  versions.addEventListener('change', () => { if (skill && discardDraft()) openSkill(skill.id, versions.value); else versions.value = String(skill?.revision || ''); });
  function validatePackage(value) {
    const validPath = name => typeof name === 'string' && name.length <= 128 && name.split('/').every(part => /^[A-Za-z0-9._-]+$/.test(part) && part !== '.' && part !== '..');
    const files = [{name: 'SKILL.md', content: value.instructions}, ...value.references];
    const names = files.map(file => file.name);
    const encoder = new TextEncoder();
    const text = [value.title, value.description, ...files.flatMap(file => [file.name, file.content])];
    if (text.some(value => [...value].some(character => { const point = character.codePointAt(0); return point >= 0xd800 && point <= 0xdfff; }))) throw new Error('Invalid Unicode text in skill package.');
    if (!value.title.trim() || encoder.encode(value.title).length > 160 || encoder.encode(value.description).length > 512) throw new Error('Invalid skill metadata: name must contain text and fit within 160 UTF-8 bytes; description must fit within 512 UTF-8 bytes.');
    if (!value.instructions.trim() || value.references.length > 16 || new Set(names).size !== names.length || files.some(file => !validPath(file.name) || typeof file.content !== 'string' || file.content.includes('\0') || names.some(name => name !== file.name && file.name.startsWith(name + '/')))) throw new Error('Invalid skill package: check text files, paths and duplicates.');
    const bytes = [value.title, value.description, value.instructions, ...value.references.flatMap(ref => [ref.name, ref.content])].reduce((total, value) => total + encoder.encode(value).length, 0);
    if (bytes > 32768) throw new Error('Skill instructions and references must fit within 32 KiB.');
    if ((value.title + value.description).includes('\0')) throw new Error('Invalid skill metadata.');
  }
  importFile.addEventListener('change', async () => {
    const file = importFile.files[0]; if (!file) return;
    const current = ++read;
    try {
      if (file.size > 262144) throw new Error('Invalid package: import must fit within 256 KiB.');
      const buffer = await file.arrayBuffer();
      let text;
      try { text = new TextDecoder('utf-8', {fatal: true}).decode(buffer); }
      catch { throw new Error('Invalid UTF-8. Import a UTF-8 Markdown or JSON file.'); }
      if (current !== read) return;
      let draft;
      if (file.name.toLowerCase().endsWith('.md')) draft = {title: file.name.slice(0, -3), description: '', instructions: text, references: []};
      else {
        const data = JSON.parse(text);
        if (!data || typeof data.name !== 'string' || (data.description !== undefined && typeof data.description !== 'string') || !Array.isArray(data.files) || data.files.some(file => !file || typeof file.path !== 'string' || typeof file.content !== 'string' || file.encoding)) throw new Error('Invalid skill package. Expected name and UTF-8 text files.');
        const roots = data.files.filter(file => file.path === 'SKILL.md');
        if (roots.length !== 1) throw new Error('Invalid skill package: exactly one SKILL.md is required.');
        draft = {title: data.name, description: data.description || '', instructions: roots[0].content, references: data.files.filter(file => file.path !== 'SKILL.md').map(file => ({name: file.path, content: file.content}))};
      }
      validatePackage(draft); if (!discardDraft()) return; editSkill(null);
      title.value = draft.title; description.value = draft.description; instructions.value = draft.instructions;
      for (const ref of draft.references) reference(ref);
      showFile(null); dirty = true; skillStatus.textContent = 'Imported private draft. Review the files before saving.';
    } catch (error) { if (current === read) skillListStatus.textContent = error instanceof SyntaxError ? 'Invalid JSON skill package.' : error.message; }
    finally { importFile.value = ''; }
  });
  editor.addEventListener('submit', async event => {
    event.preventDefault(); if (saveSkill.disabled) return;
    const payload = draftPayload();
    try {
      if ([...refs.children].some(row => !/^[A-Za-z0-9._-]+$/.test(row.querySelector('[data-reference-name]').value.trim()))) throw new Error('Invalid file name. Use a name such as checklist.md; put directories in Folder.');
      validatePackage(payload);
    } catch (error) { skillStatus.textContent = error.message; return; }
    const current = read; saveSkill.disabled = true;
    try {
      const saved = await aiRpc('PutSkill', payload);
      await loadSkills();
      if (current !== read) return;
      const newer = draftPayload();
      const selectedPath = activeFile ? referencePath(activeFile) : null;
      const changed = JSON.stringify(newer) !== JSON.stringify(payload);
      const history = [...versions.options].map(option => ({revision: option.value}));
      history.push({revision: saved.revision}); latestRevision = String(saved.revision);
      editSkill(saved, history);
      if (changed) {
        title.value = newer.title; description.value = newer.description; instructions.value = newer.instructions;
        refs.replaceChildren(); referenceSequence = 0;
        for (const ref of newer.references) reference(ref);
        restoreFile(selectedPath); dirty = true;
        skillStatus.textContent = 'Version saved. Your newer edits are still unsaved.';
      } else {
        restoreFile(selectedPath);
        skillStatus.textContent = `Version ${saved.revision} saved. Existing conversations keep their selected version.`;
      }
    } catch (error) { if (current === read) { skillStatus.textContent = error.message; saveSkill.disabled = false; } }
  });

  const modelsPane = tab('Models', 'ai-models');
  const modelList = element('ul', undefined, 'list');
  modelList.id = 'model-list';
  modelsPane.append(element('h2', 'Available models'), modelList);
  const modelListStatus = status(modelsPane, 'model-list-status');
  const personalForm = element('form', undefined, 'ai-form');
  personalForm.append(element('h2', 'Personal model'));
  const modelId = field(personalForm, 'Registration name', 'personal-model-id');
  const modelUrl = field(personalForm, 'Base URL', 'personal-model-url', 'url');
  const modelName = field(personalForm, 'Provider model ID', 'personal-model-name');
  const modelToken = field(personalForm, 'Token', 'personal-model-token', 'password');
  modelToken.autocomplete = 'new-password';
  modelToken.spellcheck = false;
  for (const input of [modelId, modelUrl, modelName]) input.required = true;
  const personalSave = button('Save personal model');
  personalSave.type = 'submit';
  personalForm.append(personalSave, element('p', 'Tokens stay on the server. Replacing a registration requires entering its token again.', 'status'));
  const personalStatus = status(personalForm, 'personal-model-status');
  modelsPane.append(personalForm);
  async function loadModels() {
    try {
      const models = await json('/api/llm');
      modelList.replaceChildren();
      for (const model of models) {
        const row = element('li');
        row.append(element('span', `${model.ref || model.id} · ${model.model}`));
        if (model.scope === 'personal') {
          const edit = button('Edit ' + model.id, () => {
            modelId.value = model.id; modelUrl.value = model.base_url || model.baseUrl || ''; modelName.value = model.model; modelToken.value = '';
            modelId.focus();
          });
          edit.disabled = !policy.canRegister;
          row.append(edit, button('Delete ' + model.id, async () => {
            try { await aiRpc('DeletePersonalModel', {id: model.id}); await loadModels(); }
            catch (error) { modelListStatus.textContent = error.message; }
          }));
        }
        modelList.append(row);
      }
      modelListStatus.textContent = models.length ? '' : 'No models available. An administrator can grant company-model access.';
    } catch (error) { modelListStatus.textContent = error.message; }
  }
  personalForm.addEventListener('submit', async event => {
    event.preventDefault();
    if (!policy.canRegister) return;
    personalSave.disabled = true;
    try {
      await aiRpc('PutPersonalModel', {id: modelId.value.trim(), baseUrl: modelUrl.value.trim(), model: modelName.value.trim(), token: modelToken.value});
      modelToken.value = '';
      personalStatus.textContent = 'Personal model saved.';
      await loadModels();
    } catch (error) { personalStatus.textContent = error.message; }
    finally { personalSave.disabled = !policy.canRegister; }
  });

  const connectionsPane = tab('Connections', 'ai-connections');
  connectionsPane.append(element('h2', 'MCP connections'), element('p', 'Administrators manage remote tool servers and grant access to users or teams. Skills cannot grant tool permissions.', 'status'));
  const mcpList = element('ul', undefined, 'skill-library');
  const mcpStatus = status(connectionsPane, 'mcp-status');
  let mcpRevision = '0', mcpCanManage = false, mcpEdit = 0;
  const mcpForm = element('form', undefined, 'ai-form'); mcpForm.hidden = true;
  const mcpId = field(mcpForm, 'Connection ID', 'mcp-id');
  const mcpName = field(mcpForm, 'Connection name', 'mcp-name');
  const mcpDescription = field(mcpForm, 'Connection description', 'mcp-description');
  const mcpUrl = field(mcpForm, 'MCP endpoint', 'mcp-url', 'url');
  const authLabel = element('label', 'Connection authentication');
  const mcpAuth = document.createElement('select'); mcpAuth.setAttribute('aria-label', 'Connection authentication');
  mcpAuth.add(new Option('None', 'none')); mcpAuth.add(new Option('Bearer token', 'bearer')); authLabel.append(mcpAuth); mcpForm.append(authLabel);
  const mcpToken = field(mcpForm, 'Connection token', 'mcp-token', 'password'); mcpToken.autocomplete = 'new-password';
  const mcpUsers = field(mcpForm, 'Connection users', 'mcp-users', 'textarea');
  const mcpTeams = field(mcpForm, 'Connection teams', 'mcp-teams', 'textarea');
  const enabledLabel = element('label', 'Connection state');
  const mcpEnabled = document.createElement('select'); mcpEnabled.setAttribute('aria-label', 'Connection state');
  mcpEnabled.add(new Option('Enabled', 'true')); mcpEnabled.add(new Option('Disabled', 'false')); enabledLabel.append(mcpEnabled); mcpForm.append(enabledLabel);
  const mcpSave = button('Save connection'); mcpSave.type = 'submit';
  mcpForm.append(element('p', 'One user or configured team ID per line. Tokens are write-only. Leave the token blank to retain it for the same endpoint.', 'status'), mcpSave);
  for (const input of [mcpId, mcpName, mcpUrl]) input.required = true;
  function editMcp(c = {}) {
    ++mcpEdit;
    mcpForm.hidden = false; mcpRevision = String(c.revision || '0'); mcpId.value = c.id || ''; mcpId.readOnly = !!c.id;
    mcpName.value = c.name || ''; mcpDescription.value = c.description || ''; mcpUrl.value = c.url || ''; mcpAuth.value = c.auth || 'none';
    mcpToken.value = ''; mcpUsers.value = (c.subjects || []).join('\n'); mcpTeams.value = (c.teams || []).join('\n'); mcpEnabled.value = String(c.enabled !== false);
  }
  const newMcp = button('New connection', () => editMcp()); newMcp.hidden = true;
  connectionsPane.append(newMcp, button('Refresh connections', () => loadMcp()), mcpList, mcpForm);
  async function loadMcp() {
    try {
      const response = await aiRpc('ListMcpConnections'); mcpCanManage = !!response.canManage;
      newMcp.hidden = !mcpCanManage; newMcp.disabled = !response.configured;
      mcpList.replaceChildren();
      for (const c of response.connections || []) {
        const row = element('li', undefined, 'skill-library-row');
        const summary = element('div'); summary.append(element('strong', c.name), element('p', c.description || 'Remote MCP server', 'status'));
        row.append(summary, element('span', c.enabled ? 'Enabled' : 'Disabled', 'badge'));
        if (mcpCanManage) row.append(button('Manage ' + c.name, () => editMcp(c)));
        if (c.enabled) row.append(button('Tools for ' + c.name, () => loadTools(c)));
        mcpList.append(row);
      }
      if (!response.configured) mcpStatus.textContent = 'MCP registration needs server encryption keys and approved endpoint origins.';
      else if (!response.connections?.length) mcpStatus.textContent = mcpCanManage ? 'No connections registered. Add a remote MCP server to get started.' : 'No connections available. Ask an administrator for access.';
    } catch (error) { mcpStatus.textContent = error.message; }
  }
  function mcpPayload() {
    return {id:mcpId.value.trim(),name:mcpName.value.trim(),description:mcpDescription.value.trim(),url:mcpUrl.value.trim(),auth:mcpAuth.value,token:mcpToken.value,enabled:mcpEnabled.value === 'true',expectedRevision:mcpRevision,subjects:lines(mcpUsers.value),teams:lines(mcpTeams.value)};
  }
  mcpForm.addEventListener('submit', async event => {
    event.preventDefault(); if (!mcpCanManage || mcpSave.disabled) return;
    const current = mcpEdit, payload = mcpPayload(); mcpSave.disabled = true;
    try {
      const saved = await aiRpc('PutMcpConnection', payload);
      if (current === mcpEdit) {
        if (JSON.stringify(mcpPayload()) === JSON.stringify(payload)) editMcp(saved);
        else if (mcpId.value.trim() === saved.id) { mcpRevision = String(saved.revision); mcpId.readOnly = true; }
        if (mcpToken.value === payload.token) mcpToken.value = '';
      }
      await loadMcp(); mcpStatus.textContent = 'MCP connection saved. Any newer draft edits remain unsaved.';
    } catch (error) { mcpStatus.textContent = error.message; }
    finally { mcpSave.disabled = false; }
  });

  const toolPanel = element('section', undefined, 'ai-form'); toolPanel.hidden = true;
  toolPanel.append(element('h2', 'Connection tools'));
  const toolSelectLabel = element('label', 'Available tools');
  const toolSelect = document.createElement('select'); toolSelect.setAttribute('aria-label', 'Available tools'); toolSelectLabel.append(toolSelect); toolPanel.append(toolSelectLabel);
  const toolDescription = element('p', '', 'status'); toolPanel.append(toolDescription);
  const toolSchema = element('details'); toolSchema.append(element('summary', 'Argument schema'));
  const schemaText = element('pre'); toolSchema.append(schemaText); toolPanel.append(toolSchema);
  const toolArguments = field(toolPanel, 'Tool arguments (JSON)', 'mcp-tool-arguments', 'textarea'); toolArguments.value = '{}';
  const toolStatus = status(toolPanel, 'mcp-tool-status');
  const toolResult = element('pre'); toolResult.id = 'mcp-tool-result'; toolResult.setAttribute('aria-live', 'polite');
  const approvalButton = button('Approve as read-only', async () => {
    const tool = selectedTool(); if (!tool || !activeConnection) return;
    const current = toolsRead;
    approvalButton.disabled = true;
    try {
      const saved = await aiRpc('ApproveMcpRead', {id:activeConnection.id,expectedRevision:String(activeConnection.revision),tool:tool.name,schemaHash:tool.schemaHash,approved:!tool.approvedRead});
      if (current !== toolsRead) return;
      await loadTools(saved); await loadMcp();
    } catch (error) { if (current === toolsRead) toolStatus.textContent = error.message; }
    finally { approvalButton.disabled = false; }
  });
  let activeConnection = null, discoveredTools = [], toolsRead = 0;
  const confirmationDialog = element('dialog', undefined, 'ai-dialog'); confirmationDialog.setAttribute('aria-label', 'Confirm tool action');
  confirmationDialog.append(element('h2', 'Confirm tool action'), element('p', 'This tool is not approved as read-only. Review the connection, tool and exact arguments before continuing.'));
  const confirmationDetails = element('pre'); confirmationDialog.append(confirmationDetails);
  const confirmAction = button('Confirm action');
  const cancelAction = button('Cancel', () => { confirmationDialog.close(); });
  confirmationDialog.append(confirmAction, cancelAction); connectionsPane.append(confirmationDialog);
  let pendingCall = null;
  confirmationDialog.addEventListener('close', () => { pendingCall = null; });
  function selectedTool() { return discoveredTools.find(tool => tool.name === toolSelect.value); }
  function showTool() {
    const tool = selectedTool();
    toolDescription.textContent = tool ? tool.description + (tool.approvedRead ? ' · Admin-approved read: runs without another confirmation.' : ' · Confirmation required before execution.') : '';
    schemaText.textContent = tool?.schemaJson || '';
    approvalButton.hidden = !mcpCanManage || !tool;
    approvalButton.textContent = tool?.approvedRead ? 'Require confirmation' : 'Approve as read-only';
    runTool.disabled = !tool; toolSelect.disabled = !tool; toolArguments.disabled = !tool; toolResult.textContent = ''; toolStatus.textContent = '';
  }
  async function loadTools(connection) {
    const current = ++toolsRead; activeConnection = connection; mcpForm.hidden = true; toolPanel.hidden = false; runTool.disabled = true; toolArguments.disabled = true; toolArguments.value = '{}';
    discoveredTools = []; toolSelect.replaceChildren(); toolSelect.disabled = true; approvalButton.hidden = true; confirmationDialog.close();
    toolStatus.textContent = 'Discovering tools…';
    try {
      const response = await aiRpc('DiscoverMcpTools', {id:connection.id}); if (current !== toolsRead) return;
      discoveredTools = response.tools || []; toolSelect.replaceChildren(...discoveredTools.map(tool => new Option(tool.name, tool.name))); showTool();
      if (!discoveredTools.length) toolStatus.textContent = 'This connection exposes no tools.';
    } catch (error) { if (current === toolsRead) { discoveredTools = []; toolSelect.replaceChildren(); showTool(); toolStatus.textContent = error.message; } }
  }
  async function executeTool(request, current) {
    const response = await aiRpc('CallMcpTool', request);
    if (current !== toolsRead) return;
    if (response.confirmation) {
      pendingCall = {...request,confirmation:response.confirmation};
      confirmationDetails.textContent = activeConnection.name + '\n' + request.tool + '\n' + request.argumentsJson;
      confirmAction.disabled = false; confirmationDialog.showModal(); cancelAction.focus();
    } else {
      confirmationDialog.close();
      try { toolResult.textContent = JSON.stringify(JSON.parse(response.resultJson), null, 2); } catch { toolResult.textContent = response.resultJson || ''; }
      toolStatus.textContent = 'Tool completed. Results are untrusted content.';
    }
  }
  const runTool = button('Run tool', async () => {
    if (!activeConnection || !selectedTool() || runTool.disabled) return;
    const current = toolsRead; runTool.disabled = true; toolStatus.textContent = 'Requesting tool…'; toolResult.textContent = '';
    try { await executeTool({id:activeConnection.id,tool:toolSelect.value,argumentsJson:toolArguments.value}, current); }
    catch (error) { if (current === toolsRead) toolStatus.textContent = error.message; }
    finally { if (current === toolsRead) runTool.disabled = false; }
  });
  confirmAction.addEventListener('click', async () => {
    if (!pendingCall || confirmAction.disabled) return;
    const request = pendingCall, current = toolsRead; pendingCall = null; confirmAction.disabled = true; cancelAction.disabled = true;
    try { await executeTool(request, current); }
    catch (error) { confirmationDialog.close(); if (current === toolsRead) toolStatus.textContent = error.message; }
    finally { cancelAction.disabled = false; }
  });
  toolSelect.addEventListener('change', () => { ++toolsRead; confirmationDialog.close(); showTool(); });
  toolPanel.append(runTool, approvalButton, toolResult); connectionsPane.append(toolPanel);

  const adminPane = tab('Administration', 'ai-admin');
  tabs.get('ai-admin').trigger.hidden = true;
  const grantsForm = element('form', undefined, 'ai-form');
  grantsForm.append(element('h2', 'Personal-model permissions'), element('p', 'One user or configured team ID per line. Revocation blocks future registration, changes and use.', 'status'));
  const subjects = field(grantsForm, 'Allowed users', 'model-policy-subjects', 'textarea');
  const teams = field(grantsForm, 'Allowed teams', 'model-policy-teams', 'textarea');
  const grantsSave = button('Save personal-model permissions');
  grantsSave.type = 'submit';
  grantsForm.append(grantsSave);
  const policyStatus = status(grantsForm, 'model-policy-status');
  adminPane.append(grantsForm);
  grantsForm.addEventListener('submit', async event => {
    event.preventDefault();
    grantsSave.disabled = true;
    try {
      policy = await aiRpc('PutPersonalModelPolicy', {expectedRevision: String(policy.revision || '0'), subjects: lines(subjects.value), teams: lines(teams.value)});
      policyStatus.textContent = 'Personal-model permissions saved.';
      applyPolicy();
    } catch (error) { policyStatus.textContent = error.message; }
    finally { grantsSave.disabled = false; }
  });
  const companyList = element('ul', undefined, 'list');
  adminPane.append(element('h2', 'Company models'), companyList);
  const companyStatus = status(adminPane, 'company-model-status');
  const companyForm = element('form', undefined, 'ai-form');
  const companyId = field(companyForm, 'Company registration name', 'company-model-id');
  const companyUrl = field(companyForm, 'Company base URL', 'company-model-url', 'url');
  const companyModel = field(companyForm, 'Company provider model ID', 'company-model-name');
  const companyToken = field(companyForm, 'Company token', 'company-model-token', 'password');
  companyToken.autocomplete = 'new-password';
  for (const input of [companyId, companyUrl, companyModel]) input.required = true;
  const companySave = button('Save company model');
  companySave.type = 'submit';
  companyForm.append(companySave);
  adminPane.append(companyForm);
  const aclForm = element('form', undefined, 'ai-form');
  aclForm.hidden = true;
  const aclHeading = element('h3', 'Company model ACL');
  aclForm.append(aclHeading, element('p', 'One verified group ID per line. Registration alone does not grant use.', 'status'));
  const companyGroups = field(aclForm, 'Company model groups', 'company-model-groups', 'textarea');
  const companyRoles = field(aclForm, 'Company model roles', 'company-model-roles', 'textarea');
  companyRoles.placeholder = 'editor';
  const aclSave = button('Save company-model ACL');
  aclSave.type = 'submit';
  aclForm.append(aclSave);
  adminPane.append(aclForm);
  let aclId = null;
  async function loadCompany() {
    if (!policy.isAdmin) return;
    try {
      const models = await json('/api/admin/shared-models');
      companyList.replaceChildren();
      for (const model of models) {
        const row = element('li');
        row.append(button('Manage ' + model.id, async () => {
          companyId.value = model.id; companyUrl.value = model.base_url; companyModel.value = model.model; companyToken.value = '';
          try {
            const value = await json('/api/admin/shared-models/' + encodeURIComponent(model.id) + '/grants');
            aclId = model.id; sharedRevision = value.revision;
            companyGroups.value = (value.grants || []).filter(grant => grant.group).map(grant => grant.group).join('\n');
            companyRoles.value = (value.grants || []).filter(grant => grant.role).map(grant => grant.role).join('\n');
            aclHeading.textContent = 'Access to ' + model.id;
            aclForm.hidden = false;
          } catch (error) { companyStatus.textContent = error.message; }
        }), button('Delete company ' + model.id, async () => {
          try { await json('/api/admin/shared-models/' + encodeURIComponent(model.id), {method: 'DELETE'}); aclForm.hidden = true; await loadCompany(); }
          catch (error) { companyStatus.textContent = error.message; }
        }));
        companyList.append(row);
      }
    } catch (error) { companyStatus.textContent = error.message; }
  }
  companyForm.addEventListener('submit', async event => {
    event.preventDefault();
    companySave.disabled = true;
    try {
      await json('/api/admin/shared-models/' + encodeURIComponent(companyId.value.trim()), mutation({base_url: companyUrl.value.trim(), model: companyModel.value.trim(), api_key: companyToken.value}));
      companyToken.value = '';
      companyStatus.textContent = 'Company model saved. Set its ACL before granting use.';
      await loadCompany();
    } catch (error) { companyStatus.textContent = error.message; }
    finally { companySave.disabled = false; }
  });
  aclForm.addEventListener('submit', async event => {
    event.preventDefault();
    if (!aclId || sharedRevision === null) return;
    aclSave.disabled = true;
    try {
      await json('/api/admin/shared-models/' + encodeURIComponent(aclId) + '/grants', mutation({expected_revision: Number(sharedRevision), grants: [...lines(companyGroups.value).map(group => ({group})), ...lines(companyRoles.value).map(role => ({role}))]}));
      const value = await json('/api/admin/shared-models/' + encodeURIComponent(aclId) + '/grants');
      sharedRevision = value.revision;
      companyStatus.textContent = 'Company-model ACL saved.';
    } catch (error) { companyStatus.textContent = error.message; }
    finally { aclSave.disabled = false; }
  });
  function applyPolicy() {
    for (const input of personalForm.querySelectorAll('input, button')) input.disabled = !policy.canRegister;
    personalStatus.textContent = policy.canRegister ? '' : 'An administrator must grant permission to register or use personal models.';
    tabs.get('ai-admin').trigger.hidden = !policy.isAdmin;
    subjects.value = (policy.subjects || []).join('\n');
    teams.value = (policy.teams || []).join('\n');
    if (policy.isAdmin && !audience.querySelector('option[value="organization"]')) audience.add(new Option('Organization', 'organization'));
  }
  selectTab(location.hash === '#ai-models' ? 'ai-models' : 'ai-skills');
  applyPolicy();
  (async () => {
    try { policy = await aiRpc('GetPersonalModelPolicy'); applyPolicy(); }
    catch (error) { personalStatus.textContent = error.message + '. Registration is unavailable.'; }
    await Promise.all([loadSkills(), loadModels(), loadCompany(), loadMcp()]);
  })();
}


// Keep optional controls outside the message flow; native popovers own dismissal and focus.
function conversationComposer(panel) {
  if (panel.composer) return panel.composer;
  const form = panel.querySelector('form');
  if (!form) return null;
  form.classList.add('conversation-composer');
  form.querySelector('#chat-prompt, .cell-chat-prompt')?.setAttribute('aria-label', 'Message');
  const body = document.createElement('div'); body.className = 'conversation-body';
  for (const child of [...panel.children]) {
    if (child !== form && !child.classList.contains('chat-heading')) body.append(child);
  }
  form.before(body);
  const toolbar = document.createElement('div'); toolbar.className = 'composer-toolbar';
  const chips = document.createElement('div'); chips.className = 'composer-attachments';
  form.prepend(chips);
  const send = form.querySelector('[type="submit"]');
  form.append(toolbar); if (send) toolbar.append(send);
  const composer = panel.composer = {toolbar};
  composer.add = (name, content) => {
    const button = document.createElement('button'); button.type = 'button'; button.className = 'btn composer-control'; button.textContent = name;
    const popup = document.createElement('div'); popup.className = 'composer-popover'; popup.popover = 'auto';
    popup.setAttribute('aria-label', name); popup.setAttribute('role', 'region');
    popup.append(content); panel.append(popup);
    wireNotebookPopover(button, popup);
    toolbar.insertBefore(button, send);
    return button;
  };
  const attachment = panel.querySelector('.chat-attach, .cell-chat-attach');
  const context = panel.querySelector('#chat-context, .cell-chat-context');
  if (attachment && context) {
    const box = document.createElement('fieldset'); box.className = 'composer-attach-fields';
    const legend = document.createElement('legend'); legend.textContent = 'Attach context'; box.append(legend, attachment);
    const label = panel.querySelector('label[for="chat-context"]');
    if (label) box.append(label);
    if (!attachment.contains(context)) box.append(context);
    context.setAttribute('aria-label', 'Attached context');
    composer.attachBox = box;
    const button = composer.add('Attach', box);
    const chip = document.createElement('div'); chip.className = 'composer-attachment-chip';
    const review = document.createElement('button'); review.type = 'button'; review.textContent = 'Attached context'; review.className = 'btn'; review.popoverTargetElement = button.popoverTargetElement;
    const remove = document.createElement('button'); remove.type = 'button'; remove.textContent = '×'; remove.className = 'btn'; remove.setAttribute('aria-label', 'Remove attached context');
    composer.removeAttachment = remove;
    chip.append(review, remove); chips.append(chip);
    composer.syncContext = () => { chip.hidden = !context.value.trim(); };
    remove.addEventListener('click', () => { context.value = ''; composer.syncContext(); });
    context.addEventListener('input', composer.syncContext); composer.syncContext();
  }
  return composer;
}

function scrollConversation(history) {
  const viewport = history.closest('.conversation-body') || history;
  viewport.scrollTop = viewport.scrollHeight;
}

function closeComposerPopovers(panel) {
  panel?.querySelectorAll('[popover]:popover-open').forEach(popup => popup.hidePopover());
}

function searchableChoices(box, list, name) {
  const search = document.createElement('input'); search.type = 'search'; search.placeholder = name; search.setAttribute('aria-label', name);
  search.className = 'composer-search'; list.before(search);
  const filter = () => {
    const query = search.value.trim().toLocaleLowerCase();
    for (const row of list.children) row.hidden = !row.textContent.toLocaleLowerCase().includes(query);
  };
  search.addEventListener('input', filter);
  return filter;
}

// Shared by personal defaults and notebook/cell composers. Refresh never upgrades pins.
function createSkillControls(parent) {
  const box = document.createElement('fieldset'); box.className = 'ai-skill-picker';
  const legend = document.createElement('legend'); legend.textContent = 'Skills';
  const list = document.createElement('div'); list.className = 'ai-skill-choices';
  const refresh = document.createElement('button'); refresh.type = 'button'; refresh.className='btn'; refresh.textContent = 'Refresh skills';
  const status = document.createElement('p'); status.className = 'status'; status.setAttribute('role', 'status');
  box.append(legend, list, refresh, status); parent.prepend(box);
  const filter = searchableChoices(box, list, parent.closest('#ai-defaults') ? 'Search default skills' : 'Search skills');
  const state = {box, pins: [], choices: [], generation: 0, touched: false, ready: false};
  state.set = pins => { state.pins = pins.map(pin => ({id:pin.id,revision:String(pin.revision)})); state.render(); };
  state.render = () => {
    list.replaceChildren();
    const ids = [...new Set([...state.choices.map(item => item.id), ...state.pins.map(pin => pin.id)])];
    for (const id of ids) {
      const choices = state.choices.filter(item => item.id === id);
      const pin = state.pins.find(item => item.id === id);
      const name = choices[0]?.title || id;
      const row = document.createElement('div'); row.className = 'ai-skill-choice';
      const label = document.createElement('label');
      const enabled = document.createElement('input'); enabled.type = 'checkbox'; enabled.checked = Boolean(pin);
      label.append(enabled, document.createTextNode('Use ' + name));
      const versions = document.createElement('select'); versions.setAttribute('aria-label', 'Version for ' + name);
      for (const revision of [...new Set([...choices.map(item => String(item.revision)), ...(pin ? [pin.revision] : [])])].sort((a,b) => Number(b)-Number(a))) {
        versions.add(new Option('Version ' + revision, revision));
      }
      if (pin) versions.value = pin.revision;
      versions.disabled = !pin;
      const change = () => {
        state.touched = true;
        state.pins = state.pins.filter(item => item.id !== id);
        if (enabled.checked) state.pins.push({id,revision:versions.value});
        versions.disabled = !enabled.checked;
        status.textContent = state.pins.length + ' enabled. Versions stay pinned until you change them.';
        state.onchange?.();
      };
      enabled.addEventListener('change', change); versions.addEventListener('change', change);
      const history = document.createElement('button'); history.type = 'button'; history.className='btn'; history.textContent = 'Versions'; history.setAttribute('aria-label','Versions for '+name);
      history.addEventListener('click', async () => {
        const generation = state.generation, key = state.key; history.disabled = true;
        try {
          const result = await aiRpc('ListSkillVersions', {id});
          if (generation !== state.generation || key !== state.key) return;
          state.choices = state.choices.filter(item => item.id !== id).concat(result.skills || []); state.render();
        } catch (error) { if (generation === state.generation) status.textContent = error.message + '. Pins unchanged.'; }
        finally { history.disabled = false; }
      });
      row.append(label, versions, history); list.append(row);
    }
    status.textContent = state.pins.length + ' enabled. Off stops new instructions; earlier messages remain in context.';
    filter(); state.onchange?.();
  };
  state.load = async () => {
    const generation = ++state.generation; refresh.disabled = true;
    try {
      const result = await aiRpc('ListSkills');
      if (generation !== state.generation) return;
      state.choices = result.skills || []; state.ready = true; state.render();
    } catch (error) { if (generation === state.generation) status.textContent = error.message + '. Pins unchanged.'; }
    finally { if (generation === state.generation) refresh.disabled = false; }
  };
  refresh.addEventListener('click', state.load);
  return state;
}

function syncSkillPicker(panel, history) {
  if (!panel) return;
  if (!panel.skillPicker) {
    const composer = conversationComposer(panel);
    panel.skillPicker = createSkillControls(panel.querySelector('form') || panel);
    if (composer) {
      const button = composer.add('Skills (0)', panel.skillPicker.box);
      panel.skillPicker.onchange = () => { button.textContent = `Skills (${panel.skillPicker.pins.length})`; };
    }
    panel.skillPicker.load();
  }
  const state = panel.skillPicker;
  const key = String(history?.id || '') + ':' + String(history?.revision || '0');
  if (key === state.key && (state.defaultsReady || state.defaultsLoading)) return;
  const changed = key !== state.key;
  state.key = key;
  if (changed) state.touched = false;
  const last = (history?.messages || []).filter(message => message.role === 'user').at(-1);
  if (changed) state.set(history?.pendingTool?.skills || last?.skills || []);
  state.box.disabled = Boolean(history?.pendingTool);
  state.defaultsReady = Boolean(last || history?.pendingTool);
  if (!state.defaultsReady) {
    state.defaultsLoading = true;
    aiRpc('GetSkillDefaults').then(defaults => {
      if (state.key !== key) return;
      state.defaultsReady = true;
      if (!state.touched) state.set(defaults.skills || []);
    }).catch(() => {
      if (state.key !== key) return;
      state.defaultsReady = false;
      const status = state.box.querySelector('[role="status"]');
      status.textContent = 'Could not load personal defaults. Reopen this conversation before sending.';
    }).finally(() => { if (state.key === key) state.defaultsLoading = false; });
  }
}

function skillForTurn(panel) {
  const state = panel?.skillPicker;
  if (!state?.ready || !state.defaultsReady) throw new Error('Wait for skill settings to load.');
  return {selectedSkills:{skills:state.pins.map(pin => ({...pin}))}};
}

function syncNotebookMcp(panel, history) {
  if (!panel) return;
  let state = panel.mcpPicker;
  if (!state) {
    const composer = conversationComposer(panel);
    const box = document.createElement('fieldset'); box.className = 'ai-skill-picker';
    const legend = document.createElement('legend'); legend.textContent = 'MCP connections';
    const choices = document.createElement('div'); choices.className = 'ai-skill-choices';
    const status = document.createElement('p'); status.className = 'status'; status.setAttribute('role','status');
    const refresh = document.createElement('button'); refresh.type='button'; refresh.className='btn'; refresh.textContent='Refresh MCP connections';
    box.append(legend,choices,refresh,status); (panel.querySelector('form') || panel).prepend(box);
    const pending = document.createElement('section'); pending.className='notebook-tool-confirmation'; pending.hidden=true;
    const body = panel.querySelector('.conversation-body'); if (body) body.append(pending); else box.after(pending);
    const button = composer?.add('Tools (0)', box);
    const filter = searchableChoices(box, choices, 'Search tools');
    state=panel.mcpPicker={box,choices,status,pending,pins:[],connections:[],key:null};
    state.render=()=>{
      choices.replaceChildren();
      const ids=[...new Set([...state.connections.filter(c=>c.enabled).map(c=>c.id),...state.pins])];
      for (const id of ids) {
        const connection=state.connections.find(c=>c.id===id);
        const label=document.createElement('label');
        const input=document.createElement('input');input.type='checkbox';input.checked=state.pins.includes(id);
        label.append(input,document.createTextNode('Enable '+(connection?.name || id)));
        input.addEventListener('change',()=>{state.pins=state.pins.filter(pin=>pin!==id);if(input.checked)state.pins.push(id);if(button)button.textContent=`Tools (${state.pins.length})`;});
        choices.append(label);
      }
      filter(); if(button)button.textContent=`Tools (${state.pins.length})`;
      status.textContent='Only enabled connections are offered to AI. Approved reads run automatically; other actions need confirmation.';
    };
    const reload=async()=>{
      refresh.disabled=true;
      try {const result=await aiRpc('ListMcpConnections');state.connections=result.connections || [];state.render();}
      catch(error){status.textContent=error.message;}
      finally{refresh.disabled=false;}
    };
    refresh.addEventListener('click',reload);reload();
  }
  const key=String(history?.id || '')+':'+String(history?.revision || '0');
  if(state.key===key)return;
  state.key=key;
  const last=(history?.messages || []).filter(m=>m.role==='user').at(-1);
  state.pins=[...(history?.pendingTool?.connections || last?.mcpConnections || [])];
  state.render();state.pending.replaceChildren();state.pending.hidden=!history?.pendingTool;
  setAiControlsBusy(panel,Boolean(history?.pendingTool));
  if(!history?.pendingTool)return;
  const turn=history.pendingTool;
  const heading=document.createElement('h3');heading.textContent=turn.phase==='confirmation'?'Confirm tool action':'Tool turn status';
  const explanation=document.createElement('p');
  explanation.textContent=turn.phase==='confirmation'?'Review the exact connection, tool and arguments. Approval applies only to this action.':turn.phase==='failed'?'The turn stopped. An action may have reached its server; verify the outcome before retrying.':'This turn is running or was interrupted. Reload its status. Interrupted operations are never automatically retried.';
  const details=document.createElement('pre');details.textContent=[turn.connection,turn.tool,turn.argumentsJson].filter(Boolean).join('\n');
  const disclosure=document.createElement('details');
  const summary=document.createElement('summary');summary.textContent=[turn.connection,turn.tool].filter(Boolean).join(' · ') + ' — Arguments';
  disclosure.append(summary,details);
  state.pending.append(heading,explanation,disclosure);
  const action=(name,confirmation,cancel)=>{
    const button=document.createElement('button');button.type='button';button.className='btn';button.textContent=name;
    button.addEventListener('click',async()=>{
      for(const control of state.pending.querySelectorAll('button'))control.disabled=true;
      try {await resumeNotebookTool(panel,history,confirmation,cancel);}
      catch(error){if(state.key===key)explanation.textContent=error.message+' Reload status before retrying.';}
      finally{if(state.key===key)for(const control of state.pending.querySelectorAll('button'))control.disabled=false;}
    });state.pending.append(button);
  };
  if(turn.phase==='confirmation' && Number(turn.expiresAt)*1000>Date.now())action('Confirm action',turn.confirmation,false);
  if(turn.phase==='confirmation' || turn.phase==='failed' || Number(turn.expiresAt)*1000<=Date.now())action('Cancel tool turn','',true);
  const reload=document.createElement('button');reload.type='button';reload.className='btn';reload.textContent='Reload tool status';
  reload.addEventListener('click',()=>{const cell=panel.closest('.cell');if(cell)loadCellChat(cell);else loadConversation();});state.pending.append(reload);
}

function mcpForTurn(panel) { return {selectedMcp:{connections:[...(panel?.mcpPicker?.pins || [])]}}; }

function setAiControlsBusy(panel,busy) {
  panel?.querySelectorAll('.session-delete').forEach(button => { button.disabled = busy; });
  if (busy) closeComposerPopovers(panel);
  if (panel?.composer?.attachBox) panel.composer.attachBox.disabled = busy;
  if (panel?.composer?.removeAttachment) panel.composer.removeAttachment.disabled = busy;
  if(panel?.skillPicker)panel.skillPicker.box.disabled=busy;
  if(panel?.mcpPicker)panel.mcpPicker.box.disabled=busy;
}

async function resumeNotebookTool(panel,history,confirmation,cancel) {
  const cell=panel.closest('.cell');
  const body={notebook:nb.id,session:history.id,expectedRevision:history.revision,toolConfirmation:confirmation,cancelToolTurn:cancel,...(cell?{cell:cell.dataset.id}:{})};
  if(cell){
    const state=cellChatState(cell.dataset.id);if(state.busy)return;
    const current=state.read;state.busy=true;
    try{const result=await chatRpc('SendMessage',body);if(state.read===current && state.active===history.id){state.conversation=result;renderCellChat(cell);}}
    finally{state.busy=false;cellChatParts(cell).send.disabled=Boolean(state.conversation?.pendingTool);}
  }else{
    if(chatBusy)return;const current=conversationRead;chatBusy=true;updateChatSend();
    try{const result=await chatRpc('SendMessage',body);if(current===conversationRead && activeChatSession===history.id){conversation=result;renderConversation();}}
    finally{chatBusy=false;updateChatSend();}
  }
}
/* ---- notebook editing and navigation -------------------------------- */
const cellUndo = [];
const cellRedo = [];
let selectionAnchor = null;
let cellNavigation = false;
let notebookQueue = false;
let stopNotebookQueue = false;
function notebookCells() { return [...cellsRoot.querySelectorAll(':scope > .cell')]; }
function selectedCells() {
  const selected = notebookCells().filter(cell => cell.classList.contains('selected'));
  return selected.length ? selected : (currentCell?.isConnected ? [currentCell] : notebookCells().slice(0, 1));
}
function selectCell(cell, extend = false) {
  const cells = notebookCells();
  if (extend && selectionAnchor?.isConnected) {
    const a = cells.indexOf(selectionAnchor), b = cells.indexOf(cell);
    cells.forEach((entry, i) => entry.classList.toggle('selected', i >= Math.min(a,b) && i <= Math.max(a,b)));
  } else {
    cells.forEach(entry => entry.classList.toggle('selected', entry === cell));
    selectionAnchor = cell;
  }
  currentCell = cell;
  syncCellSelection();
}
function syncCellSelection() {
  for (const cell of notebookCells()) {
    cell.classList.toggle('active-cell', cell === currentCell);
    cell.querySelector('.cell-select').checked = cell.classList.contains('selected');
  }
  const count = notebookCells().filter(cell => cell.classList.contains('selected')).length;
  const toolbar = document.getElementById('selection-toolbar');
  if (toolbar) {
    toolbar.hidden = !count || !cellNavigation;
    toolbar.querySelector('.selection-count').textContent = `${count} cell${count === 1 ? '' : 's'} selected`;
  }
  const mode = document.getElementById('notebook-mode');
  if (mode) mode.textContent = cellNavigation ? 'Cell navigation · Enter to edit · ? for shortcuts' : 'Editing · Esc for cell navigation';
}
function cellSnapshot() {
  return notebookCells().map(node => ({node, sql: node.querySelector('.editor').value,
    selected: node.classList.contains('selected')}));
}
function rememberCellOperation(before) {
  const after = cellSnapshot();
  if (before.length === after.length && before.every((v,i) => v.node === after[i].node && v.sql === after[i].sql)) return;
  cellUndo.push({before, after});
  if (cellUndo.length > 50) cellUndo.shift();
  cellRedo.length = 0;
  setDirty(true);
  syncCellSelection();
}
function undoCellOperation(redo = false) {
  if (notebookCells().some(c => c.classList.contains('running'))) return status('Wait for running cells before undo.');
  const source = redo ? cellRedo : cellUndo, target = redo ? cellUndo : cellRedo;
  const entry = source.at(-1);
  if (!entry) return status(redo ? 'Nothing to redo.' : 'Nothing to undo.');
  const from = redo ? entry.before : entry.after, to = redo ? entry.after : entry.before;
  const changed = to.filter(item => from.some(old => old.node === item.node && old.sql !== item.sql));
  if (changed.some(item => item.node.querySelector('.editor').value !== from.find(old => old.node === item.node).sql)) {
    return status('Source changed after this operation. Undo those text edits before undoing the cell operation.');
  }
  cellsRoot.replaceChildren(...to.map(item => item.node));
  for (const item of changed) {
    item.node.querySelector('.editor').value = item.sql;
    setCellKind(item.node, cellKind(item.sql));
  }
  for (const item of to) item.node.classList.toggle('selected', item.selected);
  currentCell = to.find(item => item.selected)?.node || to[0]?.node || null;
  selectionAnchor = currentCell;
  source.pop(); target.push(entry);
  syncCellSelection(); autosizeAll(); setDirty(true);
  if (cellNavigation && currentCell) currentCell.focus();
}
function editSelectedCells(action) {
  const cells = selectedCells();
  if (!cells.length) return;
  if (cells.some(c => c.classList.contains('running'))) return status('Wait for selected queries to finish.');
  const before = cellSnapshot();
  if (action === 'delete') {
    if (cells.length === notebookCells().length) return status('A notebook keeps at least one cell.');
    const next = notebookCells().find(c => !cells.includes(c));
    cells.forEach(c => c.remove()); selectCell(next);
  } else if (action === 'copy') {
    const copies = cells.map(cell => {
      const copy = cellTemplate('c' + crypto.randomUUID(), cell.querySelector('.editor').value);
      copy.metadata = structuredClone(cell.metadata || {});
      copy.legacyEngine = cell.legacyEngine;
      setCellKind(copy, cellKind(copy.querySelector('.editor').value));
      return copy;
    });
    cells.at(-1).after(...copies);
    notebookCells().forEach(c => c.classList.toggle('selected', copies.includes(c)));
    currentCell = copies[0]; selectionAnchor = currentCell;
  } else {
    const ordered = action === 'up' ? cells : [...cells].reverse();
    for (const cell of ordered) {
      const sibling = action === 'up' ? cell.previousElementSibling : cell.nextElementSibling;
      if (sibling && !cells.includes(sibling)) {
        if (action === 'up') sibling.before(cell); else sibling.after(cell);
      }
    }
  }
  rememberCellOperation(before);
}
function splitCell() {
  const cell = selectedCells()[0];
  if (!cell || cell.classList.contains('running')) return;
  const area = cell.querySelector('.editor'), sql = area.value, index = area.selectionStart;
  const markdown = cellKind(sql) === 'markdown';
  if (markdown && index <= sql.indexOf('%md') + 3) return status('Split Markdown after the %md line.');
  const before = cellSnapshot();
  area.value = sql.slice(0,index);
  const next = cellTemplate('c' + crypto.randomUUID(), (markdown ? '%md\n' : '') + sql.slice(index));
  next.metadata = structuredClone(cell.metadata || {});
  next.legacyEngine = cell.legacyEngine;
  cell.after(next);
  setCellKind(cell, cellKind(area.value)); setCellKind(next, cellKind(next.querySelector('.editor').value));
  selectCell(next); rememberCellOperation(before); autosizeAll();
}
function mergeCell() {
  const cell = selectedCells()[0], next = cell?.nextElementSibling;
  if (!next || cell.classList.contains('running') || next.classList.contains('running')) return;
  const area = cell.querySelector('.editor'), other = next.querySelector('.editor');
  if (cell.legacyEngine !== next.legacyEngine) return status('Cell engines differ; merge refused.');
  if (cellKind(area.value) !== cellKind(other.value)) return status('Cannot merge SQL and Markdown cells.');
  // Do not move metadata between existing cell identities during a merge.
  if (JSON.stringify(cell.metadata || {}) !== JSON.stringify(next.metadata || {})) return status('Cell metadata differs; merge refused.');
  const before = cellSnapshot();
  area.value += '\n' + (cellKind(other.value) === 'markdown' ? cellSource(other.value) : other.value);
  next.remove(); selectCell(cell); setCellKind(cell, cellKind(area.value));
  rememberCellOperation(before); autosizeAll();
}
function notebookDialog(title) {
  const dialog = document.createElement('dialog'); dialog.className = 'notebook-dialog';
  const heading = document.createElement('h2'); heading.id = 'notebook-dialog-' + crypto.randomUUID(); heading.textContent = title;
  dialog.setAttribute('aria-labelledby', heading.id);
  const close = cellButton('Close', 'btn', ''); close.addEventListener('click', () => dialog.close());
  dialog.append(heading, close);
  dialog.addEventListener('keydown', event => {
    if (event.key === 'Escape') {event.preventDefault(); event.stopPropagation(); dialog.close();}
  });
  const opener = document.activeElement.closest('dialog') ? document.getElementById('notebook-actions') : document.activeElement;
  dialog.addEventListener('close', () => { dialog.remove(); if (opener?.isConnected && !document.querySelector('dialog[open]') && !document.activeElement.closest('.cell')) opener.focus(); }, {once:true});
  document.body.append(dialog); dialog.showModal();
  return dialog;
}
function showOutline() {
  const dialog = notebookDialog('Notebook outline'); let count = 0;
  for (const cell of notebookCells()) {
    const source = cell.querySelector('.editor').value;
    if (cellKind(source) !== 'markdown') continue;
    let fence = null;
    for (const line of cellSource(source).split('\n')) {
      const marker = line.match(/^\s{0,3}(`{3,}|~{3,})/);
      if (marker) {
        if (!fence) fence = marker[1];
        else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = null;
        continue;
      }
      const match = !fence && line.match(/^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$/);
      if (!match) continue;
      const button = cellButton(match[2], 'btn outline-entry', ''); button.style.marginInlineStart = (match[1].length - 1) + 'rem';
      button.addEventListener('click', () => {dialog.close(); selectCell(cell); cell.scrollIntoView({block:'start'}); cell.querySelector('.cell-select').focus();});
      dialog.append(button); count++;
    }
  }
  if (!count) dialog.append(document.createTextNode('Add Markdown headings to build an outline.'));
}
function dialogField(dialog, title, tag = 'input') {
  const label = document.createElement('label'); label.textContent = title;
  const input = document.createElement(tag); label.append(input); dialog.append(label); return input;
}
function showNotebookSearch() {
  const dialog = notebookDialog('Find and replace');
  const find = dialogField(dialog, 'Find literal text');
  const replacement = dialogField(dialog, 'Replace with');
  const scope = dialogField(dialog, 'Search scope', 'select');
  scope.append(new Option('Whole notebook','all'), new Option('Selected cells','selected'));
  const preview = document.createElement('pre'); preview.className = 'search-preview';
  const previewButton = cellButton('Preview replacements', 'btn', '');
  const apply = cellButton('Apply replacements','btn primary',''); apply.disabled = true;
  dialog.append(previewButton, apply, preview);
  let pending = null;
  function invalidate() { pending = null; apply.disabled = true; preview.textContent = ''; }
  [find,replacement,scope].forEach(input => input.addEventListener('input', invalidate));
  previewButton.addEventListener('click', () => {
    if (!find.value) return invalidate();
    const cells = scope.value === 'all' ? notebookCells() : selectedCells();
    pending = {snapshot:cellSnapshot(), changes:cells.map(node => {
      const before = node.querySelector('.editor').value;
      return {node,before,after:before.split(find.value).join(replacement.value)};
    }).filter(item => item.before.includes(find.value))};
    preview.textContent = pending.changes.map(item => item.node.dataset.id + '\nBefore:\n' + item.before + '\nAfter:\n' + item.after).join('\n\n') || 'No matches.';
    apply.disabled = !pending.changes.length;
  });
  apply.addEventListener('click', () => {
    if (!pending) return;
    const current = cellSnapshot();
    if (current.length !== pending.snapshot.length || current.some((item,i) => item.node !== pending.snapshot[i].node || item.sql !== pending.snapshot[i].sql)) {
      invalidate(); preview.textContent = 'Source changed; preview again.'; return;
    }
    const before = cellSnapshot();
    pending.changes.forEach(item => { item.node.querySelector('.editor').value = item.after; setCellKind(item.node,cellKind(item.after)); });
    rememberCellOperation(before); autosizeAll(); invalidate(); preview.textContent = 'Replacements applied. Undo cell operation restores the source.';
  });
  find.focus();
}
async function runSelectedCells(cells = selectedCells()) {
  if (notebookQueue) return status('A notebook run queue is already active.');
  notebookQueue = true; stopNotebookQueue = false;
  try {
    for (let i=0;i<cells.length;i++) {
      if (stopNotebookQueue) break;
      const cell = cells[i];
      if (!cell.isConnected || cell.classList.contains('running')) continue;
      status(`Running cell ${i+1} of ${cells.length}`);
      await run(cell.querySelector('.btn-run'));
    }
  } finally { notebookQueue = false; status(stopNotebookQueue ? 'Queue stopped; in-flight query was allowed to finish.' : 'Notebook queue finished.'); }
}
function displayedCsv(result) {
  const escape = value => {
    let text = value == null ? '' : typeof value === 'object' ? JSON.stringify(value) : String(value);
    // Spreadsheet applications interpret formula-shaped strings even inside CSV quotes.
    if (typeof value === 'string' && /^[\s]*[=+@-]/.test(text)) text = "'" + text;
    return '"' + text.replaceAll('"','""') + '"';
  };
  return [result.columns.map(c => c.name), ...result.rows].map(row => row.map(escape).join(',')).join('\r\n') + '\r\n';
}
function outputControls(out, result) {
  const meta = out.querySelector('.out-meta'), body = out.querySelector('.out-body');
  const toggle = cellButton('Collapse output','btn ghost','');
  toggle.addEventListener('click', () => {body.hidden = !body.hidden; toggle.textContent = body.hidden ? 'Expand output' : 'Collapse output';});
  const clear = cellButton('Clear displayed output','btn ghost','');
  clear.title = 'Clears browser output and future visible-output attachments; stored exchanges remain.';
  clear.addEventListener('click', () => {outBox(out).replaceChildren(); outBox(out).assistantText = ''; status('Displayed output cleared. Stored exchanges remain.');});
  const download = cellButton('Download displayed rows as CSV','btn ghost','');
  download.addEventListener('click', () => {
    const url = URL.createObjectURL(new Blob([displayedCsv(result)], {type:'text/csv;charset=utf-8'}));
    const link = document.createElement('a'); link.href = url;
    link.download = 'displayed-rows' + (result.truncated ? '-truncated' : '') + '.csv';
    link.click(); setTimeout(() => URL.revokeObjectURL(url),1000);
  });
  meta.append(toggle,clear,download);
}
async function showNotebookHistory() {
  if (dirty || saving) return status('Save your edits before opening revision history.');
  const expected = contentRevision, generation = editGeneration;
  const dialog = notebookDialog('Notebook revision history');
  const message = document.createElement('p'); message.setAttribute('role','status'); message.textContent = 'Loading recent revisions…';
  const versions = dialogField(dialog, 'Revision', 'select');
  const preview = document.createElement('pre'); preview.className = 'revision-preview';
  const restore = cellButton('Restore as new revision','btn primary',''); restore.disabled = true;
  dialog.append(message,preview,restore);
  const scope = {id:nb.id, ...(team ? {team,workspace} : {})};
  let candidate = null, requestGeneration = 0;
  async function load() {
    const token = ++requestGeneration; candidate = null; restore.disabled = true;
    try {
      const result = await chatRpc('GetNotebookRevision',{...scope,revision:versions.value});
      if (token !== requestGeneration || !dialog.isConnected) return;
      candidate = JSON.parse(result.notebookJson);
      const current = {...nb, engine:notebookEngineId(), context:notebookContextFromDom(), cells:cellsFromDom()};
      const oldLines = JSON.stringify(current,null,2).split('\n'), newLines = JSON.stringify(candidate,null,2).split('\n');
      const diff = [];
      for (let i=0;i<Math.max(oldLines.length,newLines.length);i++) {
        if (oldLines[i] === newLines[i]) diff.push('  ' + oldLines[i]);
        else {if (i<oldLines.length) diff.push('- ' + oldLines[i]); if (i<newLines.length) diff.push('+ ' + newLines[i]);}
      }
      preview.textContent = 'Changes from current (-) to selected revision (+):\n' + diff.join('\n');
      message.textContent = 'Compare complete source, metadata, Data and Compute. Restore creates a new revision; it never executes cells.';
      restore.disabled = false;
    } catch(error) {message.textContent = error.message;}
  }
  versions.addEventListener('change',load);
  restore.addEventListener('click', async () => {
    if (!candidate || dirty || saving || generation !== editGeneration) {restore.disabled=true; message.textContent='Notebook changed; close history and save before previewing again.'; return;}
    restore.disabled=true; saving=true;
    try {
      const response = await fetch('/aster.v1.Aster/RestoreNotebookRevision',{method:'POST',headers:{'content-type':'application/json','connect-protocol-version':'1'},
        body:JSON.stringify({...scope, revision:versions.value, expectedContentRevision:expected})});
      if (!response.ok) {message.textContent=response.status===409 ? 'Restore conflict: notebook changed. Reload and review again; nothing was overwritten.' : 'Restore failed. No retry was attempted.'; candidate=null; return;}
      location.reload();
    } catch (_) {message.textContent='Restore response unavailable. Reload to inspect the saved revision before trying again.'; candidate=null;}
    finally {saving=false;}
  });
  try {
    const result = await chatRpc('ListNotebookRevisions',scope);
    for (const revision of result.revisions || []) versions.append(new Option(revision,revision));
    if (versions.options.length) await load(); else message.textContent='No revisions available.';
  } catch(error) {message.textContent=error.message;}
}
function showNotebookCommands() {
  const editor = document.activeElement.matches('.editor') ? document.activeElement : null;
  const caret = editor ? {source:editor.value,start:editor.selectionStart,end:editor.selectionEnd,direction:editor.selectionDirection} : null;
  const dialog = notebookDialog('Notebook commands');
  const search = dialogField(dialog,'Search commands'); search.type='search';
  const actions = [
    ['Undo cell operation',()=>undoCellOperation()],['Redo cell operation',()=>undoCellOperation(true)],
    ['Notebook outline',showOutline],['Find and replace',showNotebookSearch],['Revision history',showNotebookHistory],
    ['Duplicate selected cells',()=>editSelectedCells('copy')],['Split cell at cursor',splitCell],['Merge with next cell',mergeCell],
    ['Move selected cells up',()=>editSelectedCells('up')],['Move selected cells down',()=>editSelectedCells('down')],
    ['Delete selected cells',()=>editSelectedCells('delete')],['Run selected cells',runSelectedCells],
    ['Stop queued runs',()=>{stopNotebookQueue=true; status('Stopping queued work; in-flight query continues.');}],
  ];
  const buttons = actions.map(([label,action]) => {
    const button=cellButton(label,'btn command-entry','');
    button.addEventListener('click',()=>{
      dialog.close();
      // Native dialog focus restoration can reset the editor's selection.
      if (editor?.isConnected && editor.value === caret.source) editor.setSelectionRange(caret.start,caret.end,caret.direction);
      action();
    }); dialog.append(button); return button;
  });
  search.addEventListener('input',()=>buttons.forEach(button=>{button.hidden=!button.textContent.toLowerCase().includes(search.value.toLowerCase());}));
  search.addEventListener('keydown',event=>{if(event.key==='Enter'){event.preventDefault();buttons.find(button=>!button.hidden)?.click();}});
  search.focus();
}
function focusCellNavigation(cell) {
  cellNavigation = true;
  if (cell) {currentCell = cell; cell.focus(); cell.scrollIntoView({block:'nearest'});}
  syncCellSelection();
}
function editActiveCell(cell) {
  if (!cell) return;
  cellNavigation = false; selectCell(cell);
  if (cell.dataset.kind === 'markdown' && !cell.classList.contains('md-editing')) toggleMarkdownEdit(cell);
  else cell.querySelector('.editor').focus();
}
async function runSelectionAndAdvance() {
  if (notebookQueue) return status('A notebook run queue is already active.');
  const cells = selectedCells(), last = cells.at(-1), focused = document.activeElement;
  await runSelectedCells(cells);
  const selected = selectedCells();
  if (stopNotebookQueue || !last?.isConnected || !cellNavigation || document.activeElement !== focused ||
      selected.length !== cells.length || selected.some((cell,i) => cell !== cells[i])) return;
  const next = last.nextElementSibling || addCell(last);
  selectCell(next); focusCellNavigation(next);
}
function showKeyboardHelp() {
  const dialog = notebookDialog('Keyboard shortcuts');
  const note = document.createElement('p');
  note.textContent = 'Cell navigation shortcuts apply when a cell or its selection checkbox has focus. Text fields keep their own shortcuts. Escape closes autocomplete first.';
  dialog.append(note);
  const table = document.createElement('table');
  for (const [key, action] of [
    ['Esc / Enter', 'Leave editing / edit active cell'],
    ['↑ / ↓', 'Select previous / next cell'],
    ['Shift+↑/↓', 'Extend or shrink the selection'],
    ['Ctrl/Cmd+Shift+↑/↓', 'Move selected cells'],
    ['Ctrl/Cmd+Enter', 'Run selection in navigation; run current cell when editing'],
    ['Shift+Enter', 'Run selection and advance in navigation; run current cell and advance when editing'],
    ['A / B', 'Insert SQL above / below the active cell'],
    ['Z / Shift+Z', 'Undo / redo cell operation in navigation'],
    ['Ctrl/Cmd+S', 'Save notebook'],
    ['Ctrl/Cmd+Shift+P', 'Find action'], ['?', 'Show this help in navigation'],
  ]) {
    const row = document.createElement('tr'), name = document.createElement('th'), description = document.createElement('td');
    name.textContent = key; name.scope = 'row'; description.textContent = action;
    row.append(name,description); table.append(row);
  }
  dialog.append(table);
}
function notebookKeydown(event) {
  if (event.defaultPrevented || event.isComposing || event.altKey) return;
  const target = event.target, accelerator = event.ctrlKey || event.metaKey;
  if (target.closest('dialog[open],[popover]:popover-open')) return;
  if (accelerator && event.shiftKey && event.key.toLowerCase() === 'p') {
    event.preventDefault(); if (!event.repeat) showNotebookCommands(); return;
  }
  if (target.matches('.editor') && event.key === 'Escape') {
    event.preventDefault(); selectCell(target.closest('.cell')); focusCellNavigation(target.closest('.cell')); return;
  }
  // Only cell navigation surfaces own these keys; buttons and text inputs keep native behavior.
  if (!target.matches('.cell,.cell-select,#cells') || !cellNavigation) return;
  const cell = currentCell?.isConnected ? currentCell : notebookCells()[0];
  if (!cell) return;
  const key = event.key.toLowerCase();
  if (key === 'arrowup' || key === 'arrowdown') {
    event.preventDefault();
    const delta = key === 'arrowup' ? -1 : 1;
    if (accelerator && event.shiftKey) {
      if (!event.repeat) editSelectedCells(delta < 0 ? 'up' : 'down');
      focusCellNavigation(currentCell); return;
    }
    if (accelerator) return;
    const cells = notebookCells(), next = cells[cells.indexOf(cell) + delta];
    if (next) {selectCell(next,event.shiftKey); focusCellNavigation(next);}
    return;
  }
  if (key === 'enter') {
    event.preventDefault();
    if (event.repeat) return;
    if (accelerator) void runSelectedCells();
    else if (event.shiftKey) void runSelectionAndAdvance();
    else editActiveCell(cell);
    return;
  }
  if (key === 'z' && (!accelerator || !target.matches('input'))) {
    event.preventDefault(); if (!event.repeat) undoCellOperation(event.shiftKey); return;
  }
  if (accelerator) return;
  if (key === 'a' || key === 'b' || key === '?') {
    event.preventDefault(); if (event.repeat) return;
    if (key === '?') showKeyboardHelp(); else addCell(cell,key === 'a');
  }
}
if (nb) {
  const bar = document.querySelector('.nb-bar');
  const help = cellButton('Shortcuts', 'btn ghost', ''); help.setAttribute('aria-label','Keyboard shortcuts');
  help.addEventListener('click',showKeyboardHelp);
  const more = cellButton('⋯','btn ghost',''); more.id = 'notebook-actions'; more.setAttribute('aria-label','Notebook actions');
  bar.append(help,more);
  const popup = notebookPopover(more,'Notebook actions');
  for (const [label, action] of [['Find action',showNotebookCommands],['Notebook outline',showOutline],['Find and replace',showNotebookSearch],['Revision history',showNotebookHistory]]) {
    const button = cellButton(label,'btn ghost','');
    button.addEventListener('click',()=>{popup.hidePopover(); action();}); popup.append(button);
  }
  const mode = bar.nextElementSibling;
  if (mode?.matches('.status')) mode.id = 'notebook-mode';
  cellsRoot.tabIndex = -1;
  const toolbar = document.createElement('div'); toolbar.id='selection-toolbar'; toolbar.className='selection-toolbar';
  toolbar.setAttribute('role','toolbar'); toolbar.setAttribute('aria-label','Selected cells'); toolbar.hidden=true;
  const count = document.createElement('span'); count.className='selection-count'; count.setAttribute('role','status'); toolbar.append(count);
  for (const [label, text, action] of [
    ['Run selected cells','▶ Run selected',()=>runSelectedCells()],
    ['Move selected cells up','↑',()=>editSelectedCells('up')],
    ['Move selected cells down','↓',()=>editSelectedCells('down')],
    ['Duplicate selected cells','Duplicate',()=>editSelectedCells('copy')],
    ['Delete selected cells','Delete',()=>editSelectedCells('delete')],
    ['Clear selection','Clear selection',()=>{
      notebookCells().forEach(cell=>cell.classList.remove('selected'));
      currentCell=null;selectionAnchor=null;syncCellSelection();cellsRoot.focus();
    }],
  ]) {
    const button = cellButton(text,'btn','');button.setAttribute('aria-label',label);button.addEventListener('click',action);toolbar.append(button);
  }
  cellsRoot.before(toolbar); syncCellSelection();
  document.addEventListener('keydown',notebookKeydown);
}
