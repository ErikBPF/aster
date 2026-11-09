"""Selection/navigation behaviors against the real notebook shell."""
from playwright.sync_api import expect

def check(page):
    page.evaluate('clearTimeout(autosaveTimer); autosaveBlocked=true')
    while page.locator('.cell').count()<3:
        page.locator('.add-cell-bottom').click()
    cells=page.locator('.cell')
    first=cells.first
    first.locator('.editor').fill('SELECT 10')
    first.locator('.editor').focus()
    page.keyboard.press('Escape')
    expect(first).to_be_focused()
    expect(page.locator('#notebook-mode')).to_contain_text('Cell navigation')
    page.keyboard.press('Shift+ArrowDown')
    expect(page.locator('.cell.selected')).to_have_count(2)
    toolbar=page.get_by_role('toolbar',name='Selected cells')
    expect(toolbar).to_be_visible()
    expect(toolbar).to_contain_text('2 cells selected')
    ids=cells.evaluate_all('(cs)=>cs.map(c=>c.dataset.id)')
    toolbar.get_by_role('button',name='Move selected cells down',exact=True).click()
    assert cells.evaluate_all('(cs)=>cs.map(c=>c.dataset.id)')[1:3]==ids[:2]
    page.locator('.cell.selected').first.focus()
    page.keyboard.press('z')
    assert cells.evaluate_all('(cs)=>cs.map(c=>c.dataset.id)')==ids
    page.keyboard.press('Control+Shift+ArrowDown')
    assert cells.evaluate_all('(cs)=>cs.map(c=>c.dataset.id)')[1:3]==ids[:2]
    page.keyboard.press('z')
    toolbar.get_by_role('button',name='Duplicate selected cells',exact=True).click()
    expect(cells).to_have_count(len(ids)+2)
    page.locator('.cell.selected').first.focus();page.keyboard.press('z')
    expect(cells).to_have_count(len(ids))
    toolbar.get_by_role('button',name='Clear selection',exact=True).click()
    expect(toolbar).to_be_hidden()
    first.focus();page.keyboard.press('Enter')
    expect(first.locator('.editor')).to_be_focused()
    expect(page.locator('#notebook-mode')).to_contain_text('Editing')
    count=cells.count()
    page.keyboard.type('abz?')
    expect(cells).to_have_count(count)
    expect(page.get_by_role('dialog')).to_have_count(0)
    # Completion consumes the first Escape; the next switches to navigation.
    page.evaluate("showComplete(document.querySelector('.cell .editor'), [{label:'SELECT',insert:'SELECT'}], 0)")
    page.keyboard.press('Escape')
    expect(first.locator('.editor')).to_be_focused()
    page.keyboard.press('Escape');expect(first).to_be_focused()
    page.keyboard.press('b');expect(cells).to_have_count(count+1)
    expect(cells.nth(1)).to_have_attribute('data-kind','sql')
    page.keyboard.press('Escape');page.keyboard.press('z');expect(cells).to_have_count(count)
    page.keyboard.press('a');expect(cells).to_have_count(count+1)
    expect(cells.first.locator('.editor')).to_have_value('')
    page.keyboard.press('Escape');page.keyboard.press('z')
    page.keyboard.press('?')
    help=page.get_by_role('dialog',name='Keyboard shortcuts')
    expect(help).to_contain_text('Shift+↑/↓')
    page.keyboard.press('Escape')
    expect(page.get_by_role('dialog')).to_have_count(0)
    expect(page.get_by_role('button',name='Notebook commands',exact=True)).to_have_count(0)
    page.get_by_role('button',name='Notebook actions',exact=True).click()
    page.get_by_role('button',name='Find action',exact=True).click()
    palette=page.get_by_role('dialog',name='Notebook commands')
    expect(palette).to_be_visible()
    palette.get_by_label('Search commands').fill('abz')
    expect(cells).to_have_count(count)
    page.keyboard.press('Escape')
    expect(palette).to_have_count(0)
    # Repeated insert keys do not create a stream of cells.
    first.focus()
    page.dispatch_event('.cell','keydown',{'key':'b','repeat':True,'bubbles':True})
    expect(cells).to_have_count(count)
    # Keyboard execution targets the selected range, in document order.
    requests=[]
    def result(route):
        requests.append(route.request.post_data_json['cell'])
        route.fulfill(json={'columns':[],'rows':[],'truncated':False})
    page.route('**/api/query',result)
    page.evaluate("""() => {
      for (const c of notebookCells()) {c.querySelector('.editor').value='SELECT 1';setCellKind(c,'sql');}
      const catalog=notebookData.querySelector('.catalog-context'), schema=notebookData.querySelector('.query-schema');
      catalog.replaceChildren(new Option('mock','mock'));schema.replaceChildren(new Option('schema','schema'));
      catalog.value='mock';schema.value='schema';catalog.disabled=false;schema.disabled=false;legacyContextMixed=false;dirty=false;
    }""")
    first.focus();expect(first).to_be_focused();page.keyboard.press('ArrowDown');page.keyboard.press('ArrowUp')
    page.keyboard.press('Shift+ArrowDown');page.keyboard.press('Control+Enter')
    page.wait_for_function('!notebookQueue')
    assert requests==ids[:2],requests
    requests.clear()
    page.keyboard.press('Shift+Enter')
    expect(cells.nth(2)).to_be_focused()
    assert requests==ids[:2],requests
    page.unroute('**/api/query')
    page.set_viewport_size({'width':390,'height':760})
    page.keyboard.press('Shift+ArrowUp')
    expect(toolbar).to_be_visible()
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
    page.set_viewport_size({'width':1440,'height':1100})
    # Finishing a queued run must not steal focus from a newer editing intent.
    held=[]
    page.route('**/api/query',lambda route:held.append(route))
    first.focus();expect(first).to_be_focused();page.keyboard.press('ArrowDown');page.keyboard.press('ArrowUp')
    page.keyboard.press('Shift+Enter')
    page.wait_for_timeout(100)
    assert len(held)==1
    later=cells.nth(2).locator('.editor')
    later.focus()
    held[0].fulfill(json={'columns':[],'rows':[],'truncated':False})
    page.wait_for_function('!notebookQueue')
    expect(later).to_be_focused()
    expect(page.locator('#notebook-mode')).to_contain_text('Editing')
    page.unroute('**/api/query')
    # A late completion response cannot reopen suggestions in navigation mode.
    completions=[]
    page.route('**/api/sql/complete?*',lambda route:completions.append(route))
    later.fill('SEL')
    page.evaluate('clearTimeout(completeTimer); void requestComplete(document.activeElement)')
    page.wait_for_timeout(100)
    assert completions
    page.keyboard.press('Escape')
    expect(cells.nth(2)).to_be_focused()
    for route in completions:
        route.fulfill(json=[{'label':'SELECT','kind':'keyword','insert':'SELECT'}])
    page.wait_for_timeout(100)
    assert not page.evaluate('completeOpen()'),'stale completion reopened after navigation'
    page.unroute('**/api/sql/complete?*')
    print('NOTEBOOK_KEYBOARD_BROWSER_OK')
