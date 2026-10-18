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
console.log('Syntax and local documentation links passed');
