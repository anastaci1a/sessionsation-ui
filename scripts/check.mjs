import {readdir, readFile, stat} from 'node:fs/promises';
import {resolve, dirname} from 'node:path';
import {execFileSync} from 'node:child_process';
async function walk(dir) {
  for (const item of await readdir(dir, {withFileTypes:true})) {
    if (item.name === '.git' || item.name === 'node_modules') continue;
    const path = resolve(dir, item.name);
    if (item.isDirectory()) await walk(path);
    else if (path.endsWith('.mjs')) execFileSync(process.execPath, ['--check', path]);
    else if (path.endsWith('.md')) {
      const text = await readFile(path, 'utf8');
      for (const [,link] of text.matchAll(/\]\(([^)]+)\)/g)) {
        if (/^(https?:|#)/.test(link)) continue;
        await stat(resolve(dirname(path), link.split('#')[0]));
      }
    }
  }
}
await walk(process.cwd());
for (const path of ['bin/sessionsation-ui','bin/sessionsation-shell','share/launcher.rc','share/host-environment.sh']) execFileSync('/bin/bash',['-n',path]);
const manifest=await readFile('herdr-plugin.toml','utf8');
for (const key of ['id','name','version','min_herdr_version']) if(!new RegExp('^'+key+' = "[^"\\n]+"','m').test(manifest)) throw new Error('Missing manifest '+key);
if(!manifest.includes('command = ["./bin/sessionsation-ui", "manage"]')) throw new Error('Management pane must invoke the viewer');
console.log('JavaScript/Bash syntax, manifest fields and local documentation links passed');
