"""Catalog UI regressions with real app.js and controlled API response ordering."""
import os
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
HTML = '''<button id="catalog-menu"></button><div id="catalog-page" data-docs="enabled">
<aside id="catalog-rail"><input id="catalog-search"><div id="catalog-search-results" hidden></div>
<div id="catalog-tree"></div><p id="catalog-nav-status"></p></aside><div id="catalog-detail"></div></div>'''
FIXTURE = r'''
window.catalogFixture = {pending: {}, failTree: false, holdDetail: false, holdHistory: false};
window.fetch = async (input, options = {}) => {
  const url = new URL(input, location.origin), q = url.searchParams, f = window.catalogFixture;
  const response = (value, status = 200) => new Response(JSON.stringify(value), {status});
  if (url.pathname === '/api/catalog/docs' && options.method === 'PUT') {
    if (f.saveError) throw new Error('Network unavailable');
    return new Promise(resolve => { f.pending.save = () => resolve(response({})); });
  }
  if (url.pathname === '/api/catalog/tree') {
    if (!q.has('catalog')) return response({catalogs: [{id: 'lake', kind: 'mock'}]});
    if (f.holdTree) { f.treeCalls = (f.treeCalls || 0) + 1; return new Promise(resolve => { f.pending.tree = () => resolve(response({namespaces: [{name: 'sales', token: 'sales'}]})); }); }
    if (f.failTree) { f.failTree = false; return response({error: 'Temporary discovery failure'}, 503); }
    return response(q.has('namespace') ? {tables: [{name: 'orders'}, {name: 'customers'}]} : {namespaces: [{name: 'sales', token: 'sales'}]});
  }
  if (url.pathname === '/api/catalog/detail') {
    if (f.holdDetail && q.get('table') === 'orders') return new Promise(resolve => { f.pending.detail = () => resolve(response({error: 'Old selection failed'}, 503)); });
    const entity = {catalog: q.get('catalog'), namespace: q.get('namespace'), table: q.get('table')};
    return response({entity, level: entity.table ? 'table' : 'catalog', details: {...entity, kind: 'mock'},
      docs: {enabled: true, markdown: 'Current ' + (entity.table || entity.catalog)}, assets: [],
      history: {docs: [{oid: 'abc123', message: 'Earlier document', author: 'Alice', date: 'today'}], queries: []}, permissions: {metadata: 'visible', engines: []}});
  }
  if (url.pathname === '/api/catalog/docs' && f.holdHistory) return new Promise(resolve => { f.pending.history = () => resolve(response({document: {markdown: 'Old orders document'}})); });
  return response({});
};
'''

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=os.environ.get('ASTER_BROWSER_EXECUTABLE'), headless=True)
    page = browser.new_page()
    page.route('http://aster.test/**', lambda route: route.fulfill(body=HTML, content_type='text/html'))
    failures = []

    def reset():
        page.goto('http://aster.test/catalog')
        page.evaluate(FIXTURE)
        page.add_script_tag(content=(ROOT / 'crates/server/assets/app.js').read_text())
        expect(page.locator('.catalog-node')).to_have_count(1)

    def expand():
        page.locator('.catalog-node > .tree-row .tree-toggle').click()
        page.locator('.schema-node > .tree-row .tree-toggle').click()
        expect(page.locator('.table-node')).to_have_count(2)

    def disclosure():
        reset()
        toggle = page.locator('.catalog-node > .tree-row .tree-toggle')
        toggle.click()
        expect(page.locator('.schema-node')).to_be_visible()
        toggle.click()
        expect(page.locator('.schema-node')).not_to_be_visible()
        expect(toggle).to_have_attribute('aria-expanded', 'false')
        expect(toggle).to_have_attribute('aria-label', 'Expand lake')
        toggle.click()
        expect(page.locator('.schema-node')).to_be_visible()

    def retry():
        reset()
        page.evaluate('catalogFixture.failTree = true')
        toggle = page.locator('.catalog-node > .tree-row .tree-toggle')
        toggle.click()
        expect(page.locator('#catalog-nav-status')).to_contain_text('Temporary')
        toggle.click()
        expect(page.locator('.schema-node')).to_be_visible()

    def pending_disclosure():
        reset()
        page.evaluate('catalogFixture.holdTree = true')
        toggle = page.locator('.catalog-node > .tree-row .tree-toggle')
        toggle.click()
        page.wait_for_function('() => Boolean(catalogFixture.pending.tree)')
        toggle.click()
        toggle.click()
        assert page.evaluate('catalogFixture.treeCalls') == 1, 'pending discovery must not run twice'
        page.evaluate('catalogFixture.pending.tree()')
        expect(page.locator('.schema-node')).to_have_count(1)
        expect(page.locator('.schema-node')).to_be_visible()

    def stale_error():
        reset()
        expand()
        page.evaluate('catalogFixture.holdDetail = true')
        page.get_by_role('button', name='orders', exact=True).click()
        page.wait_for_function('() => Boolean(catalogFixture.pending.detail)')
        page.get_by_role('button', name='customers', exact=True).click()
        expect(page.locator('#catalog-detail h1')).to_contain_text('customers')
        page.evaluate('catalogFixture.pending.detail()')
        expect(page.locator('#catalog-detail h1')).to_contain_text('customers')

    def stale_history():
        reset()
        expand()
        page.get_by_role('button', name='orders', exact=True).click()
        page.get_by_role('button', name='History', exact=True).click()
        page.evaluate('catalogFixture.holdHistory = true')
        page.get_by_role('button', name='Earlier document', exact=True).click()
        page.wait_for_function('() => Boolean(catalogFixture.pending.history)')
        page.get_by_role('button', name='customers', exact=True).click()
        expect(page.locator('#catalog-detail h1')).to_contain_text('customers')
        page.evaluate('catalogFixture.pending.history()')
        expect(page.locator('.detail-tab.active')).to_have_text('Details')
        expect(page.locator('#catalog-detail')).not_to_contain_text('Old orders document')

    def save_navigation():
        reset()
        expand()
        page.get_by_role('button', name='orders', exact=True).click()
        page.get_by_role('button', name='Docs', exact=True).click()
        page.get_by_role('button', name='Edit', exact=True).click()
        page.get_by_role('button', name='Save docs', exact=True).click()
        page.wait_for_function('() => Boolean(catalogFixture.pending.save)')
        page.get_by_role('button', name='customers', exact=True).click()
        expect(page.locator('#catalog-detail h1')).to_contain_text('customers')
        page.evaluate('catalogFixture.pending.save()')
        expect(page.locator('.detail-tab.active')).to_have_text('Details')

    def save_failure():
        reset()
        expand()
        page.get_by_role('button', name='orders', exact=True).click()
        page.get_by_role('button', name='Docs', exact=True).click()
        page.get_by_role('button', name='Edit', exact=True).click()
        page.locator('.docs-editor textarea').fill('Keep this draft')
        page.evaluate('catalogFixture.saveError = true')
        page.get_by_role('button', name='Save docs', exact=True).click()
        expect(page.locator('.docs-editor .status')).to_contain_text('Network unavailable')
        expect(page.locator('.docs-editor textarea')).to_have_value('Keep this draft')

    for check in (disclosure, retry, pending_disclosure, stale_error, stale_history, save_navigation, save_failure):
        try:
            check()
            print('PASS', check.__name__)
        except AssertionError as error:
            failures.append(check.__name__)
            print('FAIL', check.__name__, str(error).splitlines()[0])
    browser.close()
    assert not failures, 'Failed catalog checks: ' + ', '.join(failures)
print('catalog-navigation-browser OK: 7 flows')
