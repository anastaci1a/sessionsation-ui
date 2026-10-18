import {apply,rollback} from '../src/config.mjs';
import {fileURLToPath} from 'node:url';
import {homedir} from 'node:os';
const [command,...args]=process.argv.slice(2);
try {
  if(command==='rollback' && args.length===1) { await rollback(args[0]); console.log('Original configuration restored; reload the selected HERDR server if needed.'); }
  else if(command==='apply') {
    const opts={clientConfig:homedir()+'/.config/sessionsation-ui/config.json',shell:fileURLToPath(new URL('../bin/sessionsation-shell',import.meta.url))};
    const keys={'--config':'config','--client-config':'clientConfig','--binary':'binary','--data-dir':'dataDir','--backup-dir':'backupDir'};
    for(let i=0;i<args.length;i+=2){if(!keys[args[i]] || !args[i+1]) throw new Error('Invalid configure arguments');opts[keys[args[i]]]=args[i+1];}
    console.log(await apply(opts));
  } else throw new Error('Usage: node scripts/configure.mjs apply --config ABS --binary ABS --data-dir ABS --backup-dir ABS [--client-config ABS] | rollback RECEIPT');
} catch(e) { console.error(e.message);process.exitCode=1; }
