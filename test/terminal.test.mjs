import test from 'node:test';
import assert from 'node:assert/strict';
import {EventEmitter} from 'node:events';
import {PassThrough} from 'node:stream';
import {mkdtemp,mkdir,symlink,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {render,layout,diffFrame,cellWidth,clip,Terminal,Cancelled,CANCELLED} from '../src/terminal.mjs';
import {DirectoryPicker,runLauncher} from '../src/launcher.mjs';

const screen={title:'sessionsation',items:[{label:'Codex: New session'},{label:'Resume saved session'},{label:'Plain shell'}],selected:0,home:true};
const row=(frame,y,cols)=>frame.cells.slice(y*cols,(y+1)*cols).map(c=>c.char).join('');
test('stars and gradient use full terminal height, independent of menu length',()=>{
  for(const rows of [12,24,60]) {
    const f=render(screen,{cols:100,rows,elapsed:2100});
    const long=render({...screen,items:[...screen.items,...Array(20).fill({label:'More'})]},{cols:100,rows,elapsed:2100});
    const y=rows-4;
    assert.ok(f.cells.slice(y*100,(y+1)*100).some(c=>'.+*'.includes(c.char)));
    assert.deepEqual(f.cells.slice(y*100,y*100+5),long.cells.slice(y*100,y*100+5));
    assert.match(row(f,rows-1,100),/Esc: Back/);
    assert.equal(f.geometry.backRow,rows-2);
    assert.doesNotMatch(f.cells.map(c=>c.char).join(''),/####/);
    assert.ok(f.cells.some(c=>c.char==='●'&&c.bold));
  }
});
test('shimmer and shared hue cycle change stars and options without shifting geometry',()=>{
  const frames=[0,600,1500,4200].map(elapsed=>render(screen,{cols:80,rows:32,elapsed}));
  assert.deepEqual(frames[0].geometry,frames[3].geometry);
  const marker=f=>f.cells.find(c=>c.char==='●');
  assert.notDeepEqual(marker(frames[0]).fg,marker(frames[3]).fg);
  assert.ok(frames[0].cells.some((c,i)=>c.char!==frames[1].cells[i].char||String(c.fg)!==String(frames[1].cells[i].fg)));
});
test('resize clips Unicode safely and scrolls long lists around the selected row',()=>{
  assert.equal(cellWidth('資料 é'),6);assert.equal(clip('資料 abc',5),'資料 ');
  assert.equal(cellWidth('◀—'),2);assert.equal(cellWidth('🌟'),2);
  const many={...screen,items:Array.from({length:80},(_,i)=>({label:`資料 ${i}`})),selected:79};
  for(const [cols,rows] of [[22,9],[40,14],[100,40],[8,3]]) {
    const f=render(many,{cols,rows});assert.equal(f.cells.length,cols*rows);
    if(!f.geometry.compact)assert.ok(f.geometry.first+f.geometry.visible>79);
    else assert.ok(!f.cells.some(c=>c.char==='●'));
  }
});
test('frame diff repairs cells, has no scrolling clears, and avoids unchanged output',()=>{
  const opts={cols:80,rows:24,elapsed:2000,clock:new Date(0)};
  const a=render(screen,opts),b=render({...screen,selected:1},opts);
  assert.equal(diffFrame(a.cells,a.cells,80),'');
  const delta=diffFrame(b.cells,a.cells,80);
  assert.ok(delta.length<2500);assert.doesNotMatch(delta,/\x1b\[(2J|H)|\n/);
});
test('ANSI emission matches rendered cells with terminal autowrap disabled',()=>{
  const cols=80,rows=24;
  const expected=render(screen,{cols,rows,elapsed:2000,clock:new Date(0)});
  const actual=Array(cols*rows).fill(' ');let x=0,y=0;
  const apply=output=>{
    for(let p=0;p<output.length;) {
      const escape=/^\x1b\[([\d;]*)([Hm])/.exec(output.slice(p));
      if(escape){if(escape[2]==='H'){const [r,c]=escape[1].split(';').map(Number);y=r-1;x=c-1;}p+=escape[0].length;continue;}
      const char=String.fromCodePoint(output.codePointAt(p));p+=char.length;
      actual[y*cols+x]=char;if(cellWidth(char)===2)actual[y*cols+x+1]='';x=Math.min(cols-1,x+cellWidth(char));
    }
  };
  const full=diffFrame(expected.cells,null,cols);apply(full);
  for(let r=1;r<=rows;r++)assert.ok(full.includes(`\x1b[${r};1H`),`row ${r} must be addressed explicitly`);
  assert.deepEqual(actual,expected.cells.map(c=>c.char));
  const selected=render({...screen,selected:1},{cols,rows,elapsed:2050,clock:new Date(0)});
  apply(diffFrame(selected.cells,expected.cells,cols));
  assert.deepEqual(actual,selected.cells.map(c=>c.char));
});

function terminal(t) {
  const input=new PassThrough();input.isRaw=false;input.setRawMode=v=>{input.isRaw=v;};
  const output=new EventEmitter();Object.assign(output,{columns:80,rows:24,bytes:'',write(s){this.bytes+=s;return true;}});
  const signals=new EventEmitter();const ui=new Terminal({input,output,signals,now:()=>2000});
  t.after(()=>ui.close());const key=(name,text='',extra={})=>input.emit('keypress',text,{name,...extra});
  return {ui,input,output,signals,key};
}
test('arrows, Space, Back focus, disabled choices and resize preserve selection',async t=>{
  const {ui,key,output}=terminal(t);
  const items=[{id:'disabled',label:'unavailable',disabled:true},{id:'a',label:'One'},{id:'b',label:'Two'}];
  let p=ui.choose('Choose',items);assert.equal(ui.screen.selected,1);
  key('up');assert.equal(ui.screen.selected,2);
  output.columns=32;output.rows=12;output.emit('resize');assert.equal(ui.screen.selected,2);
  key('left');assert.equal(ui.screen.back,true);key('right');key('space',' ');
  assert.equal((await p).item.id,'b');
  p=ui.choose('Choose',items);key('left');key('return');assert.equal(await p,null);
});
test('text cursor editing and clear work without treating arrows as text',async t=>{
  const {ui,key}=terminal(t);let p=ui.text('Path','ab');
  key('left');key('x','x');key('end');key('backspace');key('return');assert.equal(await p,'ax');
  p=ui.text('Path','old');key('u','',{ctrl:true});key('x','資料');key('return');assert.equal(await p,'資料');
});
test('Escape and Ctrl-C cancel, including pending reads; raw mode/listeners restore',async t=>{
  const {ui,key,input,output,signals}=terminal(t);
  let aborted=false,calls=0;
  const p=ui.wait('Pending',signal=>{calls++;signal.addEventListener('abort',()=>{aborted=true;});return new Promise(()=>{});});
  await Promise.resolve();key('escape');assert.equal(await p,CANCELLED);assert.equal(aborted,true);assert.equal(calls,1);
  const skipped=ui.wait('Cancelled before read',()=>{calls++;});key('escape');
  assert.equal(await skipped,CANCELLED);assert.equal(calls,1);
  const q=ui.choose('Cancel',[]);key('c','',{ctrl:true});await assert.rejects(q,Cancelled);
  ui.close();assert.equal(input.isRaw,false);assert.equal(input.listenerCount('keypress'),0);
  assert.equal(output.listenerCount('resize'),0);assert.equal(signals.listenerCount('SIGTERM'),0);
  assert.match(output.bytes,/\x1b\[\?25h\x1b\[\?1049l$/);
});
test('signal and EOF cancel prompts and output backpressure does not block keys',async t=>{
  for(const signal of ['SIGTERM','end']) {
    const {ui,key,input,output,signals}=terminal(t);const p=ui.choose('Cancel',screen.items);
    output.write=()=>false;ui.paint();key('down');assert.equal(ui.screen.selected,1);
    if(signal==='end')input.emit('end');else signals.emit(signal);
    await assert.rejects(p,Cancelled);ui.close();assert.equal(input.isRaw,false);
  }
});
test('queued keys survive page transitions and reads, while paste cannot select actions',async t=>{
  const {ui,input,key}=terminal(t);
  let p=ui.choose('First',[{id:'first',label:'First'}]);
  input.write('\r\x1b[B\r');
  assert.equal((await p).item.id,'first');
  await ui.wait('Read',async()=>[]);
  p=ui.choose('Second',[{id:'a',label:'A'},{id:'b',label:'B'}]);
  assert.equal((await p).item.id,'b');
  p=ui.choose('No pasted actions',[{id:'x',label:'X'}]);
  key('paste-start');key('return','\r');key('space',' ');key('paste-end');
  assert.equal(typeof ui.onKey,'function');key('escape');assert.equal(await p,null);
  p=ui.text('Paste');key('paste-start');key('a','hello');key('return','\n');key('b',' world');key('paste-end');key('return');
  assert.equal(await p,'hello world');
});
test('directory picker browses literal paths, symlinks and selection history',async t=>{
  const root=await mkdtemp(join(tmpdir(),'sessionsation-picker-'));t.after(()=>rm(root,{recursive:true,force:true}));
  await mkdir(join(root,'a ; $(literal)'));await mkdir(join(root,'資料'));await symlink(join(root,'資料'),join(root,'link'));
  const p=new DirectoryPicker(root);let entries=await p.entries();
  const index=entries.findIndex(x=>x.name==='a ; $(literal)');await p.visit(join(root,'a ; $(literal)'),index);
  p.selected=1;await p.parent(1);assert.equal(p.selected,index);
  await p.visit(join(root,'a ; $(literal)'),index);assert.equal(p.selected,1);
  await p.typed('../link');assert.equal(p.path,join(root,'資料'));
  await assert.rejects(p.create('../unsafe'));await p.create('new space');assert.equal(p.path,join(root,'資料/new space'));
  await assert.rejects(p.typed('/definitely/not/a/directory'));
});

class ScriptedUI {
  constructor(choices){this.choices=choices;this.closed=false;this.titles=[];}
  async choose(title,items){this.titles.push(title);const id=this.choices.shift();if(id===null)return null;const index=items.findIndex(x=>x.id===id);assert.ok(index>=0,`${title}: ${id}`);return {item:items[index],index};}
  async wait(_title,fn){return fn(new AbortController().signal);}
  close(){this.closed=true;}
}
const plan={action:'exec',credentials_in_client:false,argv:['/bin/echo','literal ; value'],cwd:null};
test('New chooses directory/account and preserves opaque argv with no state scan',async()=>{
  const ui=new ScriptedUI(['new','select','codex:pro']),calls=[];
  const result=await runLauncher({async call(args){calls.push(args);return args[0]==='account'?[{id:'codex:pro',selectable:true,plan:'pro'}]:plan;}},{ui,cwd:tmpdir()});
  assert.deepEqual(calls,[['account','list'],['launcher','new','--account','codex:pro']]);
  assert.deepEqual(result,{plan,directory:tmpdir()});assert.equal(ui.closed,true);
  assert.ok(!ui.titles.includes('Management'));
});
test('Resume retains backend cwd, stable ID and captured generation',async()=>{
  const ui=new ScriptedUI(['resume','stable']),calls=[];
  const result=await runLauncher({async call(args){calls.push(args);return args[0]==='state'?{sessions:[{id:'stable',name:'Saved',account_id:'codex:pro',generation:'generation',availability:'saved',capabilities:{resume:true}}]}:{...plan,cwd:'/backend-project'};}},{ui});
  assert.deepEqual(calls[1],['launcher','resume','--session','stable','--expected-generation','generation']);
  assert.equal(result.directory,null);assert.equal(result.plan.cwd,'/backend-project');
});
test('cancelled New makes no provider calls; errors never retry launcher plans',async()=>{
  let calls=0;let ui=new ScriptedUI(['new',null,null]);
  assert.equal(await runLauncher({call(){calls++;}},{ui,cwd:tmpdir()}),null);assert.equal(calls,0);
  ui=new ScriptedUI(['shell','back',null]);
  assert.equal(await runLauncher({call(){calls++;throw new Error('uncertain');}},{ui}),null);assert.equal(calls,1);
});
