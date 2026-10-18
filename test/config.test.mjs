import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,readFile,rm,stat,symlink,chmod} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {apply,applyPopup,rollback,updateShell,updatePopup} from '../src/config.mjs';
async function fixture(t){const dir=await mkdtemp(join(tmpdir(),'sessionsation-config-'));t.after(()=>rm(dir,{recursive:true,force:true}));const config=join(dir,'config.toml'),clientConfig=join(dir,'ui.json');return {dir,opts:{config,clientConfig,binary:'/backend',dataDir:'/data',shell:'/ui/bin/shell',backupDir:join(dir,'backups')}};}
test('apply preserves config fields, backs up exact bytes and restores absent UI config',async t=>{
  const {opts}=await fixture(t);const original='onboarding = false\n\n[theme]\nname = "vesper"\n';
  await writeFile(opts.config,original,{mode:0o640});await chmod(opts.config,0o640);const receipt=await apply(opts);
  assert.match(await readFile(opts.config,'utf8'),/\[terminal\]\ndefault_shell = "\/ui\/bin\/shell"/);
  assert.equal((await stat(receipt)).mode&0o777,0o600);
  assert.deepEqual(JSON.parse(await readFile(opts.clientConfig)),{binary:'/backend',dataDir:'/data'});
  await rollback(receipt);assert.equal(await readFile(opts.config,'utf8'),original);assert.equal((await stat(opts.config)).mode&0o777,0o640);
  await assert.rejects(stat(opts.clientConfig),{code:'ENOENT'});
});
test('rollback refuses concurrent edits without touching either file',async t=>{
  const {opts}=await fixture(t);await writeFile(opts.config,'[terminal]\ndefault_shell = "/bin/bash"\n');await writeFile(opts.clientConfig,'{"previous":true}\n');
  const receipt=await apply(opts);const appliedClient=await readFile(opts.clientConfig,'utf8');await writeFile(opts.config,'# operator edit\n');
  await assert.rejects(rollback(receipt),/changed/);assert.equal(await readFile(opts.clientConfig,'utf8'),appliedClient);
});
test('rejects ambiguous sections and symlinked config',async t=>{
  assert.throws(()=>updateShell('[terminal]\n[terminal]\n','/shell'),/Ambiguous/);
  const {dir,opts}=await fixture(t);await writeFile(join(dir,'real'),'');await symlink(join(dir,'real'),opts.config);await assert.rejects(apply(opts),/regular/);
});
test('updates existing shell in place with TOML-safe quoted path',()=>{
  const updated=updateShell('[terminal]\ndefault_shell="old"\n[theme]\nname="x"\n','/with "quotes"/shell');
  assert.match(updated,/default_shell = "\/with \\"quotes\\"\/shell"/);assert.match(updated,/\[theme\]/);
});
test('popup binding preserves unrelated keys and prefix, and rollback restores both files',async t=>{
  const {opts}=await fixture(t);const original='onboarding=false\n[keys]\nprefix="ctrl+a"\n[[keys.command]]\nkey="prefix+x"\ntype="shell"\ncommand="example"\n[terminal]\ndefault_shell="/old/launcher"\n';
  await writeFile(opts.config,original);opts.key='prefix+shift+l';opts.shell='/bin/bash';
  const receipt=await applyPopup(opts);const applied=await readFile(opts.config,'utf8');
  assert.match(applied,/prefix="ctrl\+a"/);assert.match(applied,/key="prefix\+x"/);assert.match(applied,/key = "prefix\+shift\+l"/);assert.match(applied,/default_shell = "\/bin\/bash"/);
  await rollback(receipt);assert.equal(await readFile(opts.config,'utf8'),original);await assert.rejects(stat(opts.clientConfig),{code:'ENOENT'});
});
test('popup installer refuses occupied keys and only updates its own binding',()=>{
  const occupied='[[keys.command]]\nkey="prefix+shift+l"\ntype="shell"\ncommand="other"\n';
  assert.throws(()=>updatePopup(occupied,'prefix+shift+l'),/already belongs/);
  assert.throws(()=>updatePopup('',''),/explicit/);
  const once=updatePopup('','prefix+shift+l');const twice=updatePopup(once,'prefix+alt+n');
  assert.equal((twice.match(/\[\[keys.command\]\]/g)||[]).length,1);assert.match(twice,/prefix\+alt\+n/);assert.doesNotMatch(twice,/prefix\+shift\+l/);
});
