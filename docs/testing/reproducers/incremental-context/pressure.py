#!/usr/bin/env python3
"""Exceed decoded-cache retention through actual observations, then revisit earlier source."""
import argparse
import json
from pathlib import Path
import subprocess
p = argparse.ArgumentParser()
p.add_argument('--consumer', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
base = a.output.resolve()
base.mkdir(parents=True, exist_ok=False)
root = base / 'workspace'
root.mkdir()
private = base / 'private'
private.mkdir()
spec = {'root': str(root), 'trace': str(private / 'pressure.trace'), 'task': 'Inspect the requested numbered source files. Keep exact latest source data.', 'capacity': 200000}
spec_path = base / 'spec.json'
spec_path.write_text(json.dumps(spec))
error_file = (base / 'stderr.log').open('w')
child = subprocess.Popen([str(a.consumer.resolve()), str(spec_path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=error_file, text=True)
def invoke(command):
    child.stdin.write(json.dumps(command) + '\n')
    child.stdin.flush()
    value = json.loads(child.stdout.readline())['answer']
    assert value['ok'], value
    return value['value']
try:
    invoke({'action': 'read', 'tool': 'workspace_list', 'args': {}})
    for i in range(48):
        marker = f'PRESSURE-CURRENT-{i:03d}-78319'
        (root / f'source-{i}.txt').write_text(marker + '\n' + 'x' * 400000 + '\n')
        invoke({'action': 'observe', 'calls': [['workspace_read', {'path': f'source-{i}.txt'}]]})
        view = invoke({'action': 'assemble'})
        assert marker in json.dumps(view), i
    invoke({'action': 'observe', 'calls': [['workspace_read', {'path': 'source-0.txt'}]]})
    view = invoke({'action': 'assemble'})
    assert 'PRESSURE-CURRENT-000-78319' in json.dumps(view)
    calls = [b['id'] for m in view['messages'] for b in m['blocks'] if b['type'] == 'call']
    results = [b['id'] for m in view['messages'] for b in m['blocks'] if b['type'] == 'result']
    assert sorted(calls) == sorted(results)
    events = [json.loads(line) for line in (private / 'pressure.efficiency.jsonl').read_text().splitlines()]
    views = [v for v in events if v.get('event') == 'context-view']
    assert max(v['decoded_message_cache_misses'] for v in views[32:]) <= 2, 'omitted history churned the decoded cache'
    assert sum(p.stat().st_size for p in root.iterdir()) * 4 > 64 * 1024 * 1024, 'fixture did not exceed retained decode weights'
    ordinary_views = len(views)
    for i in range(48):
        path = root / f'source-{i}.txt'
        old = path.stat()
        path.write_bytes(path.read_bytes().replace(b'PRESSURE-CURRENT-', b'PRESSURE-CHANGED-', 1))
        import os
        os.utime(path, ns=(old.st_atime_ns, old.st_mtime_ns))
    for _ in range(3):
        stale = invoke({'action': 'assemble'})
        rendered = json.dumps(stale)
        assert 'cached_observation_stale' in rendered and 'PRESSURE-CURRENT-' not in rendered
    events = [json.loads(line) for line in (private / 'pressure.efficiency.jsonl').read_text().splitlines()]
    all_views = [v for v in events if v.get('event') == 'context-view']
    stale_views = all_views[ordinary_views:]
    assert len(stale_views) == 3
    assert all(v['decoded_message_cache_misses'] == 0 and v['newly_indexed_messages'] == 0 for v in stale_views[1:]), stale_views
    summary = {'passed': True, 'distinct_large_sources': 48, 'source_bytes': sum(p.stat().st_size for p in root.iterdir()), 'context_views': len(views), 'decoded_cache_misses': sum(v['decoded_message_cache_misses'] for v in views), 'peak_view_cache_misses': max(v['decoded_message_cache_misses'] for v in views), 'revisited_evicted_source_correct': True, 'changed_source_warm_decode_misses':[v['decoded_message_cache_misses'] for v in stale_views], 'call_result_order_preserved': True}
    (base / 'result.json').write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))
finally:
    child.stdin.close()
    child.wait(timeout=30)
    error_file.close()
    assert child.returncode == 0, (base / 'stderr.log').read_text()
