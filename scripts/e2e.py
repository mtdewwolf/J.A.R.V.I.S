#!/usr/bin/env python3
"""Real processes and PostgreSQL; isolated temp secrets and an explicit fake model.
Requires DATABASE_URL pointing to a disposable, empty PostgreSQL database.
"""
import json, os, pathlib, signal, socket, subprocess, tempfile, time, urllib.request, urllib.error, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
ROOT=pathlib.Path(__file__).resolve().parent.parent

def free_port():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]

def wait(port,proc):
    for _ in range(100):
        if proc.poll() is not None:raise RuntimeError('service exited; inspect stderr')
        try:
            urllib.request.urlopen(f'http://127.0.0.1:{port}/healthz',timeout=.2);return
        except Exception:time.sleep(.1)
    raise RuntimeError('service did not start')

def run():
    assert os.environ.get('DATABASE_URL'), 'DATABASE_URL required (disposable database)'
    with tempfile.TemporaryDirectory(prefix='jarvis-e2e-') as tmp:
        config=pathlib.Path(tmp)/'config';export=pathlib.Path(tmp)/'export';export.mkdir();(export/'notes.txt').write_text('A private note for the approved read.\n')
        core_port,gateway_port=free_port(),free_port()
        counts={'calls':0,'lists':0}
        class McpHandler(BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_POST(self):
                body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                method=body['method']
                if method=='notifications/initialized':self.send_response(202);self.end_headers();return
                if method=='initialize':result={'protocolVersion':'2025-06-18','capabilities':{'tools':{}},'serverInfo':{'name':'fixture','version':'1'}}
                elif method=='tools/list':counts['lists']+=1;result={'tools':[{'name':'search','inputSchema':{'type':'object'}}]}
                elif method=='tools/call':
                    counts['calls']+=1
                    assert body['params']=={'name':'search','arguments':{'query':'notes'}}
                    result={'content':[{'type':'text','text':'One matching fixture note.'}],'isError':False}
                else:raise AssertionError(method)
                payload=json.dumps({'jsonrpc':'2.0','id':body['id'],'result':result}).encode()
                self.send_response(200)
                self.send_header('Content-Type','text/event-stream' if method=='tools/call' else 'application/json')
                self.send_header('Mcp-Session-Id','fixture-session');self.end_headers()
                self.wfile.write(b'data: '+payload+b'\r\n\r\n' if method=='tools/call' else payload)
            def do_DELETE(self):self.send_response(200);self.end_headers()
        mcp=ThreadingHTTPServer(('127.0.0.1',0),McpHandler)
        threading.Thread(target=mcp.serve_forever,daemon=True).start()
        env=dict(os.environ,JARVIS_CONFIG_DIR=str(config),JARVIS_EXPORT_DIR=str(export),JARVIS_CORE_PORT=str(core_port),JARVIS_GATEWAY_PORT=str(gateway_port),JARVIS_GATEWAY_URL=f'http://127.0.0.1:{gateway_port}',JARVIS_PROVIDER='fake',JARVIS_MODEL='tool')
        subprocess.run([str(ROOT/'target/debug/jarvis-cli'),'init'],env=env,check=True,stdout=subprocess.DEVNULL)
        token=(config/'token').read_text()
        connection_config=[{'id':'fixture','url':f'http://127.0.0.1:{mcp.server_port}/mcp','token_file':None,'tools':{'search':{'description':'Search the fixture notes','read_only':True,'schema':{'type':'object','properties':{'query':{'type':'string'}},'required':['query'],'additionalProperties':False}}}}]
        (config/'connections.json').write_text(json.dumps(connection_config));(config/'connections.json').chmod(0o600)
        bun=os.environ.get('BUN_BIN',str(pathlib.Path.home()/'.bun/bin/bun'))
        gateway=subprocess.Popen([bun,'run','gateway/src/server.ts'],cwd=ROOT,env=env,stdout=subprocess.DEVNULL)
        core=None
        def call(body,auth=True,extra=None):
            headers={'Content-Type':'application/json'}
            if auth:headers['Authorization']='Bearer '+token
            headers.update(extra or {})
            req=urllib.request.Request(f'http://127.0.0.1:{core_port}/v1/command',data=json.dumps(body).encode(),headers=headers)
            with urllib.request.urlopen(req,timeout=40) as r:return json.load(r)
        try:
            wait(gateway_port,gateway)
            core=subprocess.Popen([str(ROOT/'target/debug/jarvis-core')],cwd=ROOT,env=env,stdout=subprocess.DEVNULL)
            wait(core_port,core)
            try:call({'type':'system.status'},auth=False);raise AssertionError('unauthenticated accepted')
            except urllib.error.HTTPError as e:assert e.code==401
            try:call({'type':'system.status'},extra={'Origin':'https://evil.example'});raise AssertionError('origin accepted')
            except urllib.error.HTTPError as e:assert e.code==403
            sid=call({'type':'session.open'})['session_id']
            req={'type':'input.send','session_id':sid,'request_id':'e2e-1','text':'Read notes.txt'}
            result=call(req);assert result['state']=='waiting_approval',result
            assert call(req)['task_id']==result['task_id']
            p=result['proposal'];grant={'type':'approval.grant','task_id':result['task_id'],'proposal_id':p['proposal_id'],'digest':p['digest']}
            try:call(dict(grant,digest='wrong'));raise AssertionError('bad digest accepted')
            except urllib.error.HTTPError as e:assert e.code==409
            result=call(grant);assert result['state']=='succeeded',result
            assert 'received the approved tool result' in result['response']
            try:call(grant);raise AssertionError('grant replay accepted')
            except urllib.error.HTTPError as e:assert e.code==409
            candidate=call({'type':'memory.propose','key':'preferred_name','value':'Tony'})
            assert not call({'type':'memory.list'})['memories']
            call({'type':'memory.confirm','memory_id':candidate['memory_id'],'digest':candidate['digest']})
            # Pending approval survives restart, but is explicitly reapproved.
            pending=call(dict(req,request_id='e2e-2'));assert pending['state']=='waiting_approval'
            core.send_signal(signal.SIGINT);core.wait(timeout=10)
            core=subprocess.Popen([str(ROOT/'target/debug/jarvis-core')],cwd=ROOT,env=env,stdout=subprocess.DEVNULL);wait(core_port,core)
            assert call({'type':'memory.list'})['memories'][0]['value']=='Tony'
            pending=call({'type':'task.get','task_id':pending['task_id']});assert pending['state']=='waiting_approval'
            p=pending['proposal'];assert call({'type':'approval.grant','task_id':pending['task_id'],'proposal_id':p['proposal_id'],'digest':p['digest']})['state']=='succeeded'
            events=call({'type':'events.since','session_id':sid,'cursor':0})['events']
            assert any(e['type']=='response.committed' for e in events)
            assert 'private note' not in json.dumps(events) and 'Tony' not in json.dumps(events)
            call({'type':'memory.delete','memory_id':candidate['memory_id']});assert not call({'type':'memory.list'})['memories']
            erased=call({'type':'task.get','task_id':pending['task_id']});assert erased['response'] is None and 'erased' in erased['error']
            # Distinct device credential, one-time pairing, and revocation.
            pairing=call({'type':'pair.create'})
            def redeem():
                req=urllib.request.Request(f'http://127.0.0.1:{core_port}/v1/pair',data=json.dumps({'code':pairing['code'],'label':'Android test'}).encode(),headers={'Content-Type':'application/json'})
                with urllib.request.urlopen(req) as r:return json.load(r)
            device=redeem()
            try:redeem();raise AssertionError('pairing replay accepted')
            except urllib.error.HTTPError as e:assert e.code==401
            assert call({'type':'system.status'},extra={'Authorization':'Bearer '+device['token']})['mode']=='demo'
            call({'type':'device.revoke','device_id':device['device_id']})
            try:call({'type':'system.status'},extra={'Authorization':'Bearer '+device['token']});raise AssertionError('revoked device accepted')
            except urllib.error.HTTPError as e:assert e.code==401
            assert call({'type':'connection.check','connection_id':'fixture'})['reachable']
            assert counts['calls']==0
            core.send_signal(signal.SIGINT);core.wait(timeout=10);env['JARVIS_MODEL']='connection'
            core=subprocess.Popen([str(ROOT/'target/debug/jarvis-core')],cwd=ROOT,env=env,stdout=subprocess.DEVNULL);wait(core_port,core)
            proposal=call(dict(req,request_id='e2e-mcp'))
            assert proposal['state']=='waiting_approval',proposal
            assert counts['calls']==0
            p=proposal['proposal'];assert p['tool']=='connection.call'
            result=call({'type':'approval.grant','task_id':proposal['task_id'],'proposal_id':p['proposal_id'],'digest':p['digest']})
            assert result['state']=='succeeded',result
            assert counts['calls']==1
            # UI shell and API responses share the same core.
            with urllib.request.urlopen(f'http://127.0.0.1:{core_port}/') as r:
                assert 'Speech' not in r.headers.get('Content-Security-Policy','')
                assert b'At your service' in r.read()
            print('PASS: paired device tokens, revocation, MCP discovery/allowlist/approval, CRLF SSE, derived memory erasure, UI shell')
            print('PASS: auth, origin, conversation, approval binding/replay, tool continuation, restart, confirmed memory, metadata-only events, erasure')
        finally:
            mcp.shutdown()
            for proc in (core,gateway):
                if proc and proc.poll() is None:
                    proc.send_signal(signal.SIGINT)
                    try:proc.wait(timeout=10)
                    except subprocess.TimeoutExpired:proc.kill();proc.wait()

if __name__=='__main__':run()
