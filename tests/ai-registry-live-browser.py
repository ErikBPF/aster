"""Real AI registry browser + PostgreSQL restart proof; no upstream model calls."""
import base64
import json
import os
from pathlib import Path
import socket
import sys
import subprocess
import tempfile
import time
import urllib.request
import uuid

from playwright.sync_api import sync_playwright, expect


def unused_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def command(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL).strip()


container = 'aster-ai-browser-' + uuid.uuid4().hex[:12]
binary = str(Path(os.environ.get('ASTER_SERVER_BIN', 'target/debug/aster-server')).resolve())
server = None
mock = None
try:
    mock_port = unused_port()
    mock = subprocess.Popen([sys.executable, str(Path(__file__).parent/'fixtures/donsetch-mcp.py'), '--port', str(mock_port)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    command('podman', 'run', '-d', '--name', container, '-e', 'POSTGRES_PASSWORD=disposable-test',
            '-p', '127.0.0.1::5432', 'docker.io/library/postgres:17-alpine')
    for attempt in range(100):
        ready = subprocess.run(['podman', 'exec', container, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres'], capture_output=True)
        if ready.returncode == 0:
            break
        time.sleep(.2)
    else:
        raise AssertionError('disposable PostgreSQL did not start')
    command('podman', 'exec', container, 'createdb', '-U', 'postgres', 'ai_browser')
    db_port = command('podman', 'port', container, '5432/tcp').rsplit(':', 1)[1]
    with tempfile.TemporaryDirectory(prefix='aster-ai-browser-') as work:
        port = unused_port()
        base = f'http://127.0.0.1:{port}'
        # No developer/provider credentials or standing deployment configuration.
        env = {key: value for key, value in os.environ.items()
               if not key.startswith('ASTER_') and key not in ('DATABASE_URL', 'PORT')}
        env.update(ASTER_BIND=f'127.0.0.1:{port}', ASTER_METRICS_BIND=f'127.0.0.1:{unused_port()}',
                   DATABASE_URL=f'postgres://postgres:disposable-test@127.0.0.1:{db_port}/ai_browser',
                   ASTER_METADATA_STORE='postgres', ASTER_STATE_STORE='memory', ASTER_CONVERSATION_STORE='metadata',
                   ASTER_IDP_KIND='none', ASTER_DEV_LOGIN='1', ASTER_SECRET_STORE='env',
                   ASTER_NOTEBOOK_DIR=work+'/notebooks', ASTER_ENGINES='mock;mock;local',
                   ASTER_CATALOGS='mock;mock;local', ASTER_CATALOG_BINDINGS='mock;mock;mock;unprotected',
                   ASTER_PERSONAL_MODEL_ACTIVE_KEY='fixture-v1',
                   ASTER_PERSONAL_MODEL_KEYS=json.dumps({'fixture-v1':base64.b64encode(bytes(range(32))).decode()}),
                   ASTER_PERSONAL_MODEL_ALLOWED_ORIGINS='https://models.example.invalid',
                   ASTER_MCP_ACTIVE_KEY='fixture-v1', ASTER_MCP_KEYS=json.dumps({'fixture-v1':base64.b64encode(bytes(range(32))).decode()}),
                   ASTER_MCP_DEMO='1', ASTER_MCP_DEMO_ENDPOINT=f'http://127.0.0.1:{mock_port}/mcp')
        with open(work+'/server.log', 'w') as log:
            def start():
                process = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
                for attempt in range(150):
                    assert process.poll() is None, 'disposable server exited before readiness'
                    try:
                        urllib.request.urlopen(base+'/healthz', timeout=1).close()
                        return process
                    except OSError:
                        time.sleep(.1)
                process.terminate()
                process.wait(timeout=10)
                raise AssertionError('disposable server did not become ready')

            server = start()
            with sync_playwright() as pw:
                browser = pw.chromium.launch(headless=True, executable_path=os.environ.get('ASTER_BROWSER_EXECUTABLE'))
                page = browser.new_page(viewport={'width':1440,'height':1000})
                errors = []
                page.on('pageerror', lambda error: errors.append(str(error)))
                page.goto(base+'/dev-login?subject=alice&roles=admin')
                page.goto(base+'/settings/llm')
                expect(page.get_by_role('heading', name='AI workspace', exact=True)).to_be_visible()
                page.get_by_role('button', name='New skill', exact=True).click()
                page.get_by_label('Skill name', exact=True).fill('Live SQL review')
                page.get_by_label('Instructions', exact=True).fill('Review joins before proposing SQL.')
                page.get_by_role('button', name='Add reference file', exact=True).click()
                page.locator('.skill-references > fieldset:visible').get_by_label('File name', exact=True).fill('guide.md')
                page.locator('.skill-references > fieldset:visible').get_by_label('File content', exact=True).fill('Check null semantics.')
                page.get_by_role('button', name='Save new version', exact=True).click()
                expect(page.locator('#skill-status')).to_contain_text('Version 1 saved')
                page.get_by_role('button',name='SKILL.md',exact=True).click()
                page.get_by_label('Instructions', exact=True).fill('Review joins and filters.')
                page.get_by_role('button', name='Save new version', exact=True).click()
                expect(page.locator('#skill-status')).to_contain_text('Version 2 saved')
                page.get_by_label('Skill version', exact=True).select_option('1')
                expect(page.get_by_label('Instructions', exact=True)).to_have_value('Review joins before proposing SQL.')
                expect(page.get_by_role('button', name='Save new version', exact=True)).to_be_disabled()
                with page.expect_download() as exported:
                    page.get_by_role('button',name='Export version',exact=True).click()
                package_bytes=Path(exported.value.path()).read_bytes()
                package=json.loads(package_bytes)
                assert package['sourceRevision']=='1'
                assert package['files']==[{'path':'SKILL.md','content':'Review joins before proposing SQL.'},{'path':'references/guide.md','content':'Check null semantics.'}]
                original_id=page.request.post(base+'/aster.v1.Aster/ListSkills',data={}).json()['skills'][0]['id']
                defaults=page.request.post(base+'/aster.v1.Aster/PutSkillDefaults',data={'expectedRevision':'0','skills':[{'id':original_id,'revision':'1'}]})
                assert defaults.ok

                page.get_by_label('Skill version', exact=True).select_option('2')
                expect(page.get_by_label('Instructions', exact=True)).to_have_value('Review joins and filters.')
                page.locator('#skill-editor').get_by_role('button', name='Permissions', exact=True).click()
                page.get_by_label('Sharing scope', exact=True).select_option('organization')
                with page.expect_response('**/aster.v1.Aster/ShareSkill') as sharing:
                    page.get_by_role('button', name='Update sharing', exact=True).click()
                assert sharing.value.ok, 'skill sharing update failed'
                expect(page.locator('#skill-status')).to_contain_text('Sharing updated')
                bob = browser.new_page()
                bob.goto(base+'/dev-login?subject=bob&roles=editor')
                bob.goto(base+'/settings/llm')
                bob.get_by_role('button', name='Live SQL review', exact=True).click()
                expect(bob.get_by_label('Instructions', exact=True)).to_have_value('Review joins and filters.')
                expect(bob.get_by_role('button', name='Save new version', exact=True)).to_be_disabled()
                page.locator('#skill-editor').get_by_role('button', name='Permissions', exact=True).click()
                page.get_by_label('Sharing scope', exact=True).select_option('private')
                with page.expect_response('**/aster.v1.Aster/ShareSkill') as sharing:
                    page.get_by_role('button', name='Update sharing', exact=True).click()
                assert sharing.value.ok, 'skill sharing update failed'
                expect(page.locator('#skill-status')).to_contain_text('Sharing updated')
                bob.reload()
                expect(bob.locator('#skill-list-status')).to_contain_text('No skills available')
                assert bob.get_by_role('button', name='Live SQL review', exact=True).count() == 0
                bob.close()
                page.get_by_role('tab', name='Administration', exact=True).click()
                page.get_by_label('Allowed users', exact=True).fill('alice')
                page.get_by_role('button', name='Save personal-model permissions', exact=True).click()
                expect(page.locator('#model-policy-status')).to_contain_text('saved')
                page.get_by_role('tab', name='Models', exact=True).click()
                page.get_by_label('Registration name', exact=True).fill('live-model')
                page.get_by_label('Base URL', exact=True).fill('https://models.example.invalid/v1')
                page.get_by_label('Provider model ID', exact=True).fill('fixture-model')
                token = 'disposable-browser-fixture-token'
                page.get_by_label('Token', exact=True).fill(token)
                with page.expect_response('**/aster.v1.Aster/PutPersonalModel') as registration:
                    page.get_by_role('button', name='Save personal model', exact=True).click()
                assert registration.value.ok, 'personal model registration failed'
                assert token not in registration.value.text(), 'registration disclosed token'
                expect(page.locator('#personal-model-status')).to_contain_text('saved')
                expect(page.get_by_label('Token', exact=True)).to_have_value('')
                expect(page.locator('#model-list')).to_contain_text('personal/live-model')
                assert token not in page.request.get(base+'/api/llm').text(), 'model list disclosed token'
                encrypted = command('podman', 'exec', container, 'psql', '-U', 'postgres', '-d', 'ai_browser', '-Atc',
                                    "SELECT api_key='' AND ciphertext IS NOT NULL AND key_version='fixture-v1' FROM llm_helpers WHERE subject='alice' AND id='live-model'")
                assert encrypted == 't', 'personal token must be encrypted without retained plaintext'
                page.get_by_role('tab', name='Connections', exact=True).click()
                page.get_by_role('button', name='New connection', exact=True).click()
                page.get_by_label('Connection ID', exact=True).fill('donsetch')
                page.get_by_label('Connection name', exact=True).fill('Donsetch mock')
                page.get_by_label('MCP endpoint', exact=True).fill(f'http://127.0.0.1:{mock_port}/mcp')
                page.get_by_label('Connection users', exact=True).fill('alice')
                page.get_by_role('button', name='Save connection', exact=True).click()
                expect(page.locator('#mcp-status')).to_contain_text('saved')
                page.get_by_role('button', name='Tools for Donsetch mock', exact=True).click()
                expect(page.get_by_label('Available tools', exact=True)).to_have_value('inspect_catalog')
                page.get_by_role('button', name='Approve as read-only', exact=True).click()
                expect(page.get_by_role('button', name='Require confirmation', exact=True)).to_be_visible()
                page.get_by_role('button', name='Run tool', exact=True).click()
                expect(page.locator('#mcp-tool-result')).to_contain_text('3 synthetic tables')
                page.get_by_label('Available tools', exact=True).select_option('draft_note')
                page.get_by_label('Tool arguments (JSON)', exact=True).fill('{"text":"Review completed"}')
                page.get_by_role('button', name='Run tool', exact=True).click()
                dialog=page.get_by_role('dialog', name='Confirm tool action', exact=True)
                expect(dialog).to_contain_text('Review completed')
                dialog.get_by_role('button', name='Confirm action', exact=True).click()
                expect(page.locator('#mcp-tool-result')).to_contain_text('mock receipt')
                pending=page.request.post(base+'/aster.v1.Aster/CallMcpTool',data={'id':'donsetch','tool':'draft_note','argumentsJson':'{"text":"After restart"}'}).json()
                assert pending.get('confirmation'), 'changing call requires a pending confirmation'
                if os.environ.get('ASTER_BROWSER_SCREENSHOT_DIR'):
                    shots=Path(os.environ['ASTER_BROWSER_SCREENSHOT_DIR']); shots.mkdir(parents=True,exist_ok=True)
                    page.screenshot(path=str(shots/'aster-mcp-tools.png'),full_page=True)
                server.terminate()
                server.wait(timeout=15)
                server = start()
                page.reload()
                expect(page.get_by_role('button', name='Live SQL review', exact=True)).to_be_visible()
                page.get_by_role('button', name='Live SQL review', exact=True).click()
                expect(page.get_by_label('Instructions', exact=True)).to_have_value('Review joins and filters.')
                page.get_by_label('Skill version', exact=True).select_option('1')
                expect(page.get_by_label('Instructions', exact=True)).to_have_value('Review joins before proposing SQL.')
                page.get_by_role('tab', name='Models', exact=True).click()
                expect(page.locator('#model-list')).to_contain_text('personal/live-model')
                expect(page.get_by_role('button', name='Save personal model', exact=True)).to_be_enabled()
                expect(page.get_by_label('Token', exact=True)).to_have_value('')
                assert token not in page.request.get(base+'/api/llm').text(), 'restarted model list disclosed token'
                page.get_by_role('tab',name='Connections',exact=True).click()
                expect(page.get_by_role('button',name='Tools for Donsetch mock',exact=True)).to_be_visible()
                confirmed={'id':'donsetch','tool':'draft_note','argumentsJson':'{"text":"After restart"}','confirmation':pending['confirmation']}
                first=page.request.post(base+'/aster.v1.Aster/CallMcpTool',data=confirmed)
                assert first.ok and 'mock receipt' in first.text(), 'pending exact confirmation must survive restart'
                replay=page.request.post(base+'/aster.v1.Aster/CallMcpTool',data=confirmed)
                assert replay.status == 403, 'persisted confirmation must remain single-use'
                page.get_by_role('tab',name='Skills',exact=True).click()
                page.get_by_role('button',name='Back to library',exact=True).click()
                page.locator('#skill-list').get_by_role('button',name='Edit',exact=True).click()
                expect(page.get_by_role('heading',name='Versions',exact=True)).to_be_visible()
                page.get_by_role('button',name='Open version 1',exact=True).click()
                expect(page.get_by_role('button',name='Delete skill',exact=True)).to_be_disabled()
                page.get_by_role('button',name='Open version 2',exact=True).click()
                page.get_by_role('button',name='references/guide.md',exact=True).click()
                expect(page.locator('.skill-references > fieldset:visible').get_by_label('File name',exact=True)).to_have_value('guide.md')
                expect(page.locator('.skill-references > fieldset:visible').get_by_label('Folder',exact=True)).to_have_value('references')
                page.get_by_role('button',name='Delete skill',exact=True).click()
                deletion=page.get_by_role('dialog',name='Delete skill',exact=True)
                deletion.get_by_role('button',name='Cancel',exact=True).click()
                expect(page.get_by_role('button',name='Save new version',exact=True)).to_be_enabled()
                page.get_by_role('button',name='Delete skill',exact=True).click()
                with page.expect_response('**/aster.v1.Aster/DeleteSkill') as deleted:
                    deletion.get_by_role('button',name='Delete all versions',exact=True).click()
                assert deleted.value.ok, 'owner must be able to delete a saved skill'
                expect(page.locator('#skill-editor')).to_be_hidden()
                expect(page.locator('#skill-list')).not_to_contain_text('Live SQL review')
                server.terminate(); server.wait(timeout=15); server=start()
                page.reload()
                expect(page.locator('#skill-list-status')).to_contain_text('No skills available')
                persisted=page.request.post(base+'/aster.v1.Aster/GetSkillDefaults',data={}).json()
                assert persisted['skills']==[{'id':original_id,'revision':'1'}], 'personal defaults must survive restart with pinned versions'
                cleared=page.request.post(base+'/aster.v1.Aster/PutSkillDefaults',data={'expectedRevision':persisted['revision'],'skills':[]})
                assert cleared.ok, 'deleted defaults can be switched off'
                page.locator('#skill-import').set_input_files({'name':'roundtrip.json','mimeType':'application/json','buffer':package_bytes})
                expect(page.locator('#skill-status')).to_contain_text('Imported private draft')
                assert not page.request.post(base+'/aster.v1.Aster/ListSkills',data={}).json().get('skills'), 'import must not save automatically'
                with page.expect_response('**/aster.v1.Aster/PutSkill') as imported:
                    page.get_by_role('button',name='Save new version',exact=True).click()
                assert imported.value.ok
                clone=imported.value.json()
                assert clone['id']!=original_id and clone['revision']=='1' and clone['audience']=='private' and clone['owner']=='alice'
                assert clone['instructions']==package['files'][0]['content']
                assert clone['references']==[{'name':f['path'],'content':f['content']} for f in package['files'][1:]]
                server.terminate(); server.wait(timeout=15); server=start()
                loaded=page.request.post(base+'/aster.v1.Aster/GetSkill',data={'id':clone['id']})
                assert loaded.ok and loaded.json()['references']==clone['references'], 'imported package must persist after restart'
                assert not errors, 'browser JavaScript errors occurred'
                browser.close()
            print('AI_REGISTRY_LIVE_BROWSER_OK: skills/versioning/sharing/policy/models/mcp/read-approval/confirmation/restart/encryption/skill-delete/package-roundtrip', flush=True)
finally:
    if mock is not None:
        mock.terminate(); mock.wait(timeout=10)
    if server is not None and server.poll() is None:
        server.terminate()
        server.wait(timeout=15)
    subprocess.run(['podman', 'rm', '-fv', container], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
