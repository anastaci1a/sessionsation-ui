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
  const before=[await snapshot(config),await snapshot(clientConfig,true)];
  const after=[Buffer.from(updateShell(Buffer.from(before[0].bytes,'base64').toString('utf8'),shell)),Buffer.from(JSON.stringify({binary,dataDir},null,2)+'\n')];
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
