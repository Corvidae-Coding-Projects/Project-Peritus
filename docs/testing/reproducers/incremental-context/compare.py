#!/usr/bin/env python3
"""Compare actual pre/post public consumers. Read counters after all API work, before output drains."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
p = argparse.ArgumentParser()
p.add_argument('--before', type=Path, required=True)
p.add_argument('--after', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
base = a.output.resolve()
base.mkdir(parents=True, exist_ok=False)
root = base / 'workspace'
root.mkdir()
data = b''.join(f'{i:05d}: independently-owned-document-row '.encode() + b'x' * 83 + b'\n' for i in range(50000))
(root / 'document.txt').write_bytes(data)
for i in range(16):
    (root / f'source-{i}.txt').write_text((f'INDEPENDENT-OBSERVATION-{i:02d} ' + 'q' * 119 + '\n') * 100)
def run(label, binary, mode, operations):
    private = base / label
    private.mkdir()
    spec = {'root': str(root), 'trace': str(private / 'run.trace'), 'command_state': str(private / 'commands'), 'task': 'Inspect the explicitly requested source documents.', 'capacity': 200000, 'operations': operations}
    path = private / 'spec.json'
    path.write_text(json.dumps(spec))
    started = time.monotonic()
    process = subprocess.Popen([str(binary.resolve()), mode, str(path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)
    first = process.stdout.read(1)
    assert first, (label, process.communicate())
    # Output exceeds pipe capacity. The child has completed all operations and is blocked
    # writing the single JSON report, so this observes end-of-work logical reads exactly.
    io = dict(line.split(': ', 1) for line in Path(f'/proc/{process.pid}/io').read_text().splitlines())
    tail, stderr = process.communicate(timeout=120)
    assert process.returncode == 0, (label, stderr.decode())
    result = json.loads(first + tail)
    (private / 'stdout.json').write_bytes(first + tail)
    (private / 'stderr.log').write_bytes(stderr)
    return result, {'rchar': int(io['rchar']), 'elapsed_s': time.monotonic() - started, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
page_ops = [['workspace_read', {'path': 'document.txt', 'start_line': 48000 + (i % 101), 'end_line': 48003 + (i % 101)}] for i in range(96)]
old, page_old = run('pages-before', a.before, 'probe', page_ops)
new, page_new = run('pages-after', a.after, 'probe', page_ops)
assert old['outputs'] == new['outputs']
assert all(not value['is_error'] for value in new['outputs'])
assert page_new['rchar'] < page_old['rchar'] // 10, (page_old, page_new)
context_ops = [['workspace_read', {'path': f'source-{i % 16}.txt'}] for i in range(24)]
old, context_old = run('context-before', a.before, 'context', context_ops)
new, context_new = run('context-after', a.after, 'context', context_ops)
assert len(old['views']) == len(new['views'])
for before, after in zip(old['views'], new['views']):
    assert before['messages'] == after['messages'], ('model view changed', before['step'], after['step'])
assert context_new['rchar'] < context_old['rchar'], (context_old, context_new)
summary = {'document_bytes': len(data), 'page_calls': len(page_ops), 'context_calls': len(context_ops), 'exact_page_outputs_identical': True, 'exact_context_messages_identical': True, 'pages': {'before': page_old, 'after': page_new}, 'context': {'before': context_old, 'after': context_new}}
(base / 'result.json').write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
