#!/usr/bin/env python3
"""Synthetic public CLI fixture. No providers, credential files or network."""
import json, os, pathlib, subprocess, sys, time
root = pathlib.Path(os.environ['FIXTURE_ROOT'])
args = sys.argv[sys.argv.index('api')+1:]
if args[:1] == ['--api-version']:
    assert args[1] == '1'
    args = args[2:]
with (root/'calls.jsonl').open('a') as log:
    log.write(json.dumps(args)+'\n')
caps = {'launch': {'version': 1, 'list': True}, 'resume': {'version': 1, 'list': True, 'directory_selection': True}, 'providers': [{'id': 'codex', 'account_limits': True, 'enrollment': True, 'account_deletion': True}]}
accounts = []
for name, short, week in [('personal', 23, 41), ('work', 62, 18)]:
    accounts.append({'id': 'codex:'+name, 'selectable': True, 'enabled': True, 'usage_fresh': True, 'auth_required': False, 'prefill': {'eligible': True, 'valid_until': int(time.time())+3600}, 'usage': {'observed_at': int(time.time()), 'short': {'duration_minutes': 300, 'used_percent': short}, 'weekly': {'duration_minutes': 10080, 'used_percent': week}}, 'switch_thresholds': {'customized': True, 'short_percent': 80, 'weekly_percent': 90}})
sessions = []
for i, (name, account, state) in enumerate([('api-review', 'work', 'busy'), ('design-notes', 'personal', 'idle'), ('saved-draft', 'personal', 'saved')]):
    sessions.append({'id': '00000000-0000-4000-8000-'+str(i+1).zfill(12), 'generation': 'synthetic-v1', 'provider': 'codex', 'name': name, 'account_id': 'codex:'+account, 'account': account, 'availability': 'saved' if state == 'saved' else 'running', 'activity': state, 'active_since': int(time.time())-130 if state != 'saved' else None, 'conversation_created_at': '2026-01-01T09:00:00Z', 'history_file_bytes': 24576, 'conversation_id': '00000000-0000-4000-8000-000000000099', 'directory': str(root/'home/Projects'/name), 'capabilities': {'rename': True, 'resume': state == 'saved', 'archive': state == 'saved', 'delete': state == 'saved'}})
if args == ['capabilities']: result = caps
elif args == ['account', 'list']: result = accounts
elif args == ['state']: result = {'sessions': sessions, 'capabilities': caps}
elif args[:1] == ['observe']: result = {'activity': 'working', 'freshness': 'fresh', 'capabilities': {'submit': False, 'queue': True}}
elif args[:1] == ['session']: result = next(s for s in sessions if s['id'] == args[args.index('--session')+1])
elif args == ['operations'] or args == ['archives']: result = []
elif args[:2] == ['account', 'limits']: result = {'limits': {'short_percent': 80, 'weekly_percent': 90}}
elif args[:1] in [['launch'], ['resume-launch']]:
    receipt_file = root/'receipt.json'
    if args[1] == 'list': result = [json.loads(receipt_file.read_text())] if receipt_file.exists() and args[0] == 'launch' else []
    elif args[1] == 'status': result = json.loads(receipt_file.read_text())
    elif args[1] in ['validate', 'start']:
        request = json.load(sys.stdin)
        assert request['account_id'] in ['codex:personal', 'codex:work']
        assert pathlib.Path(request['directory']['path']).is_relative_to(root/'home/Projects')
        assert request['host']['session'] == 'sessionsation-demo'
        if args[1] == 'validate': result = {'valid': True}
        else:
            assert not receipt_file.exists(), 'The demo must admit one launch only'
            for value in request['directory']['create']:
                path = pathlib.Path(value)
                assert path.is_relative_to(root/'home/Projects')
                path.mkdir()
            host = subprocess.check_output(['herdr', '--session', 'sessionsation-demo', 'tab', 'create', '--workspace', request['space']['id'], '--cwd', request['directory']['path'], '--label', request['name'], '--no-focus'], text=True)
            tab = json.loads(host)['result']
            result = {'request': request, 'phase': 'running', 'tab_id': tab['tab']['tab_id'], 'pane_id': tab['root_pane']['pane_id'], 'operation_id': None}
            receipt_file.write_text(json.dumps(result))
    else: raise AssertionError(args)
else:
    raise AssertionError('Demo attempted an unexpected action: '+repr(args))
print(json.dumps({'api_version': 1, 'ok': True, 'result': result}))
