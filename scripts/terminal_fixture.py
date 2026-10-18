"""Isolated PTY acceptance. Fake public APIs; never invokes an agent or real HERDR."""
import argparse, codecs, fcntl, json, os, pathlib, pty, re, select, signal, struct, subprocess, tempfile, termios, time, unicodedata
ANSI=re.compile(rb'\x1b\[[0-9;?]*[A-Za-z]')
BACKEND='''#!/usr/bin/env python3
import json,os,sys,time,pathlib
args=sys.argv[1:];api=args[args.index('api')+1:];api=api[2:] if api[:1]==['--api-version'] else api
if api[:1] in [['account'],['capabilities']]: time.sleep(float(os.environ.get('FIXTURE_DELAY','0')))
if api==['account','list']: result=[{'id':'codex:fixture','selectable':True,'prefill':{'eligible':True,'valid_until':int(time.time())+60},'usage':{}}]
elif api==['capabilities']: result={'launch':{'version':1}}
elif api[:1]==['launch'] and api[1] in ['validate','start']:
 request=json.load(sys.stdin)
 if api[1]=='start': pathlib.Path(os.environ['FIXTURE_ROOT'],'start.json').write_text(json.dumps(request)); result={'phase':'running','tab_id':'w1:t2'}
 else: result={'valid':True}
else: result={'phase':'running','tab_id':'w1:t2'}
print(json.dumps({'api_version':1,'ok':True,'result':result}))
'''
HERDR='''#!/usr/bin/env python3
import json
print(json.dumps({'result':{'workspaces':[{'workspace_id':'w1','label':'Fixture'}]}}))
'''
class Screen:
 def __init__(self): self.grid=[[' ']*200 for _ in range(100)];self.x=0;self.y=0;self.pending='';self.decoder=codecs.getincrementaldecoder('utf8')('replace')
 def feed(self,data):
  self.pending+=self.decoder.decode(data)
  while self.pending:
   if self.pending[0]=='\x1b':
    if len(self.pending)<2:return
    if self.pending[1]=='[':
     match=re.match(r'\x1b\[([\x30-\x3f]*[\x20-\x2f]*)([@-~])',self.pending)
     if not match:return
     values=[int(x) if x.isdigit() else 0 for x in match[1].lstrip('?><=').split(';')];command=match[2]
     if command in ['H','f']:self.y=max(0,(values[0] or 1)-1);self.x=max(0,((values[1] if len(values)>1 else 1) or 1)-1)
     elif command=='J' and values[0]==2:self.grid=[[' ']*200 for _ in range(100)]
     self.pending=self.pending[match.end():];continue
    if self.pending[1] in ']P_^':
     end=re.search(r'\x07|\x1b\\',self.pending[2:])
     if not end:return
     self.pending=self.pending[2+end.end():];continue
    if self.pending[1] in '()':
     if len(self.pending)<3:return
     self.pending=self.pending[3:];continue
    self.pending=self.pending[2:];continue
   c=self.pending[0];self.pending=self.pending[1:]
   if c=='\r':self.x=0
   elif c=='\n':self.y+=1
   elif not unicodedata.combining(c):
    if self.y<100 and self.x<200:self.grid[self.y][self.x]=c
    self.x+=2 if unicodedata.east_asian_width(c) in ['W','F'] else 1
 def contains(self,text,selected=False):
  return any(text in ''.join(row) and (not selected or '●' in row) for row in self.grid)
class Terminal:
 def __init__(self,root,delay=0):
  self.root=root;self.master,self.slave=pty.openpty();fcntl.ioctl(self.slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,80,0,0));self.original=termios.tcgetattr(self.slave)
  env=dict(os.environ,TERM='xterm-256color',SESSIONSATION_BIN=str(root/'backend'),SESSIONSATION_DATA_DIR=str(root/'state'),HERDR_BIN_PATH=str(root/'herdr'),HERDR_SOCKET_PATH=str(root/'fake.sock'),HERDR_SESSION='fixture',HERDR_ACTIVE_WORKSPACE_ID='w1',HERDR_ACTIVE_PANE_CWD=str(root/'projects'),FIXTURE_ROOT=str(root),FIXTURE_DELAY=str(delay))
  self.start=time.perf_counter();self.process=subprocess.Popen([BINARY,'--preferences',str(root/'prefs.json'),'popup'],stdin=self.slave,stdout=self.slave,stderr=self.slave,env=env,start_new_session=True);self.data=b'';self.screen=Screen()
 def wait(self,text,timeout=4,selected=False):
  until=time.perf_counter()+timeout;chunk=b''
  while time.perf_counter()<until:
   if select.select([self.master],[],[],.02)[0]:
    data=os.read(self.master,65536);chunk+=data;self.data+=data;self.screen.feed(data)
    if self.screen.contains(text,selected): return (time.perf_counter()-self.start)*1000
   if self.process.poll() is not None: break
  raise AssertionError(f'No {text!r}; process={self.process.poll()}; output={ANSI.sub(b"",chunk)[-1000:]!r}')
 def drain(self):
  while select.select([self.master],[],[],.01)[0]:self.screen.feed(os.read(self.master,65536))
 def send(self,s):os.write(self.master,s)
 def close(self):
  if self.process.poll() is None:self.send(b'\x03')
  until=time.monotonic()+2
  while self.process.poll() is None and time.monotonic()<until:
   if select.select([self.master],[],[],.02)[0]:os.read(self.master,65536)
  if self.process.poll() is None:
   import sys
   print('cancel diagnostics', [(p.name,(p/'wchan').read_text()) for p in pathlib.Path('/proc',str(self.process.pid),'task').iterdir()], file=sys.stderr)
   print('original exception',repr(sys.exc_info()[1]), file=sys.stderr)
   os.killpg(self.process.pid,signal.SIGKILL);self.process.wait();raise AssertionError('UI did not cancel; output='+repr(ANSI.sub(b'',self.data)[-1000:]))
  assert termios.tcgetattr(self.slave)==self.original,'TTY state not restored'
  os.close(self.master);os.close(self.slave)
def fixture(root):
 (root/'projects').mkdir();(root/'state').mkdir()
 for name,text in [('backend',BACKEND),('herdr',HERDR)]:p=root/name;p.write_text(text);p.chmod(0o700)
 (root/'prefs.json').write_text(json.dumps({'version':1,'mode':'global','global_root':str(root/'projects'),'space_roots':{},'last_account':'codex:fixture'}))
