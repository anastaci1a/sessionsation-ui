"""Isolated PTY acceptance. Fake public APIs; never invokes an agent or real HERDR."""
import argparse, codecs, fcntl, json, os, pathlib, pty, re, select, signal, struct, subprocess, tempfile, termios, time, unicodedata
import terminal_fixture as fixture_module
from terminal_fixture import Terminal, Screen, fixture
parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True);parser.add_argument('--samples',type=int,default=20);args=parser.parse_args();binary=str(pathlib.Path(args.binary).resolve())
fixture_module.BINARY=binary
starts=[];keys=[]
with tempfile.TemporaryDirectory(prefix='sessionsation-ui-pty-') as tmp:
 root=pathlib.Path(tmp);fixture(root)
 for i in range(args.samples):
  t=Terminal(root,delay=2 if i==0 else 0)
  try:
   starts.append(t.wait('New Session'));t.drain()
   for n in range(200 if i==0 else 2):
    t.start=time.perf_counter();t.send(b'\x1b[B' if n%2==0 else b'\x1b[A');keys.append(t.wait('Configuration' if n%2==0 else 'New Session',selected=True));t.drain()
   # Summary opens while backend reads may still be delayed.
   t.send(b'\r');t.wait('Account:');t.send(b'\x1b[B\x1b[B\r');t.wait('Session name')
   t.send(b'\x1b[200~pasted\nname\x1b[201~');time.sleep(.05);t.drain();assert not (root/'projects'/'pastedname').exists()
   # Resize then cancel; both preserve terminal state and create nothing.
   fcntl.ioctl(t.slave,termios.TIOCSWINSZ,struct.pack('HHHH',12,40,0,0));os.kill(t.process.pid,signal.SIGWINCH)
  finally:t.close()
 assert not (root/'start.json').exists();assert list((root/'projects').iterdir())==[]
 # Complete the inert handoff through the summary's separated Start action.
 t=Terminal(root)
 try:
  t.wait('New Session');time.sleep(.2);t.drain();t.send(b'\r');t.wait('Account:');t.send(b'\x1b[B\x1b[B\r');t.wait('Session name');t.send(b'new-session\r');t.wait('Directory:');time.sleep(.15);t.drain();t.send(b'\x1b[B\x1b[B\r')
  until=time.monotonic()+3
  while not (root/'start.json').exists() and time.monotonic()<until:
   if select.select([t.master],[],[],.02)[0]:os.read(t.master,65536)
  assert (root/'start.json').exists(),'Start did not submit'
  request=json.loads((root/'start.json').read_text());assert request['name']=='new-session';assert request['account_id']=='codex:fixture';assert request['space']=={'kind':'existing','id':'w1'}
  assert request['directory']['create']==[str(root/'projects'/'new-session')]
  assert not (root/'projects'/'new-session').exists(),'UI created the directory itself'
 finally:t.close()
 # A deployed config symlink must put atomic Configuration saves beside its target.
 settings=root/'settings';settings.mkdir();(settings/'popup.json').write_text(json.dumps({'binary':str(root/'backend'),'dataDir':str(root/'state')}))
 (settings/'preferences.json').write_text((root/'prefs.json').read_text());pointer=root/'config-pointer.json';pointer.symlink_to(settings/'popup.json')
 t=Terminal(root,client_config=pointer)
 try:
  t.wait('New Session');t.send(b'\x1b[B\r');t.wait('Directory defaults:');t.send(b'\r\x1b');t.wait('New Session')
  until=time.monotonic()+2
  while json.loads((settings/'preferences.json').read_text())['mode']!='per_space' and time.monotonic()<until:
   t.drain();time.sleep(.01)
  assert json.loads((settings/'preferences.json').read_text())['mode']=='per_space'
  assert not (root/'preferences.json').exists()
 finally:t.close()
def report(a):
 s=sorted(a);return {'samples':len(s),'p50_ms':s[len(s)//2],'p95_ms':s[min(len(s)-1,int(len(s)*.95))],'max_ms':max(s)}
print(json.dumps({'startup':report(starts),'input':report(keys),'checks':'delayed backend, summary, paste, staged directory, request identity, resize/cancel, exact TTY restoration, atomic preferences through config symlink','models':0},indent=2))
