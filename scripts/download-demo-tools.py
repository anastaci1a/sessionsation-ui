"""Download only checksum-pinned public demo tools, never user configuration."""
import argparse, hashlib, json, pathlib, platform, urllib.request
p = argparse.ArgumentParser()
p.add_argument('--output', required=True, type=pathlib.Path)
a = p.parse_args()
if platform.system() != 'Linux' or platform.machine() != 'x86_64':
    raise SystemExit('The pinned demo currently supports Linux x86-64 only.')
a.output.mkdir(parents=True, exist_ok=True)
lock = json.loads(pathlib.Path(__file__).with_name('demo-tools.json').read_text())
for name in ['herdr', 'agg']:
    item = lock[name]
    with urllib.request.urlopen(item['url'], timeout=60) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != item['sha256']:
        raise SystemExit('Checksum mismatch: ' + name)
    target = a.output/name
    target.write_bytes(data)
    target.chmod(0o755)
    print(name + ' ' + item['version'] + ': checksum verified')
