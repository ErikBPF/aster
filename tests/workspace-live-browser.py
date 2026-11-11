"""Disposable real-router browser flow; identity/query/hosting are fixture adapters."""
import os
import re
import json
import time
from pathlib import Path
from playwright.sync_api import sync_playwright, expect, Error

origin = os.environ['ASTER_BROWSER_ORIGIN']
team = os.environ['ASTER_BROWSER_TEAM']
interactive = os.environ.get('ASTER_WORKSPACE_DEMO') == '1'
with sync_playwright() as pw:
    browser = pw.chromium.launch(headless=not interactive, executable_path=os.environ.get('ASTER_BROWSER_EXECUTABLE'))
    context = browser.new_context()
    context.add_cookies([{'name':'aster_session', 'value':os.environ['ASTER_BROWSER_SESSION'], 'url':origin}])
    page = context.new_page()
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.goto(origin + '/workspaces')
    page.get_by_label('Workspace name').fill('Browser research')
    page.get_by_role('button', name='Create workspace', exact=True).click()
    expect(page.locator('#workspace-dashboard')).to_be_visible()
    dashboard = page.url
    for repository in ['101', '102']:
        page.locator('#workspace-team').fill(team)
        page.get_by_role('button', name='Load repositories').click()
        expect(page.locator('#workspace-repository option')).to_have_count(2)
        page.locator('#workspace-repository').select_option(repository)
        page.get_by_role('button', name='Clone repository', exact=True).click()
        expect(page.locator('#workspace-checkouts > section')).to_have_count(1 if repository == '101' else 2, timeout=30000)
    card = page.locator('#workspace-checkouts > section').filter(has=page.get_by_role('heading', name='acme/rpc', exact=True))
    card.get_by_label('New notebook ID').fill('browser-analysis')
    card.get_by_role('button', name='Create notebook').click()
    expect(page.locator('.editor')).to_be_editable(timeout=30000)
    notebook_url = page.url
    expect(page.get_by_label('Commit message')).not_to_be_visible()
    expect(page.locator('.nb-bar input:visible')).to_have_count(0)
    page.get_by_role('button', name=re.compile('^Data:')).click()
    page.locator('#notebook-data .query-schema').select_option('default')
    page.get_by_role('button', name=re.compile('^Data:')).click()
    page.locator('.editor').fill('SELECT 42')
    expect(page.locator('#status')).to_contain_text('saved draft', timeout=30000)
    if interactive:
        page.screenshot(path='/tmp/aster-current-demo.png', full_page=True)
        receipt = Path('.devenv/workspace-demo.json')
        receipt.parent.mkdir(exist_ok=True)
        receipt.write_text(json.dumps({'url': page.url, 'team': team, 'mode': 'disposable fixtures'}))
        try:
            deadline = time.monotonic() + 7200
            while browser.is_connected() and context.pages and time.monotonic() < deadline:
                context.pages[0].wait_for_timeout(1000)
        except Error:
            if browser.is_connected() and context.pages:
                raise
        finally:
            receipt.unlink(missing_ok=True)
            browser.close()
        print('Workspace demo closed; disposable data will be removed.')
        raise SystemExit(0)
    page.locator('#workspace-git-panel > summary').click()
    page.get_by_label('Commit message').fill('Browser analysis')
    page.get_by_role('button', name='Commit', exact=True).click()
    expect(page.locator('#status')).to_contain_text('committed locally', timeout=30000)
    page.get_by_role('button', name='Push', exact=True).click()
    expect(page.locator('#status')).to_contain_text('pushed', timeout=30000)
    page.get_by_label('PR title').fill('Browser analysis')
    page.get_by_role('button', name='Open PR', exact=True).click()
    expect(page.get_by_role('link', name='Open pull request')).to_have_attribute('href', re.compile(r'^https://github.com/acme/rpc/pull/'))
    page.locator('[data-action="run"]').click()
    expect(page.locator('.out-content')).to_contain_text('SELECT 42', timeout=30000)
    with page.expect_download() as download:
        page.get_by_role('button', name='Export checkout', exact=True).click()
    assert download.value.suggested_filename.endswith('.tar')
    page.reload()
    expect(page.locator('.editor')).to_have_value('SELECT 42')
    expect(page.locator('#notebook-data .query-schema')).to_have_value('default')
    expect(page.locator('.out-content')).to_be_empty()
    expect(page.locator('#workspace-open-tabs')).to_contain_text('browser-analysis')
    page.goto(origin + '/catalog')
    expect(page.locator('#catalog-page')).to_be_visible()
    page.locator('.catalog-node > .tree-row .tree-toggle').click()
    page.locator('.schema-node > .tree-row .tree-toggle').click()
    expect(page.locator('.table-node')).to_contain_text('sample_orders')
    page.locator('.table-node .tree-row').click()
    expect(page.locator('#catalog-detail')).to_contain_text('order_id')
    page.screenshot(path='/tmp/aster-current-catalog.png', full_page=True)
    page.goto(notebook_url)
    second = browser.new_context()
    second.add_cookies(context.cookies())
    other = second.new_page()
    other.goto(notebook_url)
    expect(other.locator('.editor')).not_to_be_editable()
    other.evaluate("addCell(document.querySelector('.cell')); editSelectedCells('copy'); splitCell();")
    expect(other.locator('.editor')).to_have_count(1)
    expect(other.locator('#notebook-engine')).to_be_disabled()
    expect(other.locator('#notebook-data .catalog-context')).to_be_disabled()
    other.locator('#workspace-git-panel > summary').click()
    other.get_by_role('button', name='Take over', exact=True).click()
    expect(other.locator('.editor')).to_be_editable(timeout=30000)
    page.locator('.editor').fill('SELECT stale')
    expect(page.locator('#status')).to_contain_text('read-only', timeout=30000)
    other.locator('.editor').fill('SELECT recovered')
    expect(other.locator('#status')).to_contain_text('saved draft', timeout=30000)
    if other.locator('#workspace-git-panel[open]').count():
        other.locator('#workspace-git-panel > summary').click()
    other.screenshot(path='/tmp/aster-current-notebook.png', full_page=True)
    other.goto(dashboard)
    expect(other.locator('#workspace-quota')).to_contain_text('2 / 8 repositories')
    expect(other.locator('#workspace-tabs')).to_contain_text('browser-analysis')
    other.on('dialog', lambda dialog: dialog.accept())
    other.get_by_role('button', name='Delete workspace', exact=True).click()
    expect(other).to_have_url(origin + '/workspaces', timeout=30000)
    expect(other.get_by_role('link', name='Browser research', exact=True)).to_have_count(0)
    assert not errors, errors
    browser.close()
print('workspace-live-browser OK: two repositories, drafts, Git, query, takeover, tabs, deletion')
