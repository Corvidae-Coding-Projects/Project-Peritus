#!/usr/bin/env python3
"""Exercise actual authorized file plans or a native command through direct OAuth daemon runs."""
import argparse
import hashlib
import json
import secrets
import subprocess
import time
import re
from pathlib import Path

p = argparse.ArgumentParser()
for name in ['daemon', 'cli', 'client', 'output']:
    p.add_argument('--' + name, type=Path, required=True)
p.add_argument('--oauth-reference')
p.add_argument('--question-during-coding', action='store_true')
p.add_argument('--wait-after-reply', action='store_true')
p.add_argument('--resume-after-wait', action='store_true')
p.add_argument('--status-question', action='store_true')
p.add_argument('--case', choices=['read-plan', 'command', 'coding'], required=True)
a = p.parse_args()
case = a.output.resolve() / a.case
case.mkdir(parents=True)
workspace = case / 'workspace'
workspace.mkdir()
state = case / 'state'
state.mkdir(mode=0o700)
values = ['DIRECT-A-' + secrets.token_hex(8), 'DIRECT-B-' + secrets.token_hex(8)]
(workspace / 'alpha.txt').write_text(values[0] + '\n')
(workspace / 'beta.txt').write_text(values[1] + '\n')
command_token = 'DIRECT-COMMAND-' + secrets.token_hex(8)
(workspace / 'check.py').write_text('print(' + repr(command_token) + ')\n')
desired = 'DIRECT-CODE-' + secrets.token_hex(8)
(workspace / 'greeting.py').write_text("def greeting():\n    return 'OLD'\n")
before = {f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in workspace.iterdir()}
endpoint = str(state / 'daemon.sock')
digest = hashlib.sha256(b'peritus/daemon-endpoint/v1\0' + bytes([0x81]) * 16).hexdigest()[:32]
original = str(state / ('peritus-' + digest + '.sock'))
endpoint = original if len(original.encode()) <= 107 else '/tmp/peritus-' + hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\0' + original.encode()).hexdigest()[:32] + '/daemon.sock'
task = ('Read alpha.txt and beta.txt using one inspection_plan version 1 containing exactly two '
        'workspace_read nodes in that order. Use current scope/input_revision, cursor=null, '
        'max_result_bytes=65536, empty result_refs and appropriate depends_on. The arguments '
        'are literal path fields. Perform any mandatory initial listing first. '
        'Report both exact file contents. No workspace edits or processes. Local context bookkeeping is allowed.')
if a.case == 'command':
    task = ('This explicitly authorizes one native verification command and no file changes. '
            'Call run_pipeline to hand off the work. Inspect check.py, then execute it once with '
            'run_command program python3, args ["check.py"], purpose external_effect, timeout_seconds 10. '
            'Report its actual stdout. Do not compile, edit, run other commands, or invent evidence. '
            'The task is complete after this command and its report. Local context bookkeeping is allowed.')
if a.case == 'coding':
    task = ('Implement the requested change in greeting.py: greeting() must return the exact string ' + desired + '. '
            'Call run_pipeline to do the implementation. Read the current file, make only this change, '
            'and verify the actual module using python3 -B -c with an assertion on greeting() and print CODE-VERIFIED on success. '
            'Use purpose verification. Do not edit other files. Complete normal independent review and report the actual result.')
spec = dict(mode='plan', root=str(workspace), registry=str(case / 'registry.bin'), endpoint=endpoint, task=task)
if a.question_during_coding:
    if a.case != 'coding':
        raise ValueError('the interruption replay requires a real coding pipeline')
    spec['steering'] = ('Before continuing, what is 17 times 23? Answer me now in one sentence, '
                        + ('then wait for my next message. Do not change files or run commands.'
                           if a.wait_after_reply else
                           'then continue the original change without restarting it.'))
elif a.wait_after_reply:
    raise ValueError('--wait-after-reply requires --question-during-coding')
if a.resume_after_wait and not a.wait_after_reply:
    raise ValueError('--resume-after-wait requires --wait-after-reply')
if a.status_question:
    if not a.question_during_coding or a.wait_after_reply:
        raise ValueError('--status-question requires a coding question without a wait request')
    spec['steering'] = 'what are you currently working on'
reply_pattern = r'\bgreeting(?:\.py)?\b' if a.status_question else r'\b391\b'
spec_path = case / 'spec.json'
spec_path.write_text(json.dumps(spec))

def call(action):
    done = subprocess.run([str(a.client.resolve()), str(spec_path), action], capture_output=True, text=True, timeout=35)
    with (case / 'client.jsonl').open('a') as log:
        log.write(json.dumps(dict(action=action, code=done.returncode, stdout=done.stdout, stderr=done.stderr)) + '\n')
    if done.returncode:
        raise RuntimeError(done.stderr)
    return json.loads(done.stdout)

folder = call('setup')['folder_identity']
q = json.dumps
config = 'version=1\nstore_id="' + '81' * 16 + '"\n[paths]\nstate_root=' + q(str(state)) + '\n'
for name in ['artifact', 'evidence', 'workspace', 'process', 'transaction', 'backup']:
    config += name + '_root=' + q(str(state / (name + 's'))) + '\n'
config += '[approval_registry]\npayload_file=' + q(spec['registry']) + '\ngeneration=1\n[human]\nactor_id="' + '31' * 16 + '"\n[telemetry]\nmode="disabled"\n[tools]\nallow=["fs.read","fs.discover","fs.search","shell.exec","fs.write","fs.patch"]\n'
config += '[[folders]]\nworkspace_id="' + '82' * 16 + '"\nroot=' + q(str(workspace)) + '\nidentity=' + q(folder) + '\nwritable=' + ('true' if a.case in ['command', 'coding'] else 'false') + '\nprotected_paths=[]\n'
config += '[[providers]]\nkind="chatgpt-oauth"\ncredential_reference=' + q(a.oauth_reference) + '\n[providers.profile]\nprofile_id="' + '95' * 16 + '"\nrevision=1\nmodel="gpt-6.1-sol"\ncapabilities=["tool-calls","parallel-tool-calls","usage-detail","streaming","reasoning-controls","reasoning-summaries","reasoning-replay","prompt-caching"]\nmax_input_tokens=200000\nmax_output_tokens=16000\nmax_tools=64\nmax_parallel_tool_calls=8\nmax_inline_media_bytes=1\n'
if not a.oauth_reference:
    config = config.replace('kind="chatgpt-oauth"\ncredential_reference=null\n',
                            'kind="codex-runtime"\n')
    config = config.replace(',"reasoning-replay"', '')
configuration = case / 'daemon.toml'
configuration.write_text(config)
stdout = (case / 'daemon.stdout').open('w')
stderr = (case / 'daemon.stderr').open('w')
daemon = subprocess.Popen([str(a.daemon.resolve()), 'serve', '--config', str(configuration)], stdout=stdout, stderr=stderr)

def observations():
    rows = []
    for trace in (state / 'product-runs').glob('*.trace'):
        raw = trace.read_bytes()
        at = 0
        while at + 9 <= len(raw):
            tag, size = raw[at], int.from_bytes(raw[at + 1:at + 9], 'little')
            payload = raw[at + 9:at + 9 + size]
            at += 9 + size
            if tag == 7:
                rows.append(json.loads(payload))
    return rows

try:
    deadline = time.monotonic() + 20
    while not Path(endpoint).exists():
        if daemon.poll() is not None:
            raise RuntimeError((case / 'daemon.stderr').read_text())
        if time.monotonic() > deadline:
            raise TimeoutError('daemon readiness')
        time.sleep(.1)
    call('start')
    started = time.monotonic()
    deadline = started + 240
    steered = None
    reply_seconds = None
    tools_before_reply = None
    reply_texts = set()
    paused_safely = False
    resumed = False
    while True:
        after = call('inspect')
        usage = after['usage']
        if a.question_during_coding:
            if steered is None and 'Writing' in after['interaction'] and any(
                    row['name'] == 'workspace_read' and 'greeting.py' in row['arguments']
                    for row in observations()):
                reply_texts = {v['text'] for v in after['activities'] if v['kind'] == 'Assistant'}
                call('steer')
                steered = time.monotonic()
                print('Question accepted during the actual writing pipeline', flush=True)
            if steered is not None and reply_seconds is None and any(
                    v['kind'] == 'Assistant' and v['text'] not in reply_texts
                    and re.search(reply_pattern, v['text']) for v in after['activities']):
                reply_seconds = time.monotonic() - steered
                # Polling can observe a reply and a later tool together. Use their durable
                # activity order, never count all tools present by the time of the poll.
                activities = after['activities']
                received = max(i for i, v in enumerate(activities)
                               if v['kind'] == 'User' and v['text'] == spec['steering'])
                replied = next(i for i, v in enumerate(activities) if i > received
                               and v['kind'] == 'Assistant' and re.search(reply_pattern, v['text']))
                tools_before_reply = sum(v['kind'] == 'Tool' for v in activities[received + 1:replied])
        if (a.resume_after_wait and not resumed and reply_seconds is not None
                and usage and usage['state'] == 'WaitingForUser'):
            paused_safely = all(hashlib.sha256((workspace / name).read_bytes()).hexdigest() == digest
                                for name, digest in before.items())
            if not paused_safely:
                raise RuntimeError('files changed after the request to wait')
            spec['steering'] = 'Continue the original greeting.py change and verification now.'
            spec_path.write_text(json.dumps(spec))
            call('steer')
            call('resume')
            resumed = True
            continue
        if usage and usage['state'] in ['WaitingForUser', 'Completed', 'Achieved', 'Blocked', 'Cancelled', 'Failed']:
            break
        if time.monotonic() > deadline:
            raise TimeoutError('real tool journey')
        time.sleep(.2)
    rows = observations()
    names = [r['name'] for r in rows]
    answer = '\n'.join(v['text'] for v in after['activities'] if v['kind'] == 'Assistant')
    success = [r for r in rows if not r['is_error']]
    expected = values if a.case == 'read-plan' else [command_token]
    actual_tool = any(r['name'] == 'inspection_plan' and all(v in r['output'] for v in values) for r in success) if a.case == 'read-plan' else any(r['name'] == 'run_command' and command_token in r['output'] for r in success)
    current = {f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in workspace.iterdir() if f.is_file()}
    unchanged = set(before)==set(current) and all(v==current[k] for k,v in before.items() if a.case!='coding' or k!='greeting.py')
    if a.case == 'coding':
        done = subprocess.run(['python3','-B','-c','from greeting import greeting; assert greeting()=='+repr(desired)],cwd=workspace,capture_output=True)
        actual_tool = done.returncode==0 and before['greeting.py']!=current['greeting.py'] and any(r['name']=='run_command' and 'CODE-VERIFIED' in r['output'] for r in success)
        expected=[]
    settled = all(role['requests']==role['completed_requests'] for role in usage['roles'])
    reviewed = any(role['role']=='Reviewer' and role['completed_requests']>0 for role in usage['roles'])
    terminal = usage['state']=='Achieved' and reviewed if a.case=='coding' else usage['state'] in ['WaitingForUser', 'Completed', 'Achieved']
    passed = actual_tool and all(v in answer for v in expected) and unchanged and terminal and settled and (a.case=='coding' or not any(n in ['workspace_write', 'workspace_patch', 'workspace_remove'] for n in names))
    if a.wait_after_reply and not a.resume_after_wait:
        # A requested pause preserves the original file and never claims task completion.
        passed = current == before and usage['state'] == 'WaitingForUser' and settled
    if a.resume_after_wait:
        passed = passed and paused_safely and resumed
    if a.question_during_coding:
        passed = passed and steered is not None and reply_seconds is not None and reply_seconds < 30 and tools_before_reply == 0
    receipt = dict(case=a.case, passed=passed, actual_direct_oauth=bool(a.oauth_reference), actual_tools=names, actual_tool_evidence=actual_tool, exact_answer=all(v in answer for v in expected), workspace_unchanged=unchanged, requests_settled=settled, independent_review=reviewed, usage=usage, seconds=time.monotonic()-started, final_details=after['activities'][-2:], question_during_coding=a.question_during_coding, seconds_to_reply=reply_seconds, tools_before_reply=tools_before_reply, wait_after_reply=a.wait_after_reply, paused_safely=paused_safely, resumed=resumed)
    (case / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt), flush=True)
    if not passed:
        raise RuntimeError('real tool requirements failed; inspect retained receipt')
finally:
    if Path(endpoint).exists():
        subprocess.run([str(a.cli.resolve()), '--endpoint', endpoint, 'shutdown', '--wait'], capture_output=True, timeout=40)
    try:
        daemon.wait(timeout=10)
    except subprocess.TimeoutExpired:
        daemon.terminate()
        daemon.wait(timeout=10)
    stdout.close()
    stderr.close()
