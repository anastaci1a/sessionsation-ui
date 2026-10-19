import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,chmod,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {Client,executePlan,validatePlan,targetArgs,display} from '../src/client.mjs';
const run=promisify(execFile);
async function fixture(t,body) {
  const dir=await mkdtemp(join(tmpdir(),'sessionsation-ui-'));
  t.after(()=>rm(dir,{recursive:true,force:true}));
  const binary=join(dir,'backend');
  await writeFile(binary,`#!${process.execPath}\n${body}`); await chmod(binary,0o700);
  return {dir,binary,client:new Client({binary,dataDir:join(dir,'data')})};
}
test('v1 argv is literal, scoped and preserves metacharacters',async t=>{
  const f=await fixture(t,'console.log(JSON.stringify({api_version:1,ok:true,result:process.argv.slice(2)}));');
  const name='$(touch /tmp/no-ui-eval); `false`';
  assert.deepEqual(await f.client.call(['rename',...targetArgs({id:'stable',generation:'old'}),'--name',name]),
    ['--data-dir',join(f.dir,'data'),'api','--api-version','1','rename','--session','stable','--expected-generation','old','--name',name]);
});
test('structured stale error surfaces once without automatic mutation retry',async t=>{
  const f=await fixture(t,"console.error(JSON.stringify({api_version:1,ok:false,error:{code:'STALE_TARGET',message:'refresh'}}));process.exit(1);");
  await assert.rejects(f.client.call(['stop']),{code:'STALE_TARGET'});
});
test('rejects non-v1, malformed and contradictory envelopes',async t=>{
  for(const body of ["console.log('{}')", "console.log('not JSON')", "console.error(JSON.stringify({api_version:1,ok:true,result:{}}));process.exit(1)"]){
    const f=await fixture(t,body); await assert.rejects(f.client.call(['state']));
  }
});
test('opaque launch exec preserves argv and cwd, sets bypass',()=>{
  let got;
  const p={action:'exec',credentials_in_client:false,argv:['/backend','--name','a;echo bad'],cwd:'/project'};
  const calls=[]; executePlan(p,{chdir:x=>calls.push(x),exec:(...args)=>{got=args},env:{KEEP:'yes'}});
  assert.deepEqual(calls,['/project']); assert.deepEqual(got,['/backend',p.argv,{KEEP:'yes',SESSIONSATION_LAUNCHER_BYPASS:'1'}]);
  for(const bad of [{...p,argv:['relative']},{...p,cwd:'relative'},{...p,credentials_in_client:true},{...p,argv:['/bin/x','bad\0arg']}]) assert.throws(()=>validatePlan(bad));
});
test('viewer exit never submits a lifecycle operation',async t=>{
  const f=await fixture(t,"if(process.argv.at(-1)!=='state') process.exit(9); console.log(JSON.stringify({api_version:1,ok:true,result:{accounts:[],sessions:[],account_source:'fixture',observed_at:1}}));");
  const child=execFile(process.execPath,['src/main.mjs','manage'],{env:{...process.env,SESSIONSATION_BIN:f.binary,SESSIONSATION_DATA_DIR:join(f.dir,'data')}});
  child.stdin.end('6\n');
  await new Promise((resolve,reject)=>child.on('exit',code=>code===0?resolve():reject(new Error(`exit ${code}`))));
});
test('plain-shell launcher execs backend plan, not provider command',async t=>{
  const f=await fixture(t,"console.log(JSON.stringify({api_version:1,ok:true,result:{action:'exec',credentials_in_client:false,argv:['/bin/echo','PLAN_EXECUTED'],cwd:null}}));");
  const child=execFile(process.execPath,['src/main.mjs','launcher'],{env:{...process.env,SESSIONSATION_BIN:f.binary,SESSIONSATION_DATA_DIR:join(f.dir,'data')}},(e,stdout)=>{if(e)reject(e);else resolve(stdout)});
  let resolve,reject;const result=new Promise((a,b)=>{resolve=a;reject=b});child.stdin.end('3\n'); assert.match(await result,/PLAN_EXECUTED/);
});
test('display removes terminal control characters',()=>assert.equal(display('\x1bhello\r\n'),'hello'));
async function menuFixture(t,{canRemove=true}={}) {
  const state={accounts:[{id:'codex:source',name:'source',enabled:true,selectable:true,membership:'enrolled'},{id:'codex:dest',name:'dest',enabled:true,selectable:true,membership:'enrolled'}],sessions:[{id:'stable-id',name:'chosen',account_id:'codex:source',generation:'captured-generation',availability:'running',capabilities:{rename:true,switch_account:true,restart:true,stop:true}}],account_source:'fixture',observed_at:1};
  const f=await fixture(t,`const fs=require('node:fs');const args=process.argv.slice(7);fs.appendFileSync(__filename+'.calls',JSON.stringify(args)+'\\n');const result=args[0]==='state'?${JSON.stringify(state)}:args[0]==='account' && args[1]==='remove' && args.length===3?{can_remove:${canRemove},revision:'preview-revision'}:{phase:'queued'};console.log(JSON.stringify({api_version:1,ok:true,result}));`);
  async function menu(input) {
    return await new Promise((resolve,reject)=>{const child=execFile(process.execPath,['src/main.mjs','manage'],{env:{...process.env,SESSIONSATION_BIN:f.binary,SESSIONSATION_DATA_DIR:join(f.dir,'data')}},(e,stdout)=>e?reject(e):resolve(stdout));child.stdin.end(input);});
  }
  const calls=async()=> (await readFile(f.binary+'.calls','utf8')).trim().split('\n').map(JSON.parse);
  return {...f,menu,calls};
}
test('selected-session switch uses stable ID and captured generation, with explicit consent',async t=>{
  const f=await menuFixture(t);await f.menu('2\n1\n3\n1\nyes\n6\n');
  assert.deepEqual((await f.calls()).filter(a=>a[0]==='switch'),[['switch','--session','stable-id','--expected-generation','captured-generation','--to','codex:dest']]);
});
test('removal carries preview revision only after exact typed confirmation',async t=>{
  const f=await menuFixture(t);await f.menu('1\n1\n5\ncodex:source\n6\n');
  assert.deepEqual((await f.calls()).filter(a=>a[0]==='account'),[['account','remove','codex:source'],['account','remove','codex:source','--confirm','codex:source','--revision','preview-revision']]);
});
test('incorrect confirmation and active-account preview never submit removal',async t=>{
  for(const canRemove of [true,false]){
    const f=await menuFixture(t,{canRemove});await f.menu(canRemove?'1\n1\n5\nwrong\n6\n':'1\n1\n5\n6\n');
    assert.deepEqual((await f.calls()).filter(a=>a[0]==='account'),[['account','remove','codex:source']]);
  }
});
test('New account menu uses account list without global session discovery',async t=>{
  const f=await fixture(t,"const args=process.argv.slice(7);let result;if(args.join(' ')==='account list') result=[{id:'codex:example',selectable:true}];else if(args.join(' ')==='launcher new --account codex:example')result={action:'exec',credentials_in_client:false,argv:['/bin/echo','FAST_NEW_HANDOFF'],cwd:null};else{console.error('unexpected discovery '+args.join(' '));process.exit(9);}console.log(JSON.stringify({api_version:1,ok:true,result}));");
  const output=await new Promise((resolve,reject)=>{const child=execFile(process.execPath,['src/main.mjs','launcher'],{env:{...process.env,SESSIONSATION_BIN:f.binary,SESSIONSATION_DATA_DIR:join(f.dir,'data')}},(e,stdout)=>e?reject(e):resolve(stdout));child.stdin.end('1\n1\n');});
  assert.match(output,/FAST_NEW_HANDOFF/);
});
