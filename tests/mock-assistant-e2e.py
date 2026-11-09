"""Explicit localhost demo: creates a notebook, skill and model; executes synthetic SQL."""
import json
import os
import re
import time
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

base = 'http://localhost:8081'
out = Path('/tmp/aster-mock-assistant-e2e')
out.mkdir(exist_ok=True)
with sync_playwright() as pw:
    browser = pw.chromium.launch(headless=True, executable_path=os.environ['ASTER_BROWSER_EXECUTABLE'])
    page = browser.new_page(viewport={'width': 1600, 'height': 1100})
    errors, queries = [], []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.on('request', lambda req: queries.append(req.post_data_json) if req.url == base+'/api/query' else None)
    page.goto(base+'/dev-login?subject=alice&roles=admin')
    def rpc(method, body):
        r = page.request.post(base+'/aster.v1.Aster/'+method, data=body)
        assert r.ok, (method, r.status, r.text())
        return r.json()
    policy = rpc('GetPersonalModelPolicy', {})
    if not policy.get('canRegister'):
        rpc('PutPersonalModelPolicy', {'expectedRevision': policy.get('revision', '0'), 'subjects': sorted(set(policy.get('subjects', [])+['alice'])), 'teams': policy.get('teams', [])})
    rpc('PutPersonalModel', {'id': 'mock-assistant', 'baseUrl': 'http://127.0.0.1:7778/v1', 'model': 'aster-mock-assistant', 'token': ''})
    title = 'Demo orders SQL'
    existing = [s for s in rpc('ListSkills', {}).get('skills', []) if s['title'] == title]
    if existing:
        skill = rpc('GetSkill', {'id': existing[0]['id'], 'revision': '1'})
        assert skill['instructions'].startswith('ASTER_DEMO_READ_ONLY_V1')
    else:
        skill = rpc('PutSkill', {'title': title, 'expectedRevision': '0', 'instructions': 'ASTER_DEMO_READ_ONLY_V1\nGenerate one read-only SELECT with explicit columns and LIMIT 10. Use the attached orders reference. Explain that output is synthetic and require review before execution.', 'references': [{'name': 'references/orders.md', 'content': 'ASTER_DEMO_ORDERS_V1\nTable: "mock-local".aster_demo.orders\nColumns: order_id bigint, customer varchar, region varchar, total decimal(12,2), placed_at timestamp. Synthetic demo data only.'}]})
    notebook = 'mock-assistant-demo-'+str(int(time.time()))
    r = page.request.put(base+'/api/notebooks/'+notebook, headers={'if-none-match': '*'}, data={'id': notebook, 'title': 'Mock assistant — skill to SQL', 'engine': 'mock-local', 'context': {'catalog_context': 'mock-local', 'schema': 'aster_demo'}, 'cells': [{'id': 'c1', 'sql': ''}]})
    assert r.ok, (r.status, r.text())
    page.goto(base+'/notebooks/'+notebook)
    page.evaluate('clearTimeout(autosaveTimer); autosaveBlocked = true')
    page.locator('#chat-toggle').click()
    chat = page.locator('#chat-panel')
    chat.get_by_role('button', name=re.compile(r'^AI model:')).click()
    page.locator('#helper').select_option('personal/mock-assistant')
    page.keyboard.press('Escape')
    chat.get_by_role('button', name='Start a new session', exact=True).click()
    chat.get_by_role('button', name=re.compile(r'^Skills \(')).click()
    for box in chat.get_by_role('checkbox', name=re.compile(r'^Use ')).all():
        box.uncheck()
    page.keyboard.press('Escape')
    prompt = 'Generate a query listing the demo orders with customer, region, total and placement date.'
    page.locator('#chat-prompt').fill(prompt)
    page.locator('#chat-send').click()
    expect(chat.locator('.assistant').last).to_contain_text('enable the Demo orders SQL skill', timeout=15000)
    assert not queries, 'generation must not execute SQL'
    chat.get_by_role('button', name=re.compile(r'^Skills \(')).click()
    chat.get_by_role('checkbox', name='Use '+title, exact=True).check()
    chat.get_by_role('button', name='Versions for '+title, exact=True).click()
    chat.get_by_role('combobox', name='Version for '+title, exact=True).select_option('1')
    page.screenshot(path=str(out/'01-skill-selected.png'))
    page.keyboard.press('Escape')
    page.locator('#chat-prompt').fill(prompt)
    page.locator('#chat-send').click()
    reply = chat.locator('.assistant').last
    expect(reply).to_contain_text('received the read-only skill and orders reference (version 1)', timeout=15000)
    expect(reply).to_contain_text('LIMIT 10')
    assert not queries, 'assistant reply executed SQL automatically'
    page.screenshot(path=str(out/'02-generated-query.png'))
    reply.get_by_role('combobox', name='Destination for query 1', exact=True).select_option('c1')
    reply.get_by_role('button', name='Replace cell SQL', exact=True).click()
    cell = page.locator('.cell').first
    sql = cell.locator('.editor').input_value()
    assert 'FROM "mock-local".aster_demo.orders' in sql
    assert not queries, 'insertion executed SQL automatically'
    page.locator('#chat-close').click()
    with page.expect_response(base+'/api/query') as executed:
        cell.locator('[data-action=run]').click()
    response = executed.value
    assert response.ok, (response.status, response.text())
    result = response.json()
    assert len(queries) == 1 and queries[0]['sql'] == sql
    assert queries[0]['engine'] == 'mock-local'
    assert result['rows'] and 'Ada Lovelace' in json.dumps(result)
    expect(cell.locator('.out-content')).to_contain_text('Ada Lovelace')
    with page.expect_response(lambda r: r.url == base+'/api/notebooks/'+notebook and r.request.method == 'PUT') as saved:
        page.get_by_role('button', name='Save', exact=True).click()
    assert saved.value.ok, 'demo notebook save failed'
    persisted = page.request.get(base+'/api/notebooks/'+notebook)
    assert persisted.ok and persisted.json()['cells'][0]['sql'] == sql
    page.screenshot(path=str(out/'03-engine-result.png'), full_page=True)
    session = rpc('ListConversations', {'notebook': notebook})['sessions'][0]
    history = rpc('GetConversation', {'notebook': notebook, 'session': session['id']})
    assert history['messages'][-1]['skills'] == [{'id': skill['id'], 'revision': '1'}], 'exact version pin missing from conversation'
    assert not errors, errors
    receipt = {'result': 'MOCK_ASSISTANT_E2E_OK', 'notebook': base+'/notebooks/'+notebook, 'skill': skill['id'], 'skill_revision': '1', 'model': 'personal/mock-assistant', 'query': sql, 'query_requests': len(queries), 'rows': len(result['rows']), 'javascript_errors': errors, 'limits': 'Deterministic model and synthetic mock-engine rows; no SQL semantics or real inference proved.'}
    (out/'receipt.json').write_text(json.dumps(receipt, indent=2))
    print(json.dumps(receipt))
    browser.close()
