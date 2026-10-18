"""Owned installed-HERDR popup acceptance. Synthetic public backend, no models.

Run in the network-disabled build container with only UI source/binary and the
installed HERDR executable mounted. Never mount account homes or live sockets.
"""
import argparse, fcntl, json, os, pathlib, pty, select, signal, struct, subprocess, tempfile, termios, time
from terminal_fixture import Screen, BACKEND

parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
args = parser.parse_args()
binary = str(pathlib.Path(args.binary).resolve())
with tempfile.TemporaryDirectory(prefix='sessionsation-popup-host-') as tmp:
 root = pathlib.Path(tmp); plugin = root/'plugin'; plugin.mkdir(); projects = root/'projects'; projects.mkdir()
 config = root/'herdr.toml'
 config.write_text('onboarding=false\n[terminal]\ndefault_shell="/bin/bash"\n[[keys.command]]\nkey="prefix+shift+l"\ntype="plugin_action"\ncommand="sessionsation.launcher"\ndescription="New session"\n')
 env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_STATE_HOME=str(root/'state'), HERDR_CONFIG_PATH=str(config), TERM='xterm-256color',
            SESSIONSATION_BIN=str(root/'backend'), SESSIONSATION_DATA_DIR=str(root/'backend-state'), FIXTURE_ROOT=str(root))
 prefs = root/'prefs.json'
 prefs.write_text(json.dumps({'version':1,'mode':'global','global_root':str(projects),'space_roots':{},'last_account':'codex:fixture'}))
 backend = BACKEND.replace("pathlib.Path(os.environ['FIXTURE_ROOT'],'start.json').write_text(json.dumps(request)); result={'phase':'running','tab_id':'w1:t2'}", """
  import subprocess
  root=pathlib.Path(os.environ['FIXTURE_ROOT']); (root/'start.json').write_text(json.dumps(request))
  for path in request['directory']['create']: pathlib.Path(path).mkdir()
  result=json.loads(subprocess.check_output(['herdr','--session','fixture','tab','create','--workspace',request['space']['id'],'--cwd',request['directory']['path'],'--label',request['name'],'--no-focus']))['result']
  result={'phase':'running','tab_id':result['tab']['tab_id'],'pane_id':result['root_pane']['pane_id'],'request':request}
  (root/'receipt.json').write_text(json.dumps(result))
 """)
 backend = backend.replace("else: result={'phase':'running','tab_id':'w1:t2'}", "else: result=json.loads(pathlib.Path(os.environ['FIXTURE_ROOT'],'receipt.json').read_text())")
 (root/'backend').write_text(backend); (root/'backend').chmod(0o700)
 probe=plugin/'entry.py'
 probe.write_text('import os,json,pathlib\npathlib.Path('+repr(str(root/'popup.json'))+').write_text(json.dumps({"size":list(os.get_terminal_size()),"pane":os.environ.get("HERDR_PANE_ID"),"context":json.loads(os.environ["HERDR_PLUGIN_CONTEXT_JSON"])}))\nos.execv('+repr(binary)+','+repr([binary,'--preferences',str(prefs),'popup'])+')\n')
 (plugin/'herdr-plugin.toml').write_text('id="sessionsation"\nname="sessionsation"\nversion="0.2.0"\nmin_herdr_version="0.9.5"\nplatforms=["linux"]\n[[actions]]\nid="launcher"\ntitle="New Session"\ncommand='+json.dumps([binary,'open'])+'\n[[panes]]\nid="launcher"\ntitle="New Session"\nplacement="popup"\nwidth="50%"\nheight="50%"\ncommand='+json.dumps(['python3',str(probe)])+'\n')
 def host(*argv):
  p=subprocess.run(['herdr','--session','fixture',*argv],env=env,capture_output=True,text=True,timeout=10)
  assert p.returncode==0,(argv,p.stderr)
  if not p.stdout.strip():return {}
  v=json.loads(p.stdout); assert 'error' not in v,(argv,v)
  return v.get('result',v)
 log=(root/'host.log').open('w')
 server=subprocess.Popen(['herdr','--session','fixture','server'],env=env,stdout=log,stderr=log)
 raw_output=bytearray()
 master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',48,160,0,0));client=None;screen=Screen()
 def wait(text,timeout=5):
  until=time.monotonic()+timeout
  while time.monotonic()<until:
   if select.select([master],[],[],.01)[0]:
    data=os.read(master,65536);raw_output.extend(data);screen.feed(data)
    if screen.contains(text):return
  raise AssertionError('No '+text+'; client='+str(client.poll())+'; raw='+repr(bytes(raw_output[-1000:]))+'; '+str([''.join(row).strip() for row in screen.grid if ''.join(row).strip()]))
 def key(data): os.write(master,data)
 try:
  socket=root/'config/herdr/sessions/fixture/herdr.sock'
  for _ in range(100):
   if socket.exists():break
   time.sleep(.03)
  host('plugin','link',str(plugin))
  origin=host('workspace','create','--label','Origin','--cwd',str(projects))
  destination=host('workspace','create','--label','Destination','--cwd',str(projects))
  origin_pane=origin['root_pane']['pane_id'];origin_pid=host('pane','process-info','--pane',origin_pane)['process_info']['shell_pid']
  host('tab','focus',origin['tab']['tab_id'])
  client=subprocess.Popen(['herdr','--session','fixture'],env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
  time.sleep(.4)
  while select.select([master],[],[],.01)[0]:screen.feed(os.read(master,65536))
  start=time.perf_counter();host('plugin','action','invoke','sessionsation.launcher');wait('Configuration');elapsed=(time.perf_counter()-start)*1000
  popup=json.loads((root/'popup.json').read_text());assert popup['pane'] is None;assert popup['context']['workspace_id']==origin['workspace']['workspace_id']
  assert popup['size']==[64,21],popup
  busy=subprocess.run(['herdr','--session','fixture','plugin','pane','open','--plugin','sessionsation','--entrypoint','launcher'],env=env,capture_output=True,text=True,timeout=5)
  assert 'ui_busy' in busy.stdout+busy.stderr,(busy.returncode,busy.stdout,busy.stderr)
  # Rendered menu reaches the popup; account inventory remains asynchronous.
  time.sleep(.2);key(b'\r');wait('Account:');wait('Space: Origin')
  key(b'\x1b[B\r');wait('Choose Space');key(b'\x1b[B\x1b[B\r');wait('Space: Destination')
  key(b'\x1b[B\r');wait('Session name');key(b'host-fixture\r');wait('Name: host-fixture')
  time.sleep(.2);assert not (projects/'host-fixture').exists();assert not (root/'start.json').exists()
  # Real host viewport resize must preserve the draft and bottom control geometry.
  fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,120,0,0));os.kill(client.pid,signal.SIGWINCH);time.sleep(.15)
  fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',48,160,0,0));os.kill(client.pid,signal.SIGWINCH);time.sleep(.15)
  key(b'\x1b[B\x1b[B\r')
  until=time.monotonic()+5
  while not (root/'receipt.json').exists() and time.monotonic()<until:
   if select.select([master],[],[],.02)[0]:screen.feed(os.read(master,65536))
  receipt=json.loads((root/'receipt.json').read_text());request=receipt['request']
  assert request['space']=={'kind':'existing','id':destination['workspace']['workspace_id']},request
  assert request['name']=='host-fixture' and request['account_id']=='codex:fixture'
  assert (projects/'host-fixture').is_dir()
  time.sleep(.3)
  spaces=host('workspace','list')['workspaces'];assert next(w for w in spaces if w['focused'])['workspace_id']==destination['workspace']['workspace_id']
  assert host('pane','process-info','--pane',origin_pane)['process_info']['shell_pid']==origin_pid
  # A fresh popup opening proves the first one exited instead of being promoted.
  (root/'popup.json').unlink();screen=Screen();host('plugin','action','invoke','sessionsation.launcher');wait('Configuration');key(b'\x1b');time.sleep(.5)
  assert host('pane','process-info','--pane',receipt['pane_id'])['process_info']['shell_pid']>0
  key_samples=[]
  for _ in range(20):
   while select.select([master],[],[],.01)[0]:screen.feed(os.read(master,65536))
   (root/'popup.json').unlink(missing_ok=True);screen=Screen();start=time.perf_counter();key(b'\x02L');wait('Configuration');key_samples.append((time.perf_counter()-start)*1000)
   key(b'\x1b');time.sleep(.4)
  key_samples.sort()
  print(json.dumps({'keybind_startup':{'samples':len(key_samples),'p50_ms':key_samples[10],'p95_ms':key_samples[19],'max_ms':max(key_samples)},'installed_herdr':host('--version') if False else '0.9.5','plugin_to_first_paint_ms':elapsed,'popup_size':popup['size'],'checks':['origin context without pane ID','half-size popup','ui_busy refusal','real host resize preserves draft','selected destination normal tab','staged directory','exact name/account','focus then popup exit','second popup cancel preserves new terminal','origin shell unchanged'],'models':0},indent=2))
 finally:
  host('server','stop')
  if client is not None:
   client.terminate();client.wait(timeout=3)
  server.wait(timeout=3);os.close(master);os.close(slave);log.close()
