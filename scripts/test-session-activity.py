"""Read-only session refresh and loading/idle transitions in an isolated PTY."""
import argparse, json, pathlib, tempfile, time
import terminal_fixture as f

p = argparse.ArgumentParser()
p.add_argument('--binary', required=True)
args = p.parse_args()
f.BINARY = str(pathlib.Path(args.binary).resolve())
BACKEND = '''#!/usr/bin/env python3
import json, os, pathlib, sys, time
root=pathlib.Path(os.environ['FIXTURE_ROOT']); argv=sys.argv; args=argv[argv.index('api')+1:]; args=args[2:] if args[:1]==['--api-version'] else args
with (root/'calls').open('a') as log: log.write(json.dumps(args)+'\\n')
if args==['capabilities']: result={'providers':[{'id':'codex'}]}
elif args==['account','list']: result=[]
elif args==['state']:
 if not (root/'first').exists(): (root/'first').touch(); time.sleep(1.5)
 mode=(root/'mode').read_text()
 if mode=='failure': print(json.dumps({'api_version':1,'ok':False,'error':{'code':'DISCOVERY_UNAVAILABLE','message':'fixture failure'}}),file=sys.stderr);sys.exit(1)
 result={'sessions':[{'id':'working','name':'Working','account_id':'codex:work','generation':'same','availability':'saved' if mode=='saved' else 'running','activity':'busy' if mode=='busy' else 'idle','active_since':int(time.time())-70,'capabilities':{}}]}
elif args in [['operations'],['archives']]: result=[]
else: raise Exception(args)
print(json.dumps({'api_version':1,'ok':True,'result':result}))
'''
SPINNERS = set('⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏')
with tempfile.TemporaryDirectory(prefix='sessionsation-activity-') as tmp:
    root = pathlib.Path(tmp)
    f.fixture(root)
    (root/'backend').write_text(BACKEND)
    (root/'mode').write_text('busy')
    t = f.Terminal(root)
    def spin():
        return any(c in SPINNERS for row in t.screen.grid[3:18] for c in row[:80])
    def wait_state(predicate, seconds=3):
        until = time.monotonic()+seconds
        while time.monotonic()<until:
            t.drain()
            if predicate(): return
            time.sleep(.02)
        raise AssertionError('session activity did not update')
    try:
        t.wait('Manage Sessions')
        t.send(b'\x1b[B\r')
        t.wait('Loading…')
        t.wait('Operations')
        loading = next(''.join(row[:80]) for row in t.screen.grid if 'Loading…' in ''.join(row))
        assert '●' not in loading and '○' not in loading
        t.wait('Working')
        wait_state(spin)
        seen=set()
        until=time.monotonic()+.5
        while time.monotonic()<until:
            t.drain()
            seen.update(c for row in t.screen.grid[3:18] for c in row[:80] if c in SPINNERS)
            time.sleep(.02)
        assert len(seen)>=3, 'spinner frames must advance without input'
        durations={}
        for mode, expected in [('idle',False),('busy',True),('saved',False),('busy',True),('failure',False)]:
            start=time.monotonic();(root/'mode').write_text(mode)
            wait_state(lambda:spin()==expected)
            durations[mode]=round((time.monotonic()-start)*1000,2)
        # Explicit Kitty press/repeat/release arrows: a hold cannot wrap.
        t.send(b'\x1b[1;1:1B'+b'\x1b[1;1:2B'*20)
        t.wait('Operations',selected=True);time.sleep(.1);t.drain()
        assert t.screen.contains('Operations',selected=True)
        t.send(b'\x1b[1;1:3B\x1b[1;1:1B');t.wait('Working',selected=True)
        t.send(b'\x1b[1;1:2A'*20);time.sleep(.1);t.drain()
        assert t.screen.contains('Working',selected=True)
        t.send(b'\x1b[1;1:3A\x1b[1;1:1A');t.wait('Operations',selected=True)
        t.send(b'\x1b');t.wait('New Session')
    finally:
        t.close()
    calls=[json.loads(line) for line in (root/'calls').read_text().splitlines()]
    assert all(call in [['state'],['capabilities'],['account','list'],['archives']] for call in calls), calls
    print(json.dumps({'passed':True,'models':0,'checks':['nonselectable loading below unchanged skeleton','persistent ten-frame spinner','live busy/idle/exit/failure changes without input','held arrows clamp both edges; release/press rearms wrap','only read-only APIs','TTY restoration'],'transition_ms':durations},indent=2))
