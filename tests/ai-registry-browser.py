"""AI registry UI against controlled APIs; real app.js, no model calls."""
import os
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
HTML = '<!doctype html><html><body><main id="ai-registry" data-subject="alice"></main></body></html>'
FIXTURE = r'''
window.aiFixture = {admin: false, allowed: false, revision: 1, calls: [], skills: [], versions: {}, models: []};
window.fetch = async (url, options = {}) => {
  const f = aiFixture, body = options.body ? JSON.parse(options.body) : {};
  f.calls.push({url: String(url), body, method: options.method || 'GET'});
  const ok = value => new Response(JSON.stringify(value), {status: 200});
  const fail = () => new Response(JSON.stringify({message: 'Access denied'}), {status: 403});
  const method = String(url).split('/').pop();
  if (method === 'GetPersonalModelPolicy') return ok({revision: String(f.revision), subjects: f.admin ? ['alice'] : [], teams: [], canRegister: f.allowed, isAdmin: f.admin});
  if (method === 'PutPersonalModelPolicy') { f.revision++; return ok({...body, revision: String(f.revision), canRegister: true, isAdmin: true}); }
  if (method === 'ListMcpConnections') return ok({configured:true,canManage:f.admin,connections:f.mcpConnections || []});
  if (method === 'DiscoverMcpTools') return ok({tools:[{name:'draft_note',description:'Simulated write',schemaJson:'{"type":"object"}',schemaHash:'hash',approvedRead:false}]});
  if (method === 'CallMcpTool') return ok(body.confirmation ? {resultJson:'{"content":[{"type":"text","text":"Mock result"}]}'} : {confirmation:'fixture-confirmation'});
  if (method === 'PutMcpConnection') return f.admin ? ok({...body,token:undefined,revision:'1'}) : fail();
  if (method === 'GetSkillDefaults') return ok(f.defaults || {revision:'0',skills:[]});
  if (method === 'PutSkillDefaults') { f.defaults={revision:String(Number(body.expectedRevision)+1),skills:body.skills}; return ok(f.defaults); }
  if (method === 'ListSkills') return ok({skills: f.skills});
  if (method === 'ListSkillVersions') return ok({skills: f.versions[body.id] || []});
  if (method === 'GetSkill') return ok((f.versions[body.id] || []).find(s => String(s.revision) === String(body.revision)) || f.skills.find(s => s.id === body.id));
  if (method === 'PutSkill') {
    const old = f.skills.find(s => s.id === body.id);
    const s = {...body, id: old?.id || 'skill-1', revision: String(Number(old?.revision || 0) + 1), sharingRevision: old?.sharingRevision || '0', owner: 'alice', author: 'alice', audience: old?.audience || 'private', team: old?.team || '', canEdit: true};
    f.skills = [s]; f.versions[s.id] = [s, ...(f.versions[s.id] || [])]; return ok(s);
  }
  if (method === 'DeleteSkill') { f.skills=f.skills.filter(s=>s.id!==body.id); delete f.versions[body.id]; return ok({}); }
  if (method === 'ShareSkill') {
    const s = f.skills.find(s => s.id === body.id);
    Object.assign(s, {audience: body.audience, team: body.team, sharingRevision: String(Number(s.sharingRevision) + 1)}); return ok(s);
  }
  if (method === 'PutPersonalModel') { if (!f.allowed) return fail(); f.models = [{id: body.id, base_url: body.baseUrl, model: body.model, ref: 'personal/' + body.id, scope: 'personal'}]; return ok(f.models[0]); }
  if (method === 'DeletePersonalModel') { f.models = []; return ok({}); }
  if (url === '/api/llm') return ok(f.models);
  if (String(url).endsWith('/grants')) return ok({revision: '3', grants: [{group: 'analytics'}]});
  if (url === '/api/admin/shared-models') return ok([{id: 'company', base_url: 'https://models.example/v1', model: 'sql'}]);
  if (String(url).startsWith('/api/admin/shared-models/')) return ok({});
  return ok({});
};
'''
SKILL = {'id': 'skill-1', 'owner': 'alice', 'title': 'SQL review', 'revision': '1', 'sharingRevision': '0', 'audience': 'private', 'team': '', 'instructions': 'Review SQL', 'references': [], 'author': 'alice', 'canEdit': True}

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=os.environ.get('ASTER_BROWSER_EXECUTABLE'), headless=True)
    page = browser.new_page()
    page.set_default_timeout(3500)
    page.route('http://aster.test/**', lambda route: route.fulfill(body=HTML, content_type='text/html'))
    failures = []

    def reset(admin=False, allowed=False, skill=False):
        page.goto('http://aster.test/settings/llm')
        page.evaluate(FIXTURE)
        page.evaluate('v => Object.assign(aiFixture, v)', {'admin': admin, 'allowed': allowed, 'skills': [SKILL] if skill else [], 'versions': {'skill-1': [SKILL]} if skill else {}})
        page.add_script_tag(content=(ROOT / 'crates/server/assets/app.js').read_text())
        expect(page.get_by_role('heading', name='AI workspace', exact=True)).to_be_visible()

    def skills_versioning():
        reset()
        page.get_by_role('button', name='New skill', exact=True).click()
        page.get_by_label('Skill name', exact=True).fill('<img src=x onerror=alert(1)>')
        page.get_by_label('Instructions', exact=True).fill('Review SQL without executing it.')
        page.get_by_role('button', name='Add reference file', exact=True).click()
        page.locator('.skill-references > fieldset:visible').get_by_label('File name', exact=True).fill('guide.md')
        page.locator('.skill-references > fieldset:visible').get_by_label('File content', exact=True).fill('<script>window.injected = true</script>')
        page.get_by_role('button', name='Save new version', exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 1 saved')
        expect(page.locator('.skill-references > fieldset:visible').get_by_label('File name',exact=True)).to_have_value('guide.md')
        page.get_by_role('button',name='SKILL.md',exact=True).click()
        assert page.locator('#ai-registry img, #ai-registry script').count() == 0
        assert page.evaluate('Boolean(window.injected)') is False
        page.get_by_label('Instructions', exact=True).fill('Updated instructions')
        page.get_by_role('button', name='Save new version', exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 2 saved')
        page.get_by_label('Skill version', exact=True).select_option('1')
        expect(page.get_by_label('Instructions', exact=True)).to_have_value('Review SQL without executing it.')
        expect(page.get_by_role('button', name='Save new version', exact=True)).to_be_disabled()

    def skill_sharing():
        reset(skill=True)
        page.get_by_role('button', name='SQL review', exact=True).click()
        page.locator('#skill-editor').get_by_role('button', name='Permissions', exact=True).click()
        page.get_by_label('Sharing scope', exact=True).select_option('team')
        page.get_by_label('Team ID', exact=True).fill('analytics')
        page.get_by_role('button', name='Update sharing', exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Sharing updated')
        assert page.locator('#skill-audience option[value="organization"]').count() == 0
        call = page.evaluate("aiFixture.calls.findLast(c => c.url.endsWith('/ShareSkill')).body")
        assert call['expectedSharingRevision'] == '0' and call['team'] == 'analytics'

    def shared_skill_read_only():
        reset(skill=True)
        page.evaluate("aiFixture.skills[0].canEdit = false; aiFixture.skills[0].owner = 'bob'")
        page.get_by_role('button', name='SQL review', exact=True).click()
        expect(page.get_by_role('button', name='Save new version', exact=True)).to_be_disabled()
        expect(page.get_by_label('Instructions', exact=True)).to_be_disabled()
        page.locator('#skill-editor').get_by_role('button', name='Permissions', exact=True).click()
        expect(page.get_by_role('button', name='Update sharing', exact=True)).to_be_disabled()

    def personal_permission():
        reset()
        page.get_by_role('tab', name='Models', exact=True).click()
        expect(page.get_by_role('button', name='Save personal model', exact=True)).to_be_disabled()
        expect(page.locator('#personal-model-status')).to_contain_text('administrator')

    def personal_token():
        reset(allowed=True)
        page.get_by_role('tab', name='Models', exact=True).click()
        page.get_by_label('Registration name', exact=True).fill('mine')
        page.get_by_label('Base URL', exact=True).fill('https://models.example/v1')
        page.get_by_label('Provider model ID', exact=True).fill('sql')
        page.get_by_label('Token', exact=True).fill('fixture-only-token')
        page.get_by_role('button', name='Save personal model', exact=True).click()
        expect(page.locator('#personal-model-status')).to_contain_text('saved')
        expect(page.get_by_label('Token', exact=True)).to_have_value('')
        assert 'fixture-only-token' not in page.locator('#ai-registry').inner_text()
        expect(page.locator('#model-list')).to_contain_text('personal/mine')

    def admin_controls():
        reset(admin=True, allowed=True, skill=True)
        page.get_by_role('button', name='SQL review', exact=True).click()
        expect(page.locator('#skill-audience option[value="organization"]')).to_have_count(1)
        page.get_by_role('tab', name='Administration', exact=True).click()
        page.get_by_label('Allowed users', exact=True).fill('alice\nbob')
        page.get_by_label('Allowed teams', exact=True).fill('analytics')
        page.get_by_role('button', name='Save personal-model permissions', exact=True).click()
        expect(page.locator('#model-policy-status')).to_contain_text('saved')
        call = page.evaluate("aiFixture.calls.findLast(c => c.url.endsWith('/PutPersonalModelPolicy')).body")
        assert call == {'expectedRevision': '1', 'subjects': ['alice', 'bob'], 'teams': ['analytics']}
        page.get_by_role('button', name='Manage company', exact=True).click()
        expect(page.get_by_label('Company model groups', exact=True)).to_have_value('analytics')
        page.get_by_label('Company model groups', exact=True).fill('finance')
        page.get_by_role('button', name='Save company-model ACL', exact=True).click()
        call = page.evaluate("aiFixture.calls.findLast(c => c.method === 'PUT' && c.url.endsWith('/grants')).body")
        assert call['expected_revision'] == 3 and call['grants'] == [{'group': 'finance'}]

    def models_deep_link_opens_registration():
        page.goto('http://aster.test/settings/llm#ai-models')
        page.evaluate(FIXTURE)
        page.add_script_tag(content=(ROOT / 'crates/server/assets/app.js').read_text())
        expect(page.get_by_role('tab', name='Models', exact=True)).to_have_attribute('aria-selected', 'true')
        expect(page.get_by_role('heading', name='Personal model', exact=True)).to_be_visible()
        expect(page.get_by_role('heading', name='AI workspace', exact=True)).to_be_visible()

    def compact_composer_controls():
        reset(skill=True)
        page.add_style_tag(content=(ROOT / 'crates/server/assets/app.css').read_text())
        page.set_viewport_size({'width':1440,'height':900})
        page.evaluate("""() => {
          aiFixture.mcpConnections=[{id:'donsetch',name:'Donsetch',enabled:true}];
          aiFixture.defaults={revision:'1',skills:[{id:'skill-1',revision:'1'}]};
          const panel=cellChatTemplate();panel.id='compact-chat';panel.hidden=false;
          panel.className='chat-panel';document.body.append(panel);
          syncSkillPicker(panel,{id:'compact',revision:'0',messages:[]});
          syncNotebookMcp(panel,{id:'compact',revision:'0',messages:[]});
        }""")
        panel=page.locator('#compact-chat')
        expect(panel.get_by_role('button',name='Skills (1)',exact=True)).to_be_visible()
        expect(panel.locator('.ai-skill-choices').first).to_be_hidden()
        prompt=panel.locator('.cell-chat-prompt')
        expect(prompt).to_be_in_viewport(ratio=1)
        expect(panel.locator('.cell-chat-send')).to_be_in_viewport(ratio=1)
        panel.get_by_role('button',name='Skills (1)',exact=True).click()
        search=panel.get_by_role('searchbox',name='Search skills',exact=True)
        expect(search).to_be_focused()
        search.fill('missing')
        expect(panel.get_by_role('checkbox',name='Use SQL review',exact=True)).to_have_count(0)
        search.fill('SQL')
        panel.get_by_role('checkbox',name='Use SQL review',exact=True).uncheck()
        expect(panel.get_by_role('button',name='Skills (0)',exact=True)).to_be_visible()
        page.keyboard.press('Escape')
        expect(panel.get_by_role('button',name='Skills (0)',exact=True)).to_be_focused()
        expect(panel).to_be_visible()
        panel.get_by_role('button',name='Tools (0)',exact=True).click()
        panel.get_by_role('checkbox',name='Enable Donsetch',exact=True).check()
        page.keyboard.press('Escape')
        assert page.evaluate("mcpForTurn(document.getElementById('compact-chat'))") == {'selectedMcp':{'connections':['donsetch']}}
        assert page.evaluate("skillForTurn(document.getElementById('compact-chat'))") == {'selectedSkills':{'skills':[]}}
        panel.get_by_role('button',name='Attach',exact=True).click()
        panel.locator('.cell-chat-context').fill('Notebook context fixture')
        page.keyboard.press('Escape')
        panel.get_by_role('button',name='Remove attached context',exact=True).click()
        expect(panel.locator('.cell-chat-context')).to_have_value('')
        page.set_viewport_size({'width':390,'height':700})
        expect(prompt).to_be_in_viewport(ratio=1)
        expect(panel.locator('.cell-chat-send')).to_be_in_viewport(ratio=1)
        panel.get_by_role('button',name='Skills (0)',exact=True).click()
        popup=panel.locator('[popover]:popover-open')
        expect(popup).to_be_in_viewport(ratio=1)
        page.keyboard.press('Escape')
        page.keyboard.press('Escape')
        page.evaluate("""() => {
          const panel=document.getElementById('compact-chat');
          panel.querySelector('.cell-chat-sessions').textContent='Session one · Session two';
          panel.querySelector('.cell-chat-status').textContent='Resolve the tool turn before sending another message.';
          syncNotebookMcp(panel,{id:'compact',revision:'1',messages:[],pendingTool:{phase:'confirmation',connection:'donsetch',tool:'write_note',argumentsJson:'{}',confirmation:'fixture',expiresAt:String(Math.floor(Date.now()/1000)+60)}});
        }""")
        page.set_viewport_size({'width':390,'height':500})
        expect(prompt).to_be_in_viewport(ratio=1)
        expect(panel.locator('.cell-chat-send')).to_be_in_viewport(ratio=1)
        expect(panel.get_by_role('button',name='Confirm action',exact=True)).to_be_visible()
        page.set_viewport_size({'width':1280,'height':720})

    def defaults_retry_and_cell_send_recovery():
        reset(skill=True)
        page.evaluate("""() => {
          const original=window.fetch;let fail=true;
          window.fetch=(url,options)=>String(url).endsWith('/GetSkillDefaults') && fail ? (fail=false,Promise.resolve(new Response('{}',{status:503}))) : original(url,options);
          const panel=document.createElement('section');panel.id='retry-defaults';document.body.append(panel);
          syncSkillPicker(panel,{id:'same',revision:'0',messages:[]});
        }""")
        expect(page.locator('#retry-defaults')).to_contain_text('Could not load personal defaults')
        page.evaluate("syncSkillPicker(document.getElementById('retry-defaults'),{id:'same',revision:'0',messages:[]})")
        page.wait_for_function("document.getElementById('retry-defaults').skillPicker.defaultsReady")
    def cell_send_recovers_after_pending_turn():
        reset()
        page.evaluate("""() => {
          const cell=document.createElement('section');cell.className='cell';cell.dataset.id='fixture-cell';
          cell.innerHTML='<section class="cell-chat"><div class="cell-chat-history"></div><button class="cell-chat-send" disabled>Send</button></section>';
          document.body.append(cell);
          const state=cellChatState('fixture-cell');state.conversation={id:'cell-session',revision:'2',messages:[]};
          renderCellChat(cell);
        }""")
        expect(page.locator('.cell-chat-send')).to_be_enabled()

    def notebook_mcp_controls_and_confirmation():
        reset()
        page.evaluate("""() => {
          aiFixture.mcpConnections=[{id:'donsetch',name:'Donsetch',enabled:true}];
          const panel=document.createElement('section');panel.id='mcp-chat';document.body.append(panel);
          syncNotebookMcp(panel,{id:'first',revision:'0',messages:[]});
        }""")
        panel=page.locator('#mcp-chat')
        toggle=panel.get_by_role('checkbox',name='Enable Donsetch',exact=True)
        expect(toggle).not_to_be_checked()
        toggle.check()
        assert page.evaluate("mcpForTurn(document.getElementById('mcp-chat'))") == {'selectedMcp':{'connections':['donsetch']}}
        page.evaluate("syncNotebookMcp(document.getElementById('mcp-chat'),{id:'second',revision:'0',messages:[]})")
        expect(toggle).not_to_be_checked()
        page.evaluate("syncNotebookMcp(document.getElementById('mcp-chat'),{id:'second',revision:'1',messages:[],pendingTool:{phase:'confirmation',connection:'donsetch',tool:'draft_note',argumentsJson:'{\"text\":\"hello\"}',confirmation:'secret-confirmation',expiresAt:'9999999999'}})")
        expect(panel.get_by_role('button',name='Confirm action',exact=True)).to_be_visible()
        expect(panel.locator('pre')).to_contain_text('draft_note')
        expect(panel.locator('pre')).to_contain_text('hello')
        assert 'secret-confirmation' not in panel.inner_text()
        expect(toggle).to_be_disabled()

    def personal_defaults_and_notebook_toggles():
        reset(skill=True)
        page.get_by_role('tab',name='My defaults',exact=True).click()
        defaults=page.locator('#ai-defaults')
        defaults.get_by_role('checkbox',name='Use SQL review',exact=True).check()
        defaults.get_by_role('button',name='Save skill defaults',exact=True).click()
        expect(defaults.get_by_role('status').last).to_contain_text('saved')
        page.evaluate("""() => {
          const panel=document.createElement('section'); panel.id='default-chat'; document.body.append(panel);
          syncSkillPicker(panel,{id:'new-session',revision:'0',messages:[]});
        }""")
        panel=page.locator('#default-chat')
        expect(panel.get_by_role('checkbox',name='Use SQL review',exact=True)).to_be_checked()
        panel.get_by_role('checkbox',name='Use SQL review',exact=True).uncheck()
        assert page.evaluate("skillForTurn(document.getElementById('default-chat')).selectedSkills.skills") == []
        panel.get_by_role('button',name='Refresh skills',exact=True).click()
        expect(panel.get_by_role('checkbox',name='Use SQL review',exact=True)).not_to_be_checked()
        page.evaluate("syncSkillPicker(document.getElementById('default-chat'),{id:'old-session',revision:'2',messages:[{role:'user',skills:[]}]})")
        expect(panel.get_by_role('checkbox',name='Use SQL review',exact=True)).not_to_be_checked()

    def conversation_pins():
        reset(skill=True)
        page.evaluate("""() => {
          const panel = document.createElement('section'); panel.id = 'pin-fixture';
          panel.innerHTML = '<form><textarea></textarea></form>'; document.body.append(panel);
          syncSkillPicker(panel, {id:'session-1',revision:'1',messages:[{role:'user',skills:[{id:'skill-1',revision:'1'}]}]});
        }""")
        page.locator('#pin-fixture').get_by_role('button',name='Skills (1)',exact=True).click()
        picker = page.locator('#pin-fixture').get_by_label('Version for SQL review', exact=True)
        expect(picker).to_have_value('1')
        page.evaluate("""() => {
          const s = {...aiFixture.skills[0], revision:'2'};
          aiFixture.skills=[s]; aiFixture.versions['skill-1'].unshift(s);
        }""")
        page.locator('#pin-fixture').get_by_role('button', name='Refresh skills').click()
        expect(picker.locator('option[value="2"]')).to_have_count(1)
        expect(picker).to_have_value('1')
        picker.select_option('2')
        assert page.evaluate("skillForTurn(document.getElementById('pin-fixture'))") == {'selectedSkills': {'skills':[{'id':'skill-1','revision':'2'}]}}
        page.locator('#pin-fixture').get_by_role('checkbox', name='Use SQL review',exact=True).uncheck()
        assert page.evaluate("skillForTurn(document.getElementById('pin-fixture'))") == {'selectedSkills': {'skills':[]}}
        page.evaluate("syncSkillPicker(document.getElementById('pin-fixture'), {id:'session-2',messages:[]})")
        expect(page.locator('#pin-fixture').get_by_role('checkbox',name='Use SQL review',exact=True)).not_to_be_checked()

    def picker_loads_history_on_demand():
        reset(skill=True)
        page.evaluate("""() => {
          aiFixture.calls = [];
          aiFixture.skills = Array.from({length:20}, (_,i) => ({...aiFixture.skills[0], id:'skill-'+(i+1), title:'Skill '+(i+1), revision:'2'}));
          aiFixture.versions['skill-1'] = [{...aiFixture.skills[0], revision:'1'},aiFixture.skills[0]];
          const panel = document.createElement('section'); panel.id='lazy-pin'; document.body.append(panel);
          syncSkillPicker(panel, {id:'first',revision:'1',messages:[]});
        }""")
        panel = page.locator('#lazy-pin')
        picker = panel.get_by_label('Version for Skill 1', exact=True)
        expect(panel.get_by_role('checkbox')).to_have_count(20)
        panel.get_by_role('checkbox',name='Use Skill 1',exact=True).check()
        assert page.evaluate("aiFixture.calls.filter(c => c.url.endsWith('/ListSkillVersions')).length") == 0
        picker.select_option('2')
        panel.get_by_role('button', name='Versions for Skill 1',exact=True).click()
        expect(picker.locator('option[value="1"]')).to_have_count(1)
        expect(picker).to_have_value('2')
        assert page.evaluate("aiFixture.calls.filter(c => c.url.endsWith('/ListSkillVersions')).length") == 1
        page.evaluate("""() => {
          const original = window.fetch;
          window.fetch = (url, options) => String(url).endsWith('/ListSkillVersions') ? new Promise(resolve => {window.finishHistory = () => original(url,options).then(resolve)}) : original(url,options);
        }""")
        panel.get_by_role('button', name='Versions for Skill 1',exact=True).click()
        page.evaluate("""() => {
          syncSkillPicker(document.getElementById('lazy-pin'), {id:'second',revision:'1',messages:[{role:'user',skills:[{id:'skill-2',revision:'2'}]}]});
          finishHistory();
        }""")
        expect(panel.get_by_label('Version for Skill 2',exact=True)).to_have_value('2')
        expect(panel.get_by_role('button', name='Versions for Skill 1',exact=True)).to_be_enabled()
        assert page.evaluate("skillForTurn(document.getElementById('lazy-pin')).selectedSkills.skills[0].id") == 'skill-2'

    def new_skill_discards_pending_read():
        reset(skill=True)
        page.evaluate("""() => {
          const original = window.fetch;
          window.pendingSkillReads = [];
          window.fetch = (url, options) => ['GetSkill', 'ListSkillVersions'].includes(String(url).split('/').pop()) ? new Promise(resolve => pendingSkillReads.push(() => original(url,options).then(resolve))) : original(url,options);
        }""")
        page.get_by_role('button', name='SQL review', exact=True).click()
        page.wait_for_function('pendingSkillReads.length === 2')
        page.get_by_role('button', name='New skill', exact=True).click()
        page.get_by_label('Skill name', exact=True).fill('New draft')
        page.get_by_label('Instructions', exact=True).fill('Keep my draft instructions')
        page.evaluate('async () => { await Promise.all(pendingSkillReads.map(release => release())); await new Promise(requestAnimationFrame); }')
        expect(page.get_by_label('Skill name', exact=True)).to_have_value('New draft')
        expect(page.get_by_label('Instructions', exact=True)).to_have_value('Keep my draft instructions')

    def skill_library_search_and_scope():
        reset(skill=True)
        page.get_by_label('Search skills', exact=True).fill('missing')
        expect(page.locator('#skill-list')).not_to_contain_text('SQL review')
        expect(page.locator('#skill-list-status')).to_contain_text('No matching skills')
        page.get_by_label('Search skills', exact=True).fill('SQL')
        expect(page.locator('#skill-list')).to_contain_text('SQL review')
        page.get_by_label('Filter sharing scope', exact=True).select_option('team')
        expect(page.locator('#skill-list')).not_to_contain_text('SQL review')

    def skill_import_is_private_unsaved_draft():
        reset(skill=True)
        page.locator('#skill-import').set_input_files({'name':'review.json','mimeType':'application/json','buffer': b'{"name":"Imported review","description":"Imported checklist","owner":"mallory","audience":"organization","files":[{"path":"SKILL.md","content":"Review joins"},{"path":"references/check.md","content":"Check nulls"}]}'})
        expect(page.get_by_label('Skill name', exact=True)).to_have_value('Imported review')
        expect(page.get_by_label('Skill description', exact=True)).to_have_value('Imported checklist')
        assert page.evaluate("aiFixture.calls.filter(c => c.url.endsWith('/PutSkill')).length") == 0
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 1 saved')
        expect(page.get_by_label('Instructions',exact=True)).to_have_value('Review joins')
        page.get_by_role('button',name='SKILL.md',exact=True).click()
        saved=page.evaluate("aiFixture.calls.findLast(c => c.url.endsWith('/PutSkill')).body")
        assert saved['id'] == '' and saved['expectedRevision'] == '0'
        assert 'owner' not in saved and 'audience' not in saved
        assert saved['references'][0]['name'] == 'references/check.md'

    def skill_import_rejects_paths_without_losing_draft():
        reset()
        page.get_by_role('button',name='New skill',exact=True).click()
        page.get_by_label('Skill name',exact=True).fill('Keep draft')
        page.locator('#skill-import').set_input_files({'name':'bad.json','mimeType':'application/json','buffer': b'{"name":"Bad","files":[{"path":"SKILL.md","content":"text"},{"path":"../secret","content":"bad"}]}'})
        expect(page.locator('#skill-list-status')).to_contain_text('Invalid')
        expect(page.get_by_label('Skill name',exact=True)).to_have_value('Keep draft')

    def pending_read_keeps_newer_edits():
        reset(skill=True)
        page.get_by_role('button',name='SQL review',exact=True).click()
        expect(page.get_by_label('Instructions',exact=True)).to_have_value('Review SQL')
        page.evaluate("""() => {
          const original=window.fetch; window.finishRead=[];
          window.fetch=(url,options)=>['GetSkill','ListSkillVersions'].includes(String(url).split('/').pop()) ? new Promise(resolve=>finishRead.push(()=>original(url,options).then(resolve))) : original(url,options);
        }""")
        page.get_by_label('Skill version',exact=True).dispatch_event('change')
        page.wait_for_function('finishRead.length === 2')
        page.get_by_label('Instructions',exact=True).fill('Keep this newer edit')
        page.evaluate('async () => {await Promise.all(finishRead.map(release=>release())); await new Promise(requestAnimationFrame)}')
        expect(page.get_by_label('Instructions',exact=True)).to_have_value('Keep this newer edit')

    def hidden_invalid_file_reports_error():
        reset()
        page.get_by_role('button',name='New skill',exact=True).click()
        page.get_by_label('Skill name',exact=True).fill('Review')
        page.get_by_label('Instructions',exact=True).fill('Review SQL')
        page.get_by_role('button',name='Add reference file',exact=True).click()
        page.get_by_role('button',name='SKILL.md',exact=True).click()
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Invalid')
        assert page.evaluate("aiFixture.calls.filter(c=>c.url.endsWith('/PutSkill')).length") == 0

    def historical_export_uses_saved_content():
        import json
        reset(skill=True)
        page.get_by_role('button',name='SQL review',exact=True).click()
        expect(page.get_by_label('Instructions',exact=True)).to_have_value('Review SQL')
        page.get_by_label('Instructions',exact=True).fill('Unsaved changes')
        with page.expect_download() as event:
            page.get_by_role('button',name='Export version',exact=True).click()
        data=json.loads(Path(event.value.path()).read_text())
        assert data['sourceRevision'] == '1'
        assert data['files'][0] == {'path':'SKILL.md','content':'Review SQL'}
        assert 'owner' not in data and 'audience' not in data

    def mcp_admin_form_keeps_tokens_private():
        reset(admin=True)
        page.get_by_role('tab',name='Connections',exact=True).click()
        page.get_by_role('button',name='New connection',exact=True).click()
        page.get_by_label('Connection ID',exact=True).fill('analytics')
        page.get_by_label('Connection name',exact=True).fill('Analytics tools')
        page.get_by_label('MCP endpoint',exact=True).fill('https://tools.example.invalid/mcp')
        page.get_by_label('Connection authentication',exact=True).select_option('bearer')
        page.get_by_label('Connection token',exact=True).fill('fixture-only-mcp-token')
        page.get_by_label('Connection users',exact=True).fill('alice')
        page.get_by_role('button',name='Save connection',exact=True).click()
        expect(page.locator('#mcp-status')).to_contain_text('saved')
        expect(page.get_by_label('Connection token',exact=True)).to_have_value('')
        assert 'fixture-only-mcp-token' not in page.locator('#ai-registry').inner_text()
        call=page.evaluate("aiFixture.calls.findLast(c=>c.url.endsWith('/PutMcpConnection')).body")
        assert call['subjects']==['alice'] and call['expectedRevision']=='0'
        reset()
        page.get_by_role('tab',name='Connections',exact=True).click()
        expect(page.get_by_role('button',name='New connection',exact=True)).to_be_hidden()

    def changing_tool_requires_visible_confirmation():
        reset(admin=True)
        page.evaluate("aiFixture.mcpConnections=[{id:'donsetch',name:'Donsetch mock',enabled:true,revision:'1'}]")
        page.get_by_role('tab',name='Connections',exact=True).click()
        page.get_by_role('button',name='Refresh connections',exact=True).click()
        page.get_by_role('button',name='Tools for Donsetch mock',exact=True).click()
        page.get_by_label('Tool arguments (JSON)',exact=True).fill('{"text":"hello"}')
        page.get_by_role('button',name='Run tool',exact=True).click()
        dialog=page.get_by_role('dialog',name='Confirm tool action',exact=True)
        expect(dialog).to_be_visible()
        expect(dialog).to_contain_text('draft_note')
        expect(dialog).to_contain_text('hello')
        assert page.evaluate("aiFixture.calls.filter(c=>c.url.endsWith('/CallMcpTool')).length") == 1
        dialog.get_by_role('button',name='Cancel',exact=True).click()
        expect(dialog).to_be_hidden()
        assert page.evaluate("aiFixture.calls.filter(c=>c.url.endsWith('/CallMcpTool')).length") == 1
        page.get_by_role('button',name='Run tool',exact=True).click()
        dialog.get_by_role('button',name='Confirm action',exact=True).click()
        expect(page.locator('#mcp-tool-result')).to_contain_text('Mock result')
        calls=page.evaluate("aiFixture.calls.filter(c=>c.url.endsWith('/CallMcpTool')).map(c=>c.body)")
        assert calls[-1]['confirmation']=='fixture-confirmation'
        assert calls[-1]['argumentsJson']==calls[-2]['argumentsJson']

    def mcp_save_keeps_newer_draft():
        reset(admin=True)
        page.get_by_role('tab',name='Connections',exact=True).click()
        page.get_by_role('button',name='New connection',exact=True).click()
        page.get_by_label('Connection ID',exact=True).fill('old')
        page.get_by_label('Connection name',exact=True).fill('Old connection')
        page.get_by_label('MCP endpoint',exact=True).fill('https://tools.example.invalid/mcp')
        page.evaluate("""() => { const original=window.fetch; window.fetch=(url,options)=>String(url).endsWith('/PutMcpConnection') ? new Promise(resolve=>window.finishMcpSave=()=>original(url,options).then(resolve)) : original(url,options); }""")
        page.get_by_role('button',name='Save connection',exact=True).click()
        page.wait_for_function('Boolean(window.finishMcpSave)')
        page.get_by_role('button',name='New connection',exact=True).click()
        page.get_by_label('Connection name',exact=True).fill('Keep new draft')
        page.evaluate('async () => {await finishMcpSave(); await new Promise(requestAnimationFrame)}')
        expect(page.get_by_label('Connection name',exact=True)).to_have_value('Keep new draft')

    def tool_selection_invalidates_pending_confirmation():
        reset(admin=True)
        page.evaluate("""() => {
          aiFixture.mcpConnections=[{id:'donsetch',name:'Donsetch mock',enabled:true,revision:'1'}];
          const original=window.fetch;
          window.fetch=(url,options)=>String(url).endsWith('/CallMcpTool') ? new Promise(resolve=>window.finishCall=()=>original(url,options).then(resolve)) : original(url,options);
        }""")
        page.get_by_role('tab',name='Connections',exact=True).click()
        page.get_by_role('button',name='Refresh connections',exact=True).click()
        page.get_by_role('button',name='Tools for Donsetch mock',exact=True).click()
        page.get_by_label('Tool arguments (JSON)',exact=True).fill('{}')
        page.get_by_role('button',name='Run tool',exact=True).click()
        page.wait_for_function('Boolean(window.finishCall)')
        page.get_by_label('Available tools',exact=True).dispatch_event('change')
        page.evaluate('async () => {await finishCall(); await new Promise(requestAnimationFrame)}')
        expect(page.get_by_role('dialog',name='Confirm tool action',exact=True)).to_be_hidden()

    def connection_loading_clears_previous_tools():
        reset(admin=True)
        page.evaluate("aiFixture.mcpConnections=[{id:'a',name:'First',enabled:true,revision:'1'},{id:'b',name:'Second',enabled:true,revision:'1'}]")
        page.get_by_role('tab',name='Connections',exact=True).click()
        page.get_by_role('button',name='Refresh connections',exact=True).click()
        page.get_by_role('button',name='Tools for First',exact=True).click()
        expect(page.get_by_label('Available tools',exact=True)).to_have_value('draft_note')
        page.evaluate("""() => {const original=window.fetch; window.fetch=(url,options)=>String(url).endsWith('/DiscoverMcpTools') ? new Promise(resolve=>window.finishTools=()=>original(url,options).then(resolve)) : original(url,options)}""")
        page.get_by_role('button',name='Tools for Second',exact=True).click()
        expect(page.get_by_label('Available tools',exact=True)).to_be_disabled()
        expect(page.get_by_label('Available tools',exact=True).locator('option')).to_have_count(0)
        expect(page.get_by_role('button',name='Run tool',exact=True)).to_be_disabled()
        page.evaluate('async () => {await finishTools(); await new Promise(requestAnimationFrame)}')
        expect(page.get_by_label('Available tools',exact=True)).to_be_enabled()

    def exported_text_package_can_be_imported_with_json_escaping():
        reset(skill=True)
        page.evaluate("aiFixture.skills[0].instructions=String.fromCharCode(1).repeat(25000)")
        page.get_by_role('button',name='SQL review',exact=True).click()
        expect(page.get_by_role('button',name='Export version',exact=True)).to_be_enabled()
        with page.expect_download() as event:
            page.get_by_role('button',name='Export version',exact=True).click()
        data=Path(event.value.path()).read_bytes()
        assert len(data)>131072
        page.locator('#skill-import').set_input_files({'name':'escaped.json','mimeType':'application/json','buffer':data})
        expect(page.locator('#skill-status')).to_contain_text('Imported private draft')
        expect(page.get_by_label('Instructions',exact=True)).to_have_value(chr(1)*25000)

    def explicit_skill_actions():
        reset(skill=True)
        row=page.locator('#skill-list li').first
        row.get_by_role('button',name='Edit',exact=True).click()
        expect(page.get_by_label('Instructions',exact=True)).to_be_editable()
        expect(page.get_by_role('heading',name='Versions',exact=True)).to_be_visible()
        page.get_by_label('Instructions',exact=True).fill('Updated review')
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 2 saved')
        page.get_by_role('button',name='Open version 1',exact=True).click()
        expect(page.get_by_label('Instructions',exact=True)).to_have_value('Review SQL')
        expect(page.get_by_role('button',name='Save new version',exact=True)).to_be_disabled()
        expect(page.get_by_role('button',name='Delete skill',exact=True)).to_be_disabled()
        page.get_by_role('button',name='Open version 2',exact=True).click()
        page.locator('#skill-editor').get_by_role('button',name='Permissions',exact=True).click()
        expect(page.get_by_role('button',name='Update sharing',exact=True)).to_be_enabled()
        page.get_by_role('button',name='Close sharing',exact=True).click()
        page.get_by_role('button',name='Back to library',exact=True).click()
        page.evaluate("aiFixture.skills[0].canEdit=false; aiFixture.skills[0].owner='bob'")
        page.get_by_role('button',name='Refresh skills',exact=True).click()
        expect(row.get_by_role('button',name='Edit',exact=True)).to_be_disabled()

    def skill_assets_and_confirmed_delete():
        reset(skill=True)
        page.locator('#skill-list').get_by_role('button',name='Edit',exact=True).click()
        page.get_by_role('button',name='Add reference file',exact=True).click()
        page.locator('.skill-references > fieldset:visible').get_by_label('File name',exact=True).fill('checklist.md')
        expect(page.locator('.skill-references > fieldset:visible').get_by_label('Folder',exact=True)).to_have_value('references')
        page.locator('.skill-references > fieldset:visible').get_by_label('File content',exact=True).fill('Review nulls')
        expect(page.locator('.skill-path-preview')).to_contain_text('references/checklist.md')
        page.locator('.skill-references > fieldset:visible').get_by_label('Folder',exact=True).fill('examples/sql')
        expect(page.locator('.skill-path-preview')).to_contain_text('examples/sql/checklist.md')
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 2 saved')
        call=page.evaluate("aiFixture.calls.findLast(c=>c.url.endsWith('/PutSkill')).body")
        assert call['references']==[{'name':'examples/sql/checklist.md','content':'Review nulls'}]
        page.get_by_role('button',name='Delete skill',exact=True).click()
        dialog=page.get_by_role('dialog',name='Delete skill',exact=True)
        expect(dialog).to_contain_text('all versions')
        expect(dialog).to_contain_text('start a new conversation')
        expect(dialog).to_contain_text('SQL review')
        dialog.get_by_role('button',name='Cancel',exact=True).click()
        assert not page.evaluate("aiFixture.calls.some(c=>c.url.endsWith('/DeleteSkill'))")
        page.get_by_role('button',name='Delete skill',exact=True).click()
        dialog.get_by_role('button',name='Delete all versions',exact=True).click()
        expect(page.locator('#skill-editor')).to_be_hidden()
        expect(page.locator('#skill-list')).not_to_contain_text('SQL review')
        call=page.evaluate("aiFixture.calls.findLast(c=>c.url.endsWith('/DeleteSkill')).body")
        assert call['expectedRevision']=='2' and call['expectedSharingRevision']=='0'

    def asset_paths_preserve_existing_packages():
        reset(skill=True)
        page.evaluate("aiFixture.skills[0].references=[{name:'guide.md',content:'Root reference'},{name:'examples/sql/query.sql',content:'SELECT 1'}]")
        page.locator('#skill-list').get_by_role('button',name='Edit',exact=True).click()
        page.get_by_role('button',name='guide.md',exact=True).click()
        expect(page.locator('.skill-references > fieldset:visible').get_by_label('File name',exact=True)).to_have_value('guide.md')
        expect(page.locator('.skill-references > fieldset:visible').get_by_label('Folder',exact=True)).to_have_value('')
        page.get_by_role('button',name='examples/sql/query.sql',exact=True).click()
        expect(page.locator('.skill-references > fieldset:visible').get_by_label('File name',exact=True)).to_have_value('query.sql')
        expect(page.locator('.skill-references > fieldset:visible').get_by_label('Folder',exact=True)).to_have_value('examples/sql')
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 2 saved')
        call=page.evaluate("aiFixture.calls.findLast(c=>c.url.endsWith('/PutSkill')).body")
        assert [r['name'] for r in call['references']]==['guide.md','examples/sql/query.sql']
        page.get_by_role('button',name='guide.md',exact=True).click()
        page.locator('.skill-references > fieldset:visible').get_by_label('File name',exact=True).fill('wrong/path.md')
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('put directories in Folder')
        assert page.evaluate("aiFixture.calls.filter(c=>c.url.endsWith('/PutSkill')).length")==1

    def file_navigator_groups_paths_and_keeps_selection():
        reset(skill=True)
        page.add_style_tag(content=(ROOT/'crates/server/assets/app.css').read_text())
        page.evaluate("aiFixture.skills[0].references=[{name:'references/checklist.md',content:'Reference draft'},{name:'examples/sql/checklist.md',content:'Example draft'}]")
        page.locator('#skill-list').get_by_role('button',name='Edit',exact=True).click()
        nav=page.get_by_role('navigation',name='Skill files',exact=True)
        expect(nav.locator('summary')).to_have_count(3)
        file=nav.get_by_role('button',name='examples/sql/checklist.md',exact=True)
        expect(file).to_have_text('checklist.md')
        file.click()
        expect(nav.locator('[aria-current="true"]')).to_be_focused()
        active=page.locator('.skill-references > fieldset:visible')
        expect(active.get_by_label('File name',exact=True)).to_have_value('checklist.md')
        expect(active.get_by_label('Folder',exact=True)).to_have_value('examples/sql')
        active.get_by_label('File content',exact=True).fill('Keep selected example')
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 2 saved')
        expect(active.get_by_label('File content',exact=True)).to_have_value('Keep selected example')
        expect(nav.locator('[aria-current="true"]')).to_have_attribute('aria-label','examples/sql/checklist.md')
        assert not page.get_by_text('File name 1',exact=True).count()
        nav.locator('summary').filter(has_text='references').click()
        nav.get_by_role('button',name='SKILL.md',exact=True).click()
        expect(nav.get_by_role('button',name='references/checklist.md',exact=True)).not_to_be_visible()
        expect(nav.get_by_role('button',name='Add reference file',exact=True)).to_be_visible()
        if os.environ.get('ASTER_BROWSER_SCREENSHOT_DIR'):
            shots=Path(os.environ['ASTER_BROWSER_SCREENSHOT_DIR']); shots.mkdir(parents=True,exist_ok=True)
            nav.get_by_role('button',name='examples/sql/checklist.md',exact=True).click()
            page.evaluate('window.scrollTo(0,0)')
            page.screenshot(path=str(shots/'file-navigator-desktop.png'),full_page=True)
        page.set_viewport_size({'width':390,'height':844})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'file layout must fit a narrow screen'
        nav.locator('summary').filter(has_text='references').click()
        nav.get_by_role('button',name='references/checklist.md',exact=True).click()
        assert nav.evaluate('nav => { const box=nav.querySelector(".skill-file-entries").getBoundingClientRect(); const file=nav.querySelector("[aria-current=true]").getBoundingClientRect(); return file.top >= box.top && file.bottom <= box.bottom; }'), 'selected file must stay visible inside the scrollable navigator'
        if os.environ.get('ASTER_BROWSER_SCREENSHOT_DIR'):
            page.screenshot(path=str(shots/'file-navigator-mobile.png'),full_page=True)
        page.set_viewport_size({'width':1280,'height':720})

    def malformed_import_matrix_preserves_draft():
        import json
        valid={'name':'Review','files':[{'path':'SKILL.md','content':'Instructions'}]}
        invalid=[None, {}, {'name':'   ','files':valid['files']},
                 {'name':'é'*81,'files':valid['files']},
                 {**valid,'description':'é'*257},
                 {**valid,'files':[]},
                 {**valid,'files':valid['files']*2},
                 {**valid,'files':valid['files']+[{'path':'references/a.md','content':'one'},{'path':'references/a.md','content':'two'}]},
                 {**valid,'files':valid['files']+[{'path':'folder','content':'one'},{'path':'folder/a.md','content':'two'}]},
                 {**valid,'files':valid['files']+[{'path':'SKILL.md/a.md','content':'bad'}]},
                 {**valid,'files':valid['files']+[{'path':'/absolute.md','content':'bad'}]},
                 {**valid,'files':valid['files']+[{'path':'../parent.md','content':'bad'}]},
                 {**valid,'files':valid['files']+[{'path':'references\\bad.md','content':'bad'}]},
                 {**valid,'files':valid['files']+[{'path':'binary.txt','content':'AAAA','encoding':'base64'}]},
                 {**valid,'files':[{'path':'SKILL.md','content':'bad\u0000text'}]},
                 {**valid,'files':valid['files']+[{'path':f'r{i}.md','content':'r'} for i in range(17)]},
                 {**valid,'files':[{'path':'SKILL.md','content':'a'*32768}]}]
        invalid_results=[]
        for number,package in enumerate(invalid):
            reset()
            page.get_by_role('button',name='New skill',exact=True).click()
            page.get_by_label('Skill name',exact=True).fill('Keep draft')
            page.get_by_label('Instructions',exact=True).fill('Keep instructions')
            page.locator('#skill-import').set_input_files({'name':'bad.json','mimeType':'application/json','buffer':json.dumps(package).encode()})
            try:
                expect(page.locator('#skill-list-status')).to_contain_text('Invalid' if number < 16 else '32 KiB')
            except AssertionError:
                invalid_results.append(number)
            expect(page.get_by_label('Skill name',exact=True)).to_have_value('Keep draft')
            assert not page.evaluate("aiFixture.calls.some(c=>c.url.endsWith('/PutSkill'))"),number
        assert not invalid_results, f'invalid packages accepted or misreported: {invalid_results}'

    def invalid_utf8_and_surrogates_are_rejected():
        import json
        packages=[('broken.md',b'Review \xff joins'),('broken.json',b'{"name":"Review","files":[{"path":"SKILL.md","content":"bad \xff"}]}'),('surrogate.json',json.dumps({'name':'Review','files':[{'path':'SKILL.md','content':'bad'+chr(0xd800)}]}).encode())]
        for name,data in packages:
            reset()
            page.get_by_role('button',name='New skill',exact=True).click()
            page.get_by_label('Skill name',exact=True).fill('Keep draft')
            page.locator('#skill-import').set_input_files({'name':name,'mimeType':'application/octet-stream','buffer':data})
            expect(page.locator('#skill-list-status')).to_contain_text('Invalid')
            expect(page.get_by_label('Skill name',exact=True)).to_have_value('Keep draft')

    def markdown_and_unicode_byte_boundaries():
        import json
        reset()
        page.locator('#skill-import').set_input_files({'name':'review.md','mimeType':'text/markdown','buffer':b'\xef\xbb\xbf# Review\r\n\r\n'+ 'Olá 👋'.encode()})
        expect(page.get_by_label('Skill name',exact=True)).to_have_value('review')
        expect(page.get_by_label('Instructions',exact=True)).to_have_value('# Review\n\nOlá 👋')
        assert not page.evaluate("aiFixture.calls.some(c=>c.url.endsWith('/PutSkill'))")
        reset()
        package={'name':'é'*80,'description':'é'*256,'files':[{'path':'SKILL.md','content':'a'*(32768-160-512)}]}
        page.locator('#skill-import').set_input_files({'name':'limit.json','mimeType':'application/json','buffer':json.dumps(package,ensure_ascii=False).encode()})
        expect(page.locator('#skill-status')).to_contain_text('Imported private draft')
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 1 saved')

    def historical_export_import_round_trip():
        import json
        reset(skill=True)
        page.evaluate("""() => {const first=aiFixture.skills[0]; first.description='Unicode review 👋'; first.instructions='Olá — review\\njoins'; first.references=[{name:'root.md',content:'Root text'},{name:'examples/sql/join.sql',content:'SELECT 1;'}]; const second={...first,revision:'2',instructions:'New instructions',references:[]}; aiFixture.skills=[second]; aiFixture.versions[first.id]=[first,second];}""")
        page.get_by_role('button',name='SQL review',exact=True).click()
        page.get_by_role('button',name='Open version 1',exact=True).click()
        expect(page.get_by_role('button',name='Save new version',exact=True)).to_be_disabled()
        with page.expect_download() as event: page.get_by_role('button',name='Export version',exact=True).click()
        download=event.value; data=Path(download.path()).read_bytes(); package=json.loads(data)
        assert download.suggested_filename.endswith('-v1.json')
        assert set(package)=={'name','description','sourceRevision','files'}
        assert package['sourceRevision']=='1' and package['files'][0]['content']=='Olá — review\njoins'
        assert [f['path'] for f in package['files']]==['SKILL.md','root.md','examples/sql/join.sql']
        page.locator('#skill-import').set_input_files({'name':'roundtrip.json','mimeType':'application/json','buffer':data})
        expect(page.locator('#skill-status')).to_contain_text('Imported private draft')
        expect(page.get_by_role('button',name='Export version',exact=True)).to_be_disabled()
        page.get_by_role('button',name='Save new version',exact=True).click()
        expect(page.locator('#skill-status')).to_contain_text('Version 1 saved')
        saved=page.evaluate("aiFixture.calls.findLast(c=>c.url.endsWith('/PutSkill')).body")
        assert saved['id']=='' and saved['expectedRevision']=='0'
        assert saved['description']==package['description'] and saved['instructions']==package['files'][0]['content']
        assert saved['references']==[{'name':f['path'],'content':f['content']} for f in package['files'][1:]]

    def import_cancel_and_stale_read_preserve_work():
        import json
        reset()
        page.get_by_role('button',name='New skill',exact=True).click()
        page.get_by_label('Skill name',exact=True).fill('Keep draft')
        dismiss=lambda dialog:dialog.dismiss()
        page.on('dialog',dismiss)
        package={'name':'Replacement','files':[{'path':'SKILL.md','content':'Replace'}]}
        page.locator('#skill-import').set_input_files({'name':'valid.json','mimeType':'application/json','buffer':json.dumps(package).encode()})
        expect(page.get_by_label('Skill name',exact=True)).to_have_value('Keep draft')
        page.remove_listener('dialog',dismiss)
        reset()
        page.evaluate("""() => {for(const name of ['text','arrayBuffer']) { const original=Blob.prototype[name]; Blob.prototype[name]=function(){const file=this; return new Promise(resolve=>window.finishImport=()=>original.call(file).then(resolve));}; }}""")
        page.locator('#skill-import').set_input_files({'name':'valid.json','mimeType':'application/json','buffer':json.dumps(package).encode()})
        page.wait_for_function('typeof finishImport === "function"')
        page.get_by_role('button',name='New skill',exact=True).click()
        page.get_by_label('Skill name',exact=True).fill('Newer draft')
        page.evaluate('async()=>{await finishImport(); await new Promise(requestAnimationFrame)}')
        expect(page.get_by_label('Skill name',exact=True)).to_have_value('Newer draft')

    def export_waits_for_selected_version():
        import json
        reset(skill=True)
        page.evaluate("""() => {const first=aiFixture.skills[0]; const second={...first,revision:'2',instructions:'Newest'}; aiFixture.skills=[second]; aiFixture.versions[first.id]=[first,second];}""")
        page.get_by_role('button',name='SQL review',exact=True).click()
        expect(page.get_by_label('Instructions',exact=True)).to_have_value('Newest')
        page.evaluate("""() => { const original=window.fetch; window.fetch=(url,options)=>String(url).endsWith('/GetSkill') ? new Promise(resolve=>window.finishExportRead=()=>original(url,options).then(resolve)) : original(url,options); }""")
        page.get_by_role('button',name='Open version 1',exact=True).click()
        expect(page.get_by_role('button',name='Export version',exact=True)).to_be_disabled()
        page.evaluate('async()=>{await finishExportRead();await new Promise(requestAnimationFrame)}')
        expect(page.get_by_role('button',name='Export version',exact=True)).to_be_enabled()
        with page.expect_download() as event: page.get_by_role('button',name='Export version',exact=True).click()
        package=json.loads(Path(event.value.path()).read_text())
        assert package['sourceRevision']=='1' and package['files'][0]['content']=='Review SQL'

    completed = 0
    for test in (models_deep_link_opens_registration, compact_composer_controls, cell_send_recovers_after_pending_turn, defaults_retry_and_cell_send_recovery, notebook_mcp_controls_and_confirmation, personal_defaults_and_notebook_toggles, export_waits_for_selected_version, malformed_import_matrix_preserves_draft, invalid_utf8_and_surrogates_are_rejected, markdown_and_unicode_byte_boundaries, historical_export_import_round_trip, import_cancel_and_stale_read_preserve_work, file_navigator_groups_paths_and_keeps_selection, asset_paths_preserve_existing_packages, explicit_skill_actions, skill_assets_and_confirmed_delete, skills_versioning, skill_sharing, shared_skill_read_only, personal_permission, personal_token, admin_controls, conversation_pins, picker_loads_history_on_demand, new_skill_discards_pending_read, skill_library_search_and_scope, skill_import_is_private_unsaved_draft, skill_import_rejects_paths_without_losing_draft, pending_read_keeps_newer_edits, hidden_invalid_file_reports_error, historical_export_uses_saved_content, mcp_admin_form_keeps_tokens_private, changing_tool_requires_visible_confirmation, mcp_save_keeps_newer_draft, tool_selection_invalidates_pending_confirmation, connection_loading_clears_previous_tools, exported_text_package_can_be_imported_with_json_escaping):
        try:
            test()
            completed += 1
            print('PASS', test.__name__, flush=True)
        except Exception as error:
            failures.append(test.__name__)
            print('FAIL', test.__name__, str(error), flush=True)
    browser.close()
    assert not failures, ', '.join(failures)
print(f'AI_REGISTRY_BROWSER_OK: {completed} flows')
