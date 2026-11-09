"""Transfer-only browser regressions; replies are controlled, query calls forbidden."""
from playwright.sync_api import expect


def check(page):
    page.evaluate('clearTimeout(autosaveTimer); autosaveBlocked = true')
    queries = []
    def refuse(route):
        queries.append(route.request.post_data)
        route.abort()
    page.route('**/api/query', refuse)
    page.evaluate('''() => {
      cellsRoot.replaceChildren();
      addCell(null).querySelector('.editor').value = 'SELECT original';
      addCell(null).querySelector('.editor').value = 'SELECT untouched';
      currentCell = notebookCells()[0];
      conversation = {id:'transfer-fixture',revision:'1',messages:[{role:'assistant',content:'First\\n```sql\\nSELECT 11;\\n```\\nSecond\\n```SQL\\nSELECT 22;\\n```'}]};
      chatPanel.hidden = false; document.body.classList.add('chat-open'); renderConversation();
    }''')
    actions = page.locator('#chat-history .suggestion-actions')
    expect(actions).to_have_count(2)
    first, second = actions.nth(0), actions.nth(1)
    expect(first).to_contain_text('Query 1')
    expect(second).to_contain_text('SELECT 22;')
    original = page.locator('.cell').first
    first.get_by_role('button', name='Replace cell SQL', exact=True).click()
    expect(original.locator('.editor')).to_have_value('SELECT 11;')
    page.evaluate('undoCellOperation()')
    expect(original.locator('.editor')).to_have_value('SELECT original')
    page.evaluate('undoCellOperation(true)')
    expect(original.locator('.editor')).to_have_value('SELECT 11;')
    # The displayed destination survives focus changes and reordering.
    page.locator('.cell').nth(1).locator('.editor').focus()
    second.get_by_role('button', name='Replace cell SQL', exact=True).click()
    expect(original.locator('.editor')).to_have_value('SELECT 22;')
    expect(page.locator('.cell').nth(1).locator('.editor')).to_have_value('SELECT untouched')
    page.evaluate('cellsRoot.appendChild(notebookCells()[0])')
    first.get_by_role('button', name='Replace cell SQL', exact=True).click()
    expect(first).to_contain_text('Destination changed')
    expect(original.locator('.editor')).to_have_value('SELECT untouched')
    expect(page.locator('.cell').last.locator('.editor')).to_have_value('SELECT 22;')
    first.get_by_role('button', name='Replace cell SQL', exact=True).click()
    expect(page.locator('.cell').last.locator('.editor')).to_have_value('SELECT 11;')
    page.evaluate('cellsRoot.prepend(notebookCells().at(-1)); renderConversation()')
    # Deleted targets never silently replace another cell or create one.
    page.evaluate('notebookCells()[0].remove()')
    before = page.locator('.cell').count()
    first.get_by_role('button', name='Replace cell SQL', exact=True).click()
    expect(first).to_contain_text('Destination was removed')
    assert page.locator('.cell').count() == before
    expect(page.locator('.cell').first.locator('.editor')).to_have_value('SELECT untouched')
    first.get_by_role('combobox').select_option('new')
    first.get_by_role('button', name='Create SQL cell', exact=True).click()
    expect(page.locator('.cell')).to_have_count(before+1)
    expect(page.locator('.cell').last.locator('.editor')).to_have_value('SELECT 11;')
    page.evaluate('undoCellOperation()')
    expect(page.locator('.cell')).to_have_count(before)
    page.evaluate('undoCellOperation(true)')
    expect(page.locator('.cell').last.locator('.editor')).to_have_value('SELECT 11;')
    # Markdown content survives transfer in both notebook and cell assistants.
    for scope in ['notebook', 'cell']:
        page.evaluate('''scope => {
          const cell = notebookCells()[0];
          cell.querySelector('.editor').value = '%md\\n# Keep notes'; setCellKind(cell,'markdown');
          currentCell = cell;
          if (scope === 'notebook') renderConversation();
          else {
            cellChatState(cell.dataset.id).conversation = conversation;
            cell.querySelector('.cell-chat').hidden = false; renderCellChat(cell);
          }
        }''', scope)
        group = page.locator('#chat-history .suggestion-actions' if scope=='notebook' else '.cell-chat-history .suggestion-actions').first
        before = page.locator('.cell').count()
        group.get_by_role('button', name='Create SQL cell', exact=True).click()
        expect(page.locator('.cell')).to_have_count(before+1)
        expect(page.locator('.cell').first.locator('.editor')).to_have_value('%md\n# Keep notes')
        expect(page.locator('.cell').nth(1)).to_have_attribute('data-kind','sql')
        expect(page.locator('.cell').nth(1).locator('.editor')).to_have_value('SELECT 11;')
        page.evaluate('undoCellOperation()')
        expect(page.locator('.cell')).to_have_count(before)
    # A previously offered create action must not turn into replacement silently.
    page.evaluate("setCellKind(notebookCells()[0], 'sql'); notebookCells()[0].querySelector('.editor').value = 'SELECT keep';")
    group.get_by_role('button', name='Create SQL cell', exact=True).click()
    expect(group).to_contain_text('Destination changed')
    expect(page.locator('.cell').first.locator('.editor')).to_have_value('SELECT keep')
    # Cell assistant replacements use the same undo path.
    group.get_by_role('button', name='Replace cell SQL', exact=True).click()
    expect(page.locator('.cell').first.locator('.editor')).to_have_value('SELECT 11;')
    page.evaluate('undoCellOperation()')
    expect(page.locator('.cell').first.locator('.editor')).to_have_value('SELECT keep')
    assert not queries, queries
    page.unroute('**/api/query', refuse)
    print('ASSISTANT_TRANSFER_BROWSER_OK')
