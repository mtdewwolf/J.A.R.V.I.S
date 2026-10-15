#!/usr/bin/env python3
"""Start the local core + gateway and optionally attach the terminal client.
Secrets are generated once by jarvis-cli init. No services/accounts are provisioned.
"""
import argparse, os, pathlib, shutil, signal, subprocess, time, urllib.request
ROOT=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--demo',action='store_true',help='Explicit deterministic demo, not a real AI model')
parser.add_argument('--serve',action='store_true',help='Serve the browser client without attaching the terminal')
args=parser.parse_args()
env=dict(os.environ)
if args.demo:env.update(JARVIS_PROVIDER='fake',JARVIS_MODEL=env.get('JARVIS_MODEL','plain'))
for name in ['DATABASE_URL','JARVIS_PROVIDER','JARVIS_MODEL']:
    if not env.get(name):raise SystemExit(f'{name} is required. See docs/RUNNING.md.')
bun=env.get('BUN_BIN') or shutil.which('bun') or str(pathlib.Path.home()/'.bun/bin/bun')
if not pathlib.Path(bun).exists():raise SystemExit('Bun is required; see the pinned version in gateway/package.json')
for binary in ['jarvis-cli','jarvis-core']:
    if not (ROOT/'target/debug'/binary).exists():raise SystemExit('Run cargo build --workspace first.')
subprocess.run([str(ROOT/'target/debug/jarvis-cli'),'init'],env=env,check=True)
processes=[]
def wait(port,proc):
    for _ in range(100):
        if proc.poll() is not None:raise RuntimeError('service exited; inspect its error above')
        try:urllib.request.urlopen(f'http://127.0.0.1:{port}/healthz',timeout=.2);return
        except Exception:time.sleep(.1)
    raise RuntimeError('service startup timed out')
try:
    gateway=subprocess.Popen([bun,'run','gateway/src/server.ts'],cwd=ROOT,env=env);processes.append(gateway)
    wait(env.get('JARVIS_GATEWAY_PORT','8787'),gateway)
    core=subprocess.Popen([str(ROOT/'target/debug/jarvis-core')],cwd=ROOT,env=env);processes.append(core)
    port=env.get('JARVIS_CORE_PORT','8790');wait(port,core)
    print(f'JARVIS browser client: http://127.0.0.1:{port}',flush=True)
    if args.serve:
        print('Press Ctrl+C to stop. Pair another device with jarvis-cli pair.',flush=True)
        while all(p.poll() is None for p in processes):time.sleep(.5)
    else:
        env.setdefault('JARVIS_CORE_URL',f'http://127.0.0.1:{port}')
        subprocess.run([str(ROOT/'target/debug/jarvis-cli'),'chat'],env=env,check=True)
except KeyboardInterrupt:pass
finally:
    for proc in reversed(processes):
        if proc.poll() is None:
            proc.send_signal(signal.SIGINT)
            try:proc.wait(timeout=10)
            except subprocess.TimeoutExpired:proc.kill();proc.wait()
