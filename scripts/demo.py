"""Record real HERDR + popup in an owned synthetic host; no provider access.

Run in the documented network-disabled demo container. Output is a review
artifact; this script never updates the README, commits, or deploys anything.
"""
import argparse, codecs, fcntl, hashlib, json, os, pathlib, pty, re, select, shutil, signal, struct, subprocess, tempfile, termios, time
from collections import Counter
from terminal_fixture import Screen

p = argparse.ArgumentParser()
p.add_argument('--binary', required=True, type=pathlib.Path)
p.add_argument('--output', required=True, type=pathlib.Path)
a = p.parse_args()
binary = a.binary.resolve(strict=True)
output = a.output.resolve()
output.mkdir(parents=True, exist_ok=True)
for tool in ['herdr', 'agg']:
    if not shutil.which(tool): raise SystemExit('Missing pinned demo tool: '+tool)
preload = pathlib.Path('/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1')
if not preload.exists(): raise SystemExit('Install libfaketime; see docs/verification.md')
width, height = 128, 40
terminal_background = [40, 42, 54]
terminal_foreground = [248, 248, 242]
repo = pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='sessionsation-demo-') as tmp:
    root = pathlib.Path(tmp)
    home = root/'home'
    projects = home/'Projects'
    for name in ['api-review', 'design-notes', 'saved-draft', 'examples']:
        (projects/name).mkdir(parents=True)
    (projects/'examples/README.txt').write_text('Synthetic demo project. No real account or agent.\n')
    config = root/'herdr.toml'
    shell = root/'demo-shell'
    shell.write_text("#!/bin/sh\nexport PS1='demo $ '\nprintf '\\033[2J\\033[H  sessionsation demo\\n\\n  Synthetic workspace; no provider is running.\\n\\n'\nexec /bin/bash --noprofile --norc\n")
    shell.chmod(0o700)
    config.write_text('onboarding=false\n[theme.custom]\npanel_bg="#282a36"\n[terminal]\ndefault_shell='+json.dumps(str(shell))+'\n[[keys.command]]\nkey="prefix+shift+l"\ntype="plugin_action"\ncommand="sessionsation.launcher"\ndescription="Sessionsation"\n')
    backend = root/'synthetic-cli'
    backend.write_bytes((repo/'scripts/demo-backend.py').read_bytes())
    backend.chmod(0o700)
    prefs = root/'preferences.json'
    prefs.write_text(json.dumps({'version': 1, 'mode': 'global', 'global_root': str(projects), 'space_roots': {}, 'last_account': 'codex:personal'}))
    # Deliberately construct the environment; no inherited credential/config vars.
    env = {'HOME': str(home), 'USER': 'demo', 'LOGNAME': 'demo', 'HOSTNAME': 'demo-linux', 'PATH': os.environ['PATH'], 'LANG': 'C.UTF-8', 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor', 'XDG_CONFIG_HOME': str(root/'config'), 'XDG_STATE_HOME': str(root/'state'), 'XDG_DATA_HOME': str(root/'data'), 'HERDR_CONFIG_PATH': str(config), 'HERDR_SESSION': 'sessionsation-demo', 'SESSIONSATION_BIN': str(backend), 'SESSIONSATION_DATA_DIR': str(root/'backend-state'), 'FIXTURE_ROOT': str(root), 'LD_PRELOAD': str(preload), 'FAKETIME': '2026-01-01 12:00:00', 'FAKETIME_DONT_FAKE_MONOTONIC': '1', 'TZ': 'UTC', 'TOKIO_WORKER_THREADS': '2'}
    plugin = root/'plugin'
    plugin.mkdir()
    manifest = (repo/'herdr-plugin.toml').read_text().split('[[panes]]')[0]
    manifest = manifest.replace('["./bin/sessionsation-popup", "open"]', json.dumps([str(binary), 'open']))
    manifest += '\n[[panes]]\nid="launcher"\ntitle="New Session"\nplacement="popup"\nwidth="90%"\nheight="65%"\ncommand='+json.dumps([str(binary), '--preferences', str(prefs), 'popup'])+'\n'
    (plugin/'herdr-plugin.toml').write_text(manifest)
    def host(*args):
        r = subprocess.run(['herdr', '--session', 'sessionsation-demo', *args], env=env, capture_output=True, text=True, timeout=10)
        assert r.returncode == 0, (args, r.stderr)
        if not r.stdout.strip(): return {}
        data = json.loads(r.stdout)
        assert 'error' not in data, (args, data)
        return data.get('result', data)
    log = (root/'host.log').open('w')
    server = subprocess.Popen(['herdr', '--session', 'sessionsation-demo', 'server'], env=env, stdout=log, stderr=log)
    client = None
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
    screen, events = Screen(), []
    decoder = codecs.getincrementaldecoder('utf8')('replace')
    color_query_tail = bytearray()
    color_replies = {10: 0, 11: 0}
    start = time.monotonic()
    def pump(duration):
        until = time.monotonic()+duration
        while time.monotonic() < until:
            if select.select([master], [], [], min(.02, max(0, until-time.monotonic())))[0]:
                try: data = os.read(master, 65536)
                except OSError: return
                if not data: return
                # Act as the outer terminal, including its real default colors.
                # A PTY alone cannot answer HERDR's OSC queries; without these
                # replies Ghostty assumes black while agg renders blue-gray.
                color_query_tail.extend(data)
                queries = list(re.finditer(rb'\x1b\](10|11);\?(?:\x1b\\|\x07)', color_query_tail))
                for query in queries:
                    kind = int(query[1])
                    rgb = terminal_background if kind == 11 else terminal_foreground
                    reply = '\x1b]'+str(kind)+';rgb:'+'/'.join(f'{v:02x}{v:02x}' for v in rgb)+'\x1b\\'
                    os.write(master, reply.encode())
                    color_replies[kind] += 1
                if queries: del color_query_tail[:queries[-1].end()]
                if len(color_query_tail) > 64: del color_query_tail[:-64]
                screen.feed(data)
                events.append([round(time.monotonic()-start, 5), 'o', decoder.decode(data)])
    def wait(text, timeout=6):
        until = time.monotonic()+timeout
        while time.monotonic() < until:
            pump(.03)
            if screen.contains(text): return
        raise AssertionError('Demo did not reach '+repr(text)+'; screen='+repr([''.join(r[:width]).rstrip() for r in screen.grid[:height]]))
    def press(key):
        codes = {'down': b'B', 'up': b'A', 'right': b'C', 'left': b'D'}
        if key in codes:
            os.write(master, b'\x1b[1;1:1'+codes[key]+b'\x1b[1;1:3'+codes[key])
        else: os.write(master, {'enter': b'\r', 'esc': b'\x1b', 'home': b'\x1b[H', 'end': b'\x1b[F'}[key])
        pump(.45)
    def type_text(text):
        for char in text:
            os.write(master, char.encode())
            pump(.10)
        pump(.5)
    markers = []
    def mark(label): markers.append({'time': round(time.monotonic()-start, 2), 'workflow': label})
    try:
        socket = root/'config/herdr/sessions/sessionsation-demo/herdr.sock'
        for _ in range(100):
            if socket.exists(): break
            time.sleep(.03)
        assert socket.exists(), 'Owned demo host did not start'
        host('plugin', 'link', str(plugin))
        origin = host('workspace', 'create', '--label', 'Personal', '--cwd', str(projects))
        destination = host('workspace', 'create', '--label', 'Work', '--cwd', str(projects/'examples'))
        host('tab', 'focus', origin['tab']['tab_id'])
        client = subprocess.Popen(['herdr', '--session', 'sessionsation-demo'], env=env, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
        pump(.6)
        assert color_replies[11] > 0, 'Owned HERDR did not query the outer terminal background'
        os.write(master, b'\x02L')
        wait('personal'); wait('Settings'); pump(.4)
        # Move setup output to time zero; the first visible frame is the actual popup.
        first = ''.join(e[2] for e in events)
        events = [[0, 'o', first]]
        start = time.monotonic()
        mark('Home: real stars, shimmer and synthetic account bars'); pump(3)
        press('end'); press('up'); press('right'); pump(1.2); press('enter'); wait('Account limits'); mark('Usage focus and limits dialog'); pump(3); press('esc')
        press('home'); press('down'); press('enter'); wait('api-review'); mark('Manage Sessions: busy spinner, idle and saved history'); pump(3)
        press('enter'); wait('Session Details'); press('down'); press('enter'); wait('Saved history: 24576 bytes'); mark('Session details'); pump(3)
        press('esc'); press('esc'); press('esc'); wait('New Session'); press('home'); press('enter'); wait('Account:'); mark('New Session summary'); pump(2.5)
        press('down'); press('enter'); wait('Choose Space'); pump(2); press('down'); press('enter'); wait('Space:   Work'); pump(1.5)
        press('down'); press('enter'); wait('Session name'); pump(1.2); type_text('demo-notes'); press('enter'); wait('demo-notes')
        press('down'); press('enter'); wait('Start directory'); mark('Staged directory picker'); pump(3); press('esc')
        assert not (projects/'demo-notes').exists(), 'Picker created the directory early'
        press('down'); pump(.4); mark('Start Session: fields validated'); pump(2.5); press('enter')
        until = time.monotonic()+6
        while not (root/'receipt.json').exists() and time.monotonic() < until: pump(.05)
        receipt = json.loads((root/'receipt.json').read_text())
        assert receipt['request']['space']['id'] == destination['workspace']['workspace_id']
        assert receipt['request']['name'] == 'demo-notes' and (projects/'demo-notes').is_dir()
        until = time.monotonic()+6
        while time.monotonic() < until:
            pump(.08)
            focused = next((w for w in host('workspace', 'list')['workspaces'] if w['focused']), None)
            if focused and focused['workspace_id'] == destination['workspace']['workspace_id'] and not screen.contains('Start Session'): break
        else: raise AssertionError('Popup did not exit and focus its selected Space')
        assert host('pane', 'process-info', '--pane', receipt['pane_id'])['process_info']['shell_pid'] > 0
        mark('Normal-tab handoff: inert shell, no provider'); pump(3)
        host('tab', 'focus', origin['tab']['tab_id'])
        host('tab', 'close', receipt['tab_id'])
        pump(.3); os.write(master, b'\x02L'); wait('Settings'); wait('personal'); pump(.7)
        # Repeat the genuine initial terminal stream for a stable, clean loop seam.
        events.append([round(time.monotonic()-start, 5), 'o', '\x1b[0m\x1b[2J\x1b[H'+first])
        cast = output/'demo.cast'
        header = {'version': 2, 'width': width, 'height': height, 'timestamp': 1767268800, 'env': {'TERM': 'xterm-256color', 'SHELL': '/bin/bash'}, 'title': 'Synthetic sessionsation-ui in HERDR'}
        cast.write_text('\n'.join(json.dumps(e) for e in [header, *events])+'\n')
        subprocess.run(['agg', '--theme', 'dracula', '--text-font-family', 'DejaVu Sans Mono', '--font-size', '14', '--line-height', '1.25', '--fps-cap', '10', '--last-frame-duration', '1', str(cast), str(output/'demo.gif')], check=True)
        from PIL import Image, ImageOps, ImageDraw
        gif = Image.open(output/'demo.gif')
        frames = []
        duration_ms = 0
        for i in range(gif.n_frames):
            gif.seek(i)
            frames.append((duration_ms, gif.convert('RGB')))
            duration_ms += gif.info.get('duration', 0)
        assert Counter(frames[0][1].getdata()).most_common(1)[0][0] == tuple(terminal_background), 'Rendered background differs from OSC reply'
        assert gif.info.get('loop') == 0 and 30000 <= duration_ms <= 75000
        assert (output/'demo.gif').stat().st_size <= 3_000_000, 'Review GIF size budget'
        sheet = Image.new('RGB', (1280, 1200), '#111111')
        for i, k in enumerate([0, 1, 2, 5, 6, 7]):
            target = int((markers[k]['time']+.55)*1000)
            _, frame = min(frames, key=lambda item: abs(item[0]-target))
            frame = frame.copy(); frame.thumbnail((640, 400))
            sheet.paste(frame, ((i%2)*640, (i//2)*400))
        sheet.save(output/'contact-sheet.png')
        calls = [json.loads(line) for line in (root/'calls.jsonl').read_text().splitlines()]
        assert sum(call[:2] == ['launch', 'start'] for call in calls) == 1
        assert not any(call[0] in ['send', 'switch', 'stop', 'restart'] for call in calls)
        evidence = {'synthetic_only': True, 'provider_calls': 0, 'models': 0, 'size': [width, height], 'markers': markers, 'gif_bytes': (output/'demo.gif').stat().st_size, 'gif_frames': gif.n_frames, 'gif_duration_ms': duration_ms, 'pacing': {'key_pause_ms': 450, 'typing_ms_per_character': 100, 'reading_pauses_seconds': '1.2–3'}, 'handoff_verified': True, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'tools': json.loads((repo/'scripts/demo-tools.json').read_text()), 'loop': 'Actual initial terminal stream repeated; no replacement UI drawn'}
        evidence['terminal_colors'] = {'background': terminal_background, 'foreground': terminal_foreground, 'osc_replies': color_replies, 'renderer_theme': 'dracula', 'herdr_panel_bg': '#282a36'}
        (output/'demo-evidence.json').write_text(json.dumps(evidence, indent=2)+'\n')
        print(json.dumps(evidence, indent=2))
    except Exception:
        if (root/'calls.jsonl').exists():
            print((root/'calls.jsonl').read_text(), flush=True)
        raise
    finally:
        # Only this temporary host/process group is eligible for cleanup.
        try: host('server', 'stop')
        except Exception:
            if server.poll() is None: server.terminate()
        if client and client.poll() is None: client.terminate()
        for process in [client, server]:
            if process:
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired: process.kill(); process.wait()
        os.close(master); os.close(slave); log.close()
