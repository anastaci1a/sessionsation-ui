import {readFileSync,existsSync} from 'node:fs';
import {homedir} from 'node:os';
import {execFile} from 'node:child_process';
import {isAbsolute} from 'node:path';
import {promisify} from 'node:util';
const execute = promisify(execFile);
export const defaultClientConfig = () => existsSync('/mnt/user/appdata') ? '/mnt/user/appdata/sessionsation-ui/config.json' : homedir()+'/.config/sessionsation-ui/config.json';
export class BackendError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}
export class Client {
  constructor(options={}) {
    const configPath=process.env.SESSIONSATION_UI_CONFIG ?? defaultClientConfig();
    let stored={};
    if(existsSync(configPath)) { try { stored=JSON.parse(readFileSync(configPath,'utf8')); }
      catch { throw new BackendError('CONFIG_INVALID','Invalid UI configuration'); } }
    const binary=options.binary ?? process.env.SESSIONSATION_BIN ?? stored.binary;
    const dataDir=options.dataDir ?? process.env.SESSIONSATION_DATA_DIR ?? stored.dataDir;
    if (!binary || !isAbsolute(binary) || !dataDir || !isAbsolute(dataDir))
      throw new BackendError('CONFIG_REQUIRED', 'Set absolute SESSIONSATION_BIN and SESSIONSATION_DATA_DIR');
    this.binary=binary; this.dataDir=dataDir;
  }
  async call(args) {
    let stdout, stderr, failed=false;
    try { ({stdout,stderr}=await execute(this.binary,
      ['--data-dir',this.dataDir,'api','--api-version','1',...args],
      {encoding:'utf8',maxBuffer:8*1024*1024,timeout:60000})); }
    catch (e) { failed=true; stdout=e.stdout; stderr=e.stderr; }
    let body;
    try { body=JSON.parse(failed ? stderr : stdout); }
    catch { throw new BackendError('BACKEND_PROTOCOL', 'Backend returned no valid envelope; inspect backend before retrying'); }
    if (body.api_version !== 1 || typeof body.ok !== 'boolean')
      throw new BackendError('BACKEND_VERSION', 'Public CLI v1 required');
    if (!body.ok) throw new BackendError(body.error?.code ?? 'BACKEND_ERROR',body.error?.message ?? 'Backend error');
    if (failed) throw new BackendError('BACKEND_PROTOCOL','Success envelope on failed execution');
    return body.result;
  }
}
export function validatePlan(plan) {
  if (plan?.action !== 'exec' || plan.credentials_in_client !== false ||
      !Array.isArray(plan.argv) || !plan.argv.length ||
      !plan.argv.every(x=>typeof x==='string' && !x.includes('\0')) ||
      !isAbsolute(plan.argv[0]) || !(plan.cwd===null || (typeof plan.cwd==='string' && isAbsolute(plan.cwd))))
    throw new BackendError('INVALID_LAUNCH_PLAN','Backend returned an invalid execution plan');
  return plan;
}
export function executePlan(plan, {exec=process.execve, chdir=process.chdir, env=process.env}={}) {
  validatePlan(plan);
  if (plan.cwd !== null) chdir(plan.cwd);
  exec(plan.argv[0], plan.argv, {...env,SESSIONSATION_LAUNCHER_BYPASS:'1'});
}
export const targetArgs = s => ['--session',s.id,'--expected-generation',s.generation];
export const display = value => String(value??'').replace(/[\x00-\x1f\x7f-\x9f]/g,'');
export const PROVIDER_LABELS={codex:'Codex',claude:'Claude Code'};
export const providerOf=a=>a?.provider??String(a?.id??'').split(':')[0]??'codex';
export const providerLabel=p=>PROVIDER_LABELS[p]??display(p);
/** Providers the backend implements and this client has a flow for. */
export const addableProviders=caps=>(caps?.providers??[]).filter(p=>p.implemented===true&&p.id in PROVIDER_LABELS).map(p=>p.id);
export const filterAccounts=(accounts,provider)=>provider?accounts.filter(a=>providerOf(a)===provider):accounts;
const windowText=(w,duration)=>w&&w.duration_minutes===duration&&Number.isFinite(w.used_percent)&&w.used_percent>=0&&w.used_percent<=100
  ? `${Math.round(100-w.used_percent)}%` : 'unknown';
/** Remaining 5h/weekly usage; a missing window or unusable state is "unknown", never 0%. */
export function usageSummary(a) {
  const u=a?.usage??{}; const state=u.state??a?.usage_state;
  if (['missing','no_windows','error'].includes(state)||!u.observed_at) return 'usage unavailable';
  return `5h ${windowText(u.short,300)} / wk ${windowText(u.weekly,10080)}${state==='stale'||a.usage_fresh===false?' (stale)':''}`;
}
export const accountLabel=a=>`${a.id}${typeof a.display_name==="string"&&a.display_name?` "${a.display_name.replace(/[\u0000-\u001f\u007f]/g,"")}"`:""} [${providerLabel(providerOf(a))}] ${usageSummary(a)}`;
/** Shell-quoted, control-free rendering of a backend exec plan; the UI never runs it. */
export const loginCommand=argv=>argv.map(x=>{const s=display(x); return /^[A-Za-z0-9_./:=@%+,-]+$/.test(s)?s:`'${s.replaceAll("'","'\\''")}'`;}).join(' ');
