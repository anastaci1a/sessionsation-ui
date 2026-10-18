import test from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:net';
import {mkdtemp,mkdir,symlink,writeFile,readFile,chmod,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
const run=promisify(execFile);
const helper=resolve('share/host-environment.sh');
async function fixture(t) {
  const dir=await mkdtemp(join(tmpdir(),'ss-host-'));
  const home=join(dir,'home'), data=join(dir,'data');
  await mkdir(join(home,'.config'),{recursive:true});await mkdir(data);
  await symlink(data,join(home,'.config/herdr'));
  const socket=join(data,'herdr.sock');const server=createServer();
  await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(socket,resolve);});
  t.after(async()=>{await new Promise(resolve=>server.close(resolve));await rm(dir,{recursive:true,force:true});});
  const env={...process.env,HOME:home,HERDR_ENV:'1',HERDR_PANE_ID:'w7:p4',HERDR_SOCKET_PATH:join(home,'.config/herdr/herdr.sock')};
  for(const key of ['HERDR_SESSION','SESSIONSATION_HERDR_SESSION','XDG_CONFIG_HOME','SESSIONSATION_LAUNCHER_BYPASS','SESSIONSATION_SHELL_ACTIVE'])delete env[key];
  return {dir,home,data,socket,env};
}
async function normalized(env){return run('/bin/bash',['-c','source "$1"; sessionsation_host_environment || exit; printf "%s|%s" "${SESSIONSATION_HERDR_SESSION-}" "${HERDR_SESSION-}"','test',helper],{env});}
test('default HERDR missing session is resolved from real symlinked socket',async t=>{
  const f=await fixture(t);assert.equal((await normalized(f.env)).stdout,'default|');
});
test('named and explicit host contexts retain their exact session',async t=>{
  const f=await fixture(t);assert.equal((await normalized({...f.env,HERDR_SESSION:'named'})).stdout,'|named');assert.equal((await normalized({...f.env,SESSIONSATION_HERDR_SESSION:'explicit'})).stdout,'explicit|');
});
test('unknown socket and missing pane fail before any backend invocation',async t=>{
  const f=await fixture(t);
  for(const env of [{...f.env,HERDR_SOCKET_PATH:join(f.dir,'other.sock')},{...f.env,HERDR_PANE_ID:''}])await assert.rejects(normalized(env),e=>e.code===1&&e.stderr.includes('HERDR_HOST_CONTEXT'));
  const marker=join(f.dir,'backend-called');const backend=join(f.dir,'backend');await writeFile(backend,`#!/bin/bash\ntouch '${marker}'\n`);await chmod(backend,0o700);
  await assert.rejects(run(resolve('bin/sessionsation-ui'),['launcher'],{env:{...f.env,HERDR_SOCKET_PATH:join(f.dir,'other.sock'),SESSIONSATION_BIN:backend,SESSIONSATION_DATA_DIR:join(f.dir,'state')}}));
  await assert.rejects(readFile(marker),{code:'ENOENT'});
});
test('outside HERDR has no invented host context',async t=>{
  const f=await fixture(t);assert.equal((await normalized({...f.env,HERDR_ENV:''})).stdout,'|');
});
test('default retained shell new -> gate1 passes exact host context through plan exec',async t=>{
  const f=await fixture(t);const marker=join(f.dir,'launched.json'), backend=join(f.dir,'backend'), agent=join(f.dir,'fixture-agent');
  await writeFile(join(f.home,'.bashrc'),'');
  await writeFile(agent,`#!${process.execPath}\nconst fs=require('node:fs');fs.writeFileSync(${JSON.stringify(marker)},JSON.stringify({args:process.argv.slice(2),session:process.env.SESSIONSATION_HERDR_SESSION,pane:process.env.HERDR_PANE_ID}));`);await chmod(agent,0o700);
  await writeFile(backend,`#!${process.execPath}\nconst args=process.argv.slice(2);let result;if(args.slice(-2).join(' ')==='account list')result=[{id:'codex:gate1',selectable:true}];else{if(JSON.stringify(args.slice(-4))!==JSON.stringify(['launcher','new','--account','codex:gate1']))process.exit(9);result={action:'exec',credentials_in_client:false,argv:[${JSON.stringify(agent)},'--account','codex:gate1'],cwd:null};}console.log(JSON.stringify({api_version:1,ok:true,result}));`);await chmod(backend,0o700);
  const output=await new Promise((resolve,reject)=>{const child=execFile(resolvePath('bin/sessionsation-shell'),[],{env:{...f.env,SESSIONSATION_BIN:backend,SESSIONSATION_DATA_DIR:join(f.dir,'state')}},(e,stdout,stderr)=>e?reject(new Error(stderr)):resolve(stdout));child.stdin.end('1\n1\nexit\n');});
  assert.match(output,/codex:gate1/);assert.deepEqual(JSON.parse(await readFile(marker)),{args:['--account','codex:gate1'],session:'default',pane:'w7:p4'});
});
function resolvePath(path){return resolve(path);}
