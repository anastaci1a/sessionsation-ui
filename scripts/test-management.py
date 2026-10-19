"""Isolated management PTY acceptance; synthetic accounts and receipts only."""
import argparse,json,pathlib,tempfile,time,fcntl,termios,struct,os,signal
import terminal_fixture as f
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);args=p.parse_args();f.BINARY=str(pathlib.Path(args.binary).resolve())
BACKEND='''#!/usr/bin/env python3
import json,os,pathlib,sys,time
root=pathlib.Path(os.environ['FIXTURE_ROOT']);args=sys.argv;api=args[args.index('api')+1:];api=api[2:] if api[:1]==['--api-version'] else api
with (root/'calls.jsonl').open('a') as log:log.write(json.dumps(api)+'\\n')
caps={'launch':{'version':1,'list':True},'resume':{'version':1,'list':True},'providers':[{'id':'codex','account_limits':True,'enrollment':True,'account_deletion':True}]}
accounts=[{'id':'codex:fixture'+str(i),'selectable':True,'enabled':True,'usage_fresh':True,'prefill':{'eligible':True,'valid_until':int(time.time())+60},'usage':{'observed_at':int(time.time()),'short':({'duration_minutes':300,'used_percent':95 if i==0 else 1} if i!=2 else None),'weekly':{'duration_minutes':10080,'used_percent':20 if i==0 else 94}},'switch_thresholds':{'customized':True,'short_percent':80,'weekly_percent':90}} for i in range(6)]
session={'id':'11111111-1111-4111-8111-111111111111','generation':'captured','name':'Savedfixture','account_id':'codex:fixture0','account':'fixture0','availability':'saved','directory':str(root/'projects'),'capabilities':{'rename':True,'resume':True,'switch_account':False,'restart':False,'stop':False},'original_host':None}
receipt=None
if (root/'start.json').exists():
 request=json.loads((root/'start.json').read_text());receipt={'request':request,'phase':'admitted' if (root/'hold').exists() else 'running','tab_id':'w1:t2','operation_id':None}
if api==['capabilities']:result=caps
elif api==['account','list']:result=accounts
elif api==['state']:result={'sessions':[session],'capabilities':caps}
elif api[:1]==['session']:result=session
elif api==['operations']:result=([{'id':'deletion-fixture','kind':'account_delete','phase':'preserving_history','detail':'Preserving conversation history.','can_cancel':False}] if (root/'delete.json').exists() else [])
elif api[:2]==['account','delete']:
 if '--confirm' not in api:
  time.sleep(.3);result={'account_id':api[2],'can_delete':True,'file_scanning':False}
 else:
  (root/'delete.json').write_text(json.dumps(api));result={'id':'deletion-fixture','kind':'account_delete','phase':'admitted','can_cancel':False}
elif api[:2]==['account','remove']:
 time.sleep(2);result={'account_id':api[2],'revision':'preview','can_remove':True,'affected_sessions':[],'stored_data':{'files':3,'bytes':20,'history_files':1}}
elif api[:2]==['account','limits']:result={'limits':{'short_percent':80,'weekly_percent':90}}
elif api[:1] in [['launch'],['resume-launch']]:
 if api[1]=='list':result=[receipt] if receipt else []
 elif api[1]=='status':result=receipt
 elif api[1]=='validate':json.load(sys.stdin);result={'valid':True}
 elif api[1]=='start':
  request=json.load(sys.stdin);(root/'start.json').write_text(json.dumps(request));result={'phase':'admitted'}
 else:raise Exception(api)
else:raise Exception(api)
print(json.dumps({'api_version':1,'ok':True,'result':result}))
'''
with tempfile.TemporaryDirectory(prefix='sessionsation-management-') as tmp:
 root=pathlib.Path(tmp);f.fixture(root);(root/'backend').write_text(BACKEND);(root/'hold').touch()
 t=f.Terminal(root)
 try:
  t.wait('Manage Sessions');time.sleep(.2);t.drain()
  # Compact account list, common bottom bars and action separators.
  t.send(b'\x1b[B\x1b[B\r');t.wait('[add account]');t.send(b'\x1b[B');t.wait('5h');t.drain()
  assert all('│' not in ''.join(row) for row in t.screen.grid[22:23]),'one blank row above controls'
  t.send(b'\r');t.wait('Usage details');t.wait('Deregister account');t.drain()
  dots=[i for i,row in enumerate(t.screen.grid[:24]) if '•' in row];assert len(dots)>=2
  # Deregistration preview must paint before a slow backend returns; cancel is instant.
  t.send(b'\x1b[B\x1b[B\x1b[B\r');start=time.perf_counter();t.wait('Loading…');assert time.perf_counter()-start<.2
  start=time.perf_counter();t.send(b'\r');t.wait('Usage details');assert time.perf_counter()-start<.2
  time.sleep(2.1);t.drain();assert not t.screen.contains('Type codex-fixture0')
  # Actual deletion uses a fast intent preview, friendly confirmation and one CLI admission.
  t.send(b'\x1b[B\r');t.wait('Loading…');t.wait('Type codex-fixture0');t.send(b'codex-fixture0\r');t.wait('Continue in background')
  deadline=time.monotonic()+3
  while not (root/'delete.json').exists() and time.monotonic()<deadline:t.drain();time.sleep(.01)
  assert (root/'delete.json').exists();t.send(b'\r');t.wait('Manage Sessions')
  # Shared account picker right-aligns both percentages and slashes.
  t.send(b'\r\r');t.wait('Choose Account');t.wait('5h /');t.drain()
  rows=[''.join(row[:80]) for row in t.screen.grid[:24]]
  paired=[row for row in rows if '5h /' in row];assert len(paired)==5
  assert len({row.index('/') for row in paired})==1
  assert len({tuple(i for i,c in enumerate(row) if c=='%') for row in paired})==1
  weekly=next(row for row in rows if 'fixture2' in row);assert '/' not in weekly and 'wk' in weekly
  t.send(b'\x1b');t.wait('Account:');t.send(b'\x1b');t.wait('Manage Sessions')
  # Select a saved session, then Resume (selection itself never attaches).
  t.send(b'\x1b[B\r');t.wait('Savedfixture');t.send(b'\x1b[B\r');t.wait('Resume');t.send(b'\x1b[B\x1b[B\r');t.wait('Resume Session');t.wait('Directory:');time.sleep(.2);t.drain()
  t.send(b'\x1b[B\x1b[B\x1b[B\r');t.wait('Continue in background');t.send(b'\x1b');t.wait('Manage Sessions')
  deadline=time.monotonic()+3
  while not (root/'start.json').exists() and time.monotonic()<deadline:t.drain();time.sleep(.01)
  request=json.loads((root/'start.json').read_text())
  assert request['session_id']=='11111111-1111-4111-8111-111111111111';assert request['expected_generation']=='captured';assert 'directory' not in request
  (root/'hold').unlink();time.sleep(1.5);t.drain();assert t.process.poll() is None,'background completion must not exit/focus'
  assert t.screen.contains('Manage Sessions')
 finally:t.close()
 # Reopen and inspect the original receipt without another mutation.
 t=f.Terminal(root)
 try:
  t.wait('Manage Sessions');t.send(b'\x1b[B\r');t.wait('[new session]');time.sleep(.2);t.drain();t.send(b'\x1b[A');t.wait('Operations',selected=True);t.send(b'\r');t.wait('Resume');t.wait('complete')
  t.send(b'\x1b[B');t.wait('Resume',selected=True);t.send(b'\r');t.wait('Details',selected=True);t.send(b'\r');t.wait('Close',selected=True)
  t.send(b'\r');t.wait('Details',selected=True);time.sleep(1.2);t.drain()
  assert t.screen.contains('Details'),'status polling must not reopen dismissed details'
 finally:t.close()
 calls=[json.loads(line) for line in (root/'calls.jsonl').read_text().splitlines()]
 assert sum(c[:2]==['resume-launch','start'] for c in calls)==1
 assert sum(c[:2]==['account','delete'] and '--confirm' in c for c in calls)==1
 assert not any(c[:1]==['cancel-operation'] for c in calls)
 assert not any(c[0] in ['stop','restart','switch','send','submit','queue'] for c in calls)
 assert list((root/'projects').iterdir())==[]
print(json.dumps({'passed':True,'models':0,'checks':['compact bars and separators','right-aligned mixed-window picker','saved selection never attaches','resume captures identity and fixes directory','background completion retains menu','reopened receipt without resend','details close returns to submenu and stays closed','TTY restoration']},indent=2))
