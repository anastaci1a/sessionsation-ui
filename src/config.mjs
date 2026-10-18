import {readFile,writeFile,mkdir,rename,lstat,unlink,chmod,chown} from 'node:fs/promises';
import {createHash,randomUUID} from 'node:crypto';
import {isAbsolute,dirname,join} from 'node:path';
const hash=x=>createHash('sha256').update(x).digest('hex');
async function snapshot(path,optional=false) {
  if(!isAbsolute(path)) throw new Error('Configuration paths must be absolute');
  let st; try { st=await lstat(path); } catch(e) { if(e.code==='ENOENT' && optional) return {path,exists:false}; throw e; }
  if(!st.isFile() || st.isSymbolicLink()) throw new Error('Configuration must be a regular file');
  const bytes=await readFile(path);return {path,exists:true,bytes:bytes.toString('base64'),hash:hash(bytes),mode:st.mode&0o777,uid:st.uid,gid:st.gid};
}
async function atomic(path,bytes,mode=0o600,owner) {
  await mkdir(dirname(path),{recursive:true,mode:0o700});
  const temp=path+'.sessionsation-'+randomUUID();
  try { await writeFile(temp,bytes,{flag:'wx',mode}); await chmod(temp,mode); if(owner?.exists) await chown(temp,owner.uid,owner.gid); await rename(temp,path); }
  finally { await unlink(temp).catch(e=>{if(e.code!=='ENOENT') throw e;}); }
}
export function updateShell(text,shell) {
  if(!isAbsolute(shell) || /[\r\n\0]/.test(shell)) throw new Error('Absolute shell path required');
  const lines=text.split('\n');let terminal=false,sections=0,keys=0,insert=-1;
  for(let i=0;i<lines.length;i++){
    if(/^\s*\[/.test(lines[i])) {
      if(terminal && insert<0) insert=i;
      terminal=/^\s*\[terminal\]\s*(#.*)?$/.test(lines[i]); if(terminal) sections++;
    }
    if(terminal && /^\s*default_shell\s*=/.test(lines[i])) { lines[i]=`default_shell = ${JSON.stringify(shell)}`; keys++; }
  }
  if(sections>1 || keys>1) throw new Error('Ambiguous terminal configuration');
  if(!sections) return text.replace(/\n?$/,'\n')+`\n[terminal]\ndefault_shell = ${JSON.stringify(shell)}\n`;
  if(!keys) lines.splice(insert<0?lines.length:insert,0,`default_shell = ${JSON.stringify(shell)}`);
  return lines.join('\n');
}
export async function apply({config,clientConfig,binary,dataDir,shell,backupDir}) {
  for(const p of [config,clientConfig,binary,dataDir,shell,backupDir]) if(!isAbsolute(p)) throw new Error('Absolute paths required');
  return applyPair({config,clientConfig,binary,dataDir,backupDir,rewrite:text=>updateShell(text,shell)});
}
export function updatePopup(text,key,shell) {
  if(!/^prefix\+(?:(?:shift|alt|ctrl)\+)*[a-z0-9]$/.test(key)) throw new Error('Choose an explicit prefix key binding');
  const blocks=[...text.matchAll(/^\s*\[\[keys\.command\]\]\s*(?:#.*)?$/gm)];
  const owned=[];
  for(const match of blocks) {
    const start=match.index;
    const following=text.slice(start+match[0].length).search(/^\s*\[/m);
    const end=following<0?text.length:start+match[0].length+following;
    const block=text.slice(start,end);
    const get=name=>{const m=block.match(new RegExp('^\\s*'+name+'\\s*=\\s*(["\'])(.*?)\\1\\s*(?:#.*)?$','m'));return m?.[2];};
    const command=get('command');
    if(get('key')===key && command!=='sessionsation.launcher') throw new Error('Key binding already belongs to another command');
    if(command==='sessionsation.launcher') owned.push({start,end});
  }
  if(owned.length>1) throw new Error('Ambiguous launcher bindings');
  const entry=`[[keys.command]]\nkey = ${JSON.stringify(key)}\ntype = "plugin_action"\ncommand = "sessionsation.launcher"\ndescription = "New session"\n`;
  let result=owned.length?text.slice(0,owned[0].start)+entry+text.slice(owned[0].end):text.replace(/\n?$/,'\n')+'\n'+entry;
  if(shell) result=updateShell(result,shell);
  return result;
}
export async function applyPopup({config,clientConfig,binary,dataDir,backupDir,key,shell}) {
  for(const p of [config,clientConfig,binary,dataDir,backupDir]) if(!isAbsolute(p)) throw new Error('Absolute paths required');
  return applyPair({config,clientConfig,binary,dataDir,backupDir,rewrite:text=>updatePopup(text,key,shell)});
}
async function applyPair({config,clientConfig,binary,dataDir,backupDir,rewrite}) {
  const before=[await snapshot(config),await snapshot(clientConfig,true)];
  const after=[Buffer.from(rewrite(Buffer.from(before[0].bytes,'base64').toString('utf8'))),Buffer.from(JSON.stringify({binary,dataDir},null,2)+'\n')];
  if(before[0].hash===hash(after[0])) throw new Error('Launcher already configured; preserve its existing rollback receipt');
  const receipt={version:1,files:before.map((b,i)=>({...b,appliedHash:hash(after[i])}))};
  await mkdir(backupDir,{recursive:true,mode:0o700});
  const receiptPath=join(backupDir,'launcher-'+randomUUID()+'.json');
  await writeFile(receiptPath,JSON.stringify(receipt,null,2)+'\n',{flag:'wx',mode:0o600});
  // Refuse concurrent changes before writing either file.
  for(const b of before) { const now=await snapshot(b.path,true); if(now.exists!==b.exists || now.hash!==b.hash) throw new Error('Configuration changed; no changes applied'); }
  try { await atomic(clientConfig,after[1],before[1].mode,before[1]); await atomic(config,after[0],before[0].mode,before[0]); }
  catch(e) { await rollback(receiptPath,true); throw e; }
  return receiptPath;
}
export async function rollback(path,partial=false) {
  const receipt=JSON.parse(await readFile(path,'utf8'));
  if(receipt.version!==1 || !Array.isArray(receipt.files) || receipt.files.length!==2) throw new Error('Invalid rollback receipt');
  for(const f of receipt.files) {
    const now=await snapshot(f.path,true);
    if(now.hash!==f.appliedHash && !(partial && now.hash===f.hash && now.exists===f.exists))
      throw new Error('Configuration changed since apply; refusing to overwrite it');
  }
  for(const f of receipt.files) {
    if(f.exists) await atomic(f.path,Buffer.from(f.bytes,'base64'),f.mode,f);
    else await unlink(f.path).catch(e=>{if(e.code!=='ENOENT') throw e;});
  }
}
