"""Behavior checks called by the real notebook browser fixture."""
from playwright.sync_api import expect

def check(page):
    page.evaluate('autosaveBlocked = true')
    def command(name):
        page.keyboard.press('Control+Shift+P')
        page.get_by_role('dialog',name='Notebook commands').get_by_role('button',name=name,exact=True).click()
    while page.locator('.cell').count()<3:
        page.locator('.add-cell-bottom').click()
    first=page.locator('.cell').first
    first.locator('.editor').fill('SELECT 123')
    first.locator('.editor').focus()
    page.evaluate("document.querySelector('.cell').metadata = {note:'keep'}")
    original=page.locator('.cell').count()
    command('Duplicate selected cells')
    expect(page.locator('.cell')).to_have_count(original+1)
    assert page.locator('.cell').nth(1).evaluate('(c) => c.metadata')=={'note':'keep'}
    command('Undo cell operation')
    expect(page.locator('.cell')).to_have_count(original)
    command('Redo cell operation')
    expect(page.locator('.cell')).to_have_count(original+1)
    # Legacy cell engines survive duplicate and split even without notebook compute.
    page.evaluate("notebookEngine.value=''; nb.cells.push({id:document.querySelector('.cell').dataset.id,engine:'legacy'}); document.querySelector('.cell').legacyEngine='legacy'")
    first.locator('.editor').focus()
    command('Duplicate selected cells')
    assert page.evaluate('cellsFromDom()[1].engine')=='legacy'
    command('Undo cell operation')
    # Native edits survive undo of a structural operation.
    page.locator('.cell').nth(1).locator('.editor').fill('SELECT 456')
    command('Undo cell operation');command('Redo cell operation')
    expect(page.locator('.cell').nth(1).locator('.editor')).to_have_value('SELECT 456')
    # Selection and bulk movement preserve document order.
    first.locator('.cell-select').check()
    page.locator('.cell').nth(1).locator('.cell-select').click(modifiers=['Shift'])
    assert page.locator('.cell.selected').count()==2
    ids=page.locator('.cell').evaluate_all('(cs) => cs.map(c=>c.dataset.id)')
    command('Move selected cells down');command('Undo cell operation')
    assert page.locator('.cell').evaluate_all('(cs) => cs.map(c=>c.dataset.id)')==ids
    command('Delete selected cells');command('Undo cell operation')
    assert page.locator('.cell').evaluate_all('(cs) => cs.map(c=>c.dataset.id)')==ids
    # Literal source search previews before mutation; replacement is undoable.
    command('Find and replace')
    dialog=page.get_by_role('dialog',name='Find and replace')
    dialog.get_by_label('Find literal text').fill('SELECT')
    dialog.get_by_label('Replace with').fill('select')
    dialog.get_by_role('button',name='Preview replacements',exact=True).click()
    expect(dialog.locator('.search-preview')).to_contain_text('SELECT')
    dialog.get_by_role('button',name='Apply replacements',exact=True).click()
    expect(first.locator('.editor')).to_have_value('select 123')
    dialog.get_by_role('button',name='Close',exact=True).click()
    command('Undo cell operation')
    expect(first.locator('.editor')).to_have_value('SELECT 123')
    # Outline ignores fenced headings and follows source without executing it.
    first.locator('.editor').fill('%md\n# Overview\n```\n# Not a section\n```\n## Detail')
    page.locator('.add-cell-bottom').focus()
    command('Notebook outline')
    outline=page.get_by_role('dialog',name='Notebook outline')
    expect(outline.get_by_role('button',name='Overview',exact=True)).to_be_visible()
    expect(outline.get_by_role('button',name='Not a section',exact=True)).to_have_count(0)
    outline.get_by_role('button',name='Detail',exact=True).click()
    # Splitting Markdown preserves the magic; merging is recoverable.
    first.locator('[data-action=md-toggle]').click()
    first.locator('.editor').fill('%md\n# One\n# Two')
    first.locator('.editor').evaluate('(e) => e.setSelectionRange(10,10)')
    command('Split cell at cursor')
    expect(page.locator('.cell').nth(1).locator('.editor')).to_have_value('%md\n# Two')
    assert page.evaluate('cellsFromDom()[1].engine')=='legacy'
    page.evaluate('selectCell(document.querySelector(".cell"))')
    command('Merge with next cell')
    assert '# Two' in first.locator('.editor').input_value()
    command('Undo cell operation'); command('Undo cell operation')
    expect(first.locator('.editor')).to_have_value('%md\n# One\n# Two')
    # Replacement preview becomes invalid if the source changed meanwhile.
    command('Find and replace')
    dialog=page.get_by_role('dialog',name='Find and replace')
    dialog.get_by_label('Find literal text').fill('One')
    dialog.get_by_label('Replace with').fill('Changed')
    dialog.get_by_role('button',name='Preview replacements',exact=True).click()
    page.evaluate("document.querySelector('.cell .editor').value += ' newer'")
    dialog.get_by_role('button',name='Apply replacements',exact=True).click()
    expect(dialog.locator('.search-preview')).to_have_text('Source changed; preview again.')
    dialog.get_by_role('button',name='Close',exact=True).click()
    # Run-all and selected runs share admission; stop never claims in-flight cancellation.
    pending=[]
    page.route('**/api/query',lambda route:pending.append(route))
    page.evaluate("""() => {
      const c=document.querySelectorAll('.cell')[1]; c.querySelector('.editor').value='SELECT 1'; setCellKind(c,'sql'); selectCell(c);
      const catalog=notebookData.querySelector('.catalog-context'), schema=notebookData.querySelector('.query-schema');
      catalog.replaceChildren(new Option('mock','mock')); schema.replaceChildren(new Option('schema','schema'));
      catalog.value='mock'; schema.value='schema'; catalog.disabled=false; schema.disabled=false; legacyContextMixed=false; dirty=false;
      void runSelectedCells();
    }""")
    page.wait_for_timeout(150)
    assert len(pending)==1,page.locator(".cell").nth(1).inner_text()
    page.evaluate('void runAll()')
    page.wait_for_timeout(150)
    assert len(pending)==1,'overlapping Run all dispatched another query'
    page.evaluate('stopNotebookQueue=true')
    pending[0].fulfill(json={'columns':[],'rows':[],'truncated':False})
    page.wait_for_function('!notebookQueue')
    page.unroute('**/api/query')
    # Output export includes only displayed rows, handles quoting and formula text safely.
    page.evaluate("""() => {const c=document.querySelectorAll('.cell')[1];renderResult(c.querySelector('.out'),
      {columns:[{name:'value',type:'text'}],rows:[['a,"b\\nc'],['=SUM(1)'],[null]],truncated:true},4)}""")
    out=page.locator('.cell').nth(1).locator('div.out')
    expect(out).to_contain_text('truncated')
    context=out.evaluate('(out) => cellChatContext(out.closest(".cell"))')
    out.get_by_role('button',name='Collapse output').click()
    assert out.evaluate('(out) => cellChatContext(out.closest(".cell"))')==context
    expect(out.locator('.out-body')).to_be_hidden()
    out.get_by_role('button',name='Expand output').click()
    with page.expect_download() as download:
        out.get_by_role('button',name='Download displayed rows as CSV').click()
    data=open(download.value.path()).read()
    assert '"a,""b\nc"' in data and "'=SUM(1)" in data,data
    out.get_by_role('button',name='Clear displayed output').click()
    expect(out.locator('table')).to_have_count(0)
    # History preview uses a full document and one fixed CAS revision on conflict.
    page.evaluate("dirty=false; saving=false; contentRevision='" + 'a'*40 + "'")
    page.route('**/aster.v1.Aster/ListNotebookRevisions',lambda route:route.fulfill(json={'revisions':['b'*40]}))
    import json
    historical={'id':'sales','title':'Old title','engine':None,'context':None,'cells':[{'id':'old','sql':'SELECT 8','engine':None,'metadata':{'old':True}}]}
    page.route('**/aster.v1.Aster/GetNotebookRevision',lambda route:route.fulfill(json={'notebookJson':json.dumps(historical)}))
    writes=[]
    def conflict(route):
        writes.append((route.request.headers,route.request.post_data_json))
        route.fulfill(status=409,json={'error':'conflict'})
    page.route('**/aster.v1.Aster/RestoreNotebookRevision',conflict)
    command('Revision history')
    history=page.get_by_role('dialog',name='Notebook revision history')
    expect(history.locator('.revision-preview')).to_contain_text('Old title')
    history.get_by_role('button',name='Restore as new revision',exact=True).click()
    expect(history.get_by_role('status')).to_contain_text('Restore conflict')
    expect(history.get_by_role('button',name='Restore as new revision',exact=True)).to_be_disabled()
    assert len(writes)==1 and writes[0][1]['expectedContentRevision']=='a'*40
    assert writes[0][1]['revision']=='b'*40
    assert page.evaluate('contentRevision')=='a'*40
    history.get_by_role('button',name='Close',exact=True).click()
    page.unroute('**/aster.v1.Aster/RestoreNotebookRevision')
    page.set_viewport_size({'width':390,'height':760})
    page.keyboard.press('Control+Shift+P')
    expect(page.get_by_role('dialog',name='Notebook commands')).to_be_in_viewport(ratio=1)
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
    page.keyboard.press('Escape')
    page.set_viewport_size({'width':1440,'height':1100})
    print('NOTEBOOK_FEATURES_BROWSER_OK')
