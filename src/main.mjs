import {createInterface} from 'node:readline';
import {Client,executePlan,targetArgs,display,providerLabel,providerOf,addableProviders,filterAccounts,accountLabel,loginCommand} from './client.mjs';

const mode=process.argv[2]??'launcher';
if (!['launcher','manage'].includes(mode)) { console.error('Usage: sessionsation-ui launcher|manage'); process.exit(2); }
const lines=createInterface({input:process.stdin,crlfDelay:Infinity});
const input=lines[Symbol.asyncIterator]();
const print=s=>console.log(display(s));
async function ask(question) { process.stdout.write(`${display(question)} `); const r=await input.next(); return r.done ? null : r.value.trim(); }
async function choose(title,items,label=x=>x.name??x.id) {
  print(title); items.forEach((x,i)=>print(`${i+1}. ${label(x)}`)); print('0. Back');
  const value=await ask('Choice:');
  if (!value || !/^\d+$/.test(value)) return null;
  return items[Number(value)-1]??null;
}
function show(value) { console.log(JSON.stringify(value,null,2).replace(/\u001b/g,'')); }
async function confirm(question) { return (await ask(`${question} [yes/NO]`))==='yes'; }
const client=new Client();
async function launch() {
  while(true) {
    const action=await choose('sessionsation launcher',['new','resume','shell','exit'],x=>x);
    if (!action || action==='exit') return;
    let args=['launcher',action];
    if (action!=='shell') {
      if (action==='new') {
        const all=(await client.call(['account','list'])).filter(a=>a.selectable);
        const providers=[...new Set(all.map(providerOf))].sort();
        let provider=null;
        if (providers.length>1) {
          const picked=await choose('New session: provider',['all',...providers],p=>p==='all'?'All':providerLabel(p));
          if (!picked) continue;
          provider=picked==='all'?null:picked;
        }
        const a=await choose('New session: account',filterAccounts(all,provider),accountLabel);
        if (!a) continue; args.push('--account',a.id);
      } else {
        const state=await client.call(['state']);
        const s=await choose('Resume saved session',state.sessions.filter(s=>s.capabilities?.resume),s=>`${s.name} [${s.account_id}]`);
        if (!s) continue; args.push(...targetArgs(s));
      }
    }
    const plan=await client.call(args); lines.close(); executePlan(plan); return;
  }
}
async function sessionMenu(state) {
  const s=await choose('Select session',state.sessions,s=>`${s.name} [${s.account_id}] ${s.availability}`);
  if (!s) return;
  const choices=['inspect'];
  for (const [key,action] of [['rename','rename'],['switch_account','switch'],['restart','restart'],['stop','stop']])
    if (s.capabilities?.[key]) choices.push(action);
  const action=await choose(s.name,choices,x=>x);
  if (!action) return;
  if (action==='inspect') return show(s);
  const args=[action,...targetArgs(s)];
  if (action==='rename') { const name=await ask('Unique name:'); if (!name) return; args.push('--name',name); }
  if (action==='switch') {
    const a=await choose('Destination account',state.accounts.filter(a=>a.selectable && a.id!==s.account_id),a=>a.id);
    if (!a) return; args.push('--to',a.id);
  }
  if (!await confirm(`${action} selected session ${s.name}?`)) return;
  show(await client.call(args)); print('Operation admitted; inspect Operations for completion or confirmation.');
}
async function accountMenu(state) {
  const a=await choose('Select account',state.accounts,a=>`${accountLabel(a)} ${a.membership} ${a.enabled?'enabled':'disabled'}${a.auth_required?' login required':''}`);
  if (!a) return;
  const action=await choose(a.id,['inspect','usage',...(a.membership==='removed'?['restore']:['login',a.enabled?'disable':'enable','remove'])],x=>x);
  if (!action) return;
  if (action==='inspect') return show(a);
  if (action==='usage') { const end=Math.floor(Date.now()/1000); return show(await client.call(['account','usage',a.id,'--start',String(end-86400),'--end',String(end)])); }
  if (action==='remove') {
    const preview=await client.call(['account','remove',a.id]); show(preview);
    if (!preview.can_remove) { print('Account is in use; resolve active sessions/operations before removal.'); return; }
    const typed=await ask(`Removal preserves recoverable local history. Type ${a.id} to confirm:`);
    if (typed!==a.id) return;
    return show(await client.call(['account','remove',a.id,'--confirm',typed,'--revision',preview.revision]));
  }
  if (await confirm(`${action} ${a.id}?`)) show(await client.call(['account',action,a.id]));
}
async function operations() {
  const ops=await client.call(['operations']); show(ops);
  const op=await choose('Inspect operation',ops,x=>`${x.id} ${x.kind} ${x.phase}`);
  if (!op) return;
  const action=await choose('Explicit operation response',['back','respond','cancel-operation','recover'],x=>x);
  if (!action || action==='back') return;
  if (action==='respond') {
    const answer=await ask('Response yes or no:'); if (!['yes','no'].includes(answer)) return;
    show(await client.call(['respond',op.id,answer,'--sequence',String(op.confirmation_seq)]));
  } else if(await confirm(`${action} ${op.id}?`)) show(await client.call([action,op.id]));
}
async function manage() {
  while(true) {
    try {
      const state=await client.call(['state']);
      print(`sessionsation management — ${state.account_source}; snapshot ${state.observed_at}`);
      const action=await choose('Management',['accounts','sessions','operations','enroll','capabilities','exit'],x=>x);
      if (!action || action==='exit') return;
      if (action==='accounts') await accountMenu(state);
      if (action==='sessions') await sessionMenu(state);
      if (action==='operations') await operations();
      if (action==='capabilities') show(await client.call(['capabilities']));
      if (action==='enroll') {
        const caps=await client.call(['capabilities']);
        const providers=addableProviders(caps);
        const provider=providers.length>1?await choose('Provider',providers,providerLabel):providers[0]??'codex';
        const name=provider&&await ask(`${providerLabel(provider)} account name:`);
        const id=name&&`${provider}:${name}`;
        if (id && await confirm(`Ask backend to enroll ${id}?`)) {
          const added=await client.call(['account','add',id]);
          if (Array.isArray(added?.login?.argv)) print(`Run in a terminal to sign in: ${loginCommand(added.login.argv)}`);
          else show(added);
        }
      }
    } catch(e) { print(`${e.code??'UI_ERROR'}: ${e.message}`); const response=await ask('Press Enter to refresh, or q to exit:'); if(response===null || response==='q') return; }
  }
}
try { if(mode==='launcher') await launch(); else await manage(); }
catch(e) { print(`${e.code??'UI_ERROR'}: ${e.message}`); process.exitCode=1; }
finally { lines.close(); }
