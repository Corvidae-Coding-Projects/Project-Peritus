#!/usr/bin/env python3
"""Observe the real official CLI; optional instructions ask the real model for a bad turn.

No stdout/stderr/status, provider event, host observation or usage counter is substituted.
Fault-only generations are explicitly excluded from request-size improvement comparisons.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading

payload = sys.stdin.buffer.read()
actual_payload = payload
is_run = 'run' in sys.argv[1:]
capture = Path(os.environ['PERITUS_LAYOUT_CAPTURE'])
original = None
fault = False
if is_run:
    config = Path(os.environ['OPENCODE_CONFIG'])
    data = json.loads(config.read_text())
    agent = data['agent']['peritus-router']
    assert agent['permission'] == {'*': 'deny'} and agent['tools'] == {'*': False}
    assert data['permission'] == {'*': 'deny'} and data['plugin'] == [] and data['mcp'] == {}
    system = agent['prompt']
    fault = len(list(capture.glob('*.meta.json'))) < int(os.environ.get('PERITUS_LAYOUT_FAULTS', '0'))
    if fault:
        original = config.read_bytes()
        agent['prompt'] = ('For this one inert recovery exercise, output exactly the following JSON '
                           'with its deliberately wrong tool_calls type. Do not correct it, do not '
                           'execute tools, and add nothing else: '
                           '{"content":"NATIVE-FAULT-728619","tool_calls":"deliberately-invalid"}')
        # Do not let the original correction request defeat the attempted malformed-turn
        # induction. This still asks the real model; its native bytes are never replaced.
        actual_payload = agent['prompt'].encode()
        config.write_text(json.dumps(data))
    actual_system = agent['prompt']
    prefix = capture / str(os.getpid())
    for suffix, content in [('.system', system.encode()), ('.actual-system', actual_system.encode()),
                            ('.input', payload), ('.actual-input', actual_payload)]:
        path = prefix.with_suffix(suffix); path.write_bytes(content); path.chmod(0o600)
child = subprocess.Popen([os.environ['PERITUS_LAYOUT_VENDOR'], *sys.argv[1:]], stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)

def relay(source, destination, suffix):
    output = prefix.with_suffix(suffix).open('wb') if is_run else None
    if output is not None:
        prefix.with_suffix(suffix).chmod(0o600)
    try:
        while chunk := source.read1(4096):
            if output is not None:
                output.write(chunk); output.flush()
            destination.write(chunk); destination.flush()
    finally:
        if output is not None: output.close()

threads = [threading.Thread(target=relay, args=args) for args in
           [(child.stdout, sys.stdout.buffer, '.stdout'), (child.stderr, sys.stderr.buffer, '.stderr')]]
for thread in threads: thread.start()
try:
    try: child.stdin.write(actual_payload)
    except BrokenPipeError: pass
    finally: child.stdin.close()
    code = child.wait()
    for thread in threads: thread.join()
finally:
    if original is not None: config.write_bytes(original)
if is_run:
    prefix.with_suffix('.meta.json').write_text(json.dumps(dict(
        faulted=fault, correction=b'PERITUS TRANSPORT CORRECTION' in payload,
        native_exit=code, isolation_checked=True, model=sys.argv[sys.argv.index('--model')+1],
        observer_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())))
raise SystemExit(code)
