#!/usr/bin/env python3
"""Drive real host tools and optional authenticated design handoffs; no mock providers."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--consumer', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--peritus-copy', type=Path)
parser.add_argument('--baseline-consumer', type=Path)
args = parser.parse_args()
binary = args.consumer.resolve()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=False)
results = []


def invoke(name, spec, mode='probe', executable=binary):
    request = out / f'{name}.json'
    request.write_text(json.dumps(spec))
    result = subprocess.run([str(executable), mode, str(request)], text=True,
                            capture_output=True, timeout=600)
    (out / f'{name}.stdout').write_text(result.stdout)
    (out / f'{name}.stderr').write_text(result.stderr)
    assert result.returncode == 0, (name, result.returncode, result.stderr[-2000:])
    return json.loads(result.stdout)


def complete(result, position='after'):
    return (result[position]['required_tool'] is None
            and result[position]['completion_blocker'] is None)


def note(name):
    results.append(name)
    (out / 'results.json').write_text(json.dumps({
        'passed': results, 'binary': str(binary),
        'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    }, indent=2))


rng = random.Random(739211)
for index, relative in enumerate(['alpha.txt', 'name space.txt', 'nested/café.txt', 'brackets[3].txt']):
    root = out / f'workspace-{index}'
    root.mkdir()
    source = root / relative
    source.parent.mkdir(parents=True, exist_ok=True)
    source.write_text(f'ORIGINAL-{rng.randrange(1000, 9999)}\n')
    spec = {'root': str(root), 'trace': str(out / f'private-{index}/run.trace'),
            'operations': [['workspace_list', {}]]}
    result = invoke(f'{index}-one-check', spec)
    assert result['before']['required_tool'] == 'workspace_list'
    assert complete(result)
    receipt = json.loads(Path(spec['trace']).with_suffix('.observations.json').read_text())
    assert receipt['grounding_complete'] and receipt['files'] == {}
    note(f'{index}-one-check-no-file-read')
    (root / 'new-entry.txt').write_text('directory changed\n')
    spec['operations'] = []
    spec['revision'] = 2
    spec['task'] = 'Changed task: continue work without another startup inventory.'
    result = invoke(f'{index}-restart', spec)
    assert complete(result, 'before')
    note(f'{index}-restart-directory-and-task-change')
    spec['operations'] = [['workspace_read', {'path': relative}]]
    result = invoke(f'{index}-target-read', spec)
    assert not result['outputs'][0]['is_error']
    source.write_text(f'CHANGED-{rng.randrange(1000, 9999)}\n')
    spec['operations'] = []
    result = invoke(f'{index}-stale', spec)
    assert complete(result, 'before') and relative in result['before']['guidance']
    spec['operations'] = [['workspace_read', {'path': relative}]]
    result = invoke(f'{index}-refresh', spec)
    assert result['outputs'][0]['output']['source_sha256'] == hashlib.sha256(source.read_bytes()).hexdigest()
    source.unlink()
    spec['operations'] = []
    assert complete(invoke(f'{index}-deleted', spec), 'before')
    note(f'{index}-stale-target-refresh-and-deletion')
    other = dict(spec, workspace=93)
    assert invoke(f'{index}-scope', other)['before']['required_tool'] == 'workspace_list'
    note(f'{index}-separate-workspace-does-not-reuse')

if args.baseline_consumer:
    root = out / 'legacy-workspace'
    root.mkdir()
    (root / 'existing.txt').write_text('old installed program\n')
    spec = {'root': str(root), 'trace': str(out / 'legacy-private/run.trace'),
            'operations': [['workspace_list', {}]]}
    old = invoke('original-blocker', spec, executable=args.baseline_consumer.resolve())
    assert old['after']['required_tool'] == 'workspace_read'
    spec['operations'] = []
    assert complete(invoke('legacy-receipt-upgrade', spec), 'before')
    note('original-two-step-blocker-and-real-legacy-receipt-upgrade')

if args.peritus_copy:
    root = args.peritus_copy.resolve()
    assert (root / 'Cargo.toml').is_file() and (root / 'crates').is_dir()
    trace = out / 'live-private/run.trace'
    spec = {'root': str(root), 'trace': str(trace), 'run': 113, 'workspace': 114,
            'model': 'gpt-6.1-sol', 'stop_at_writing': True,
            'task': 'Add a concise paragraph documenting one startup grounding check to the product-runner README. Preserve unrelated work. Do not run unit tests, lint, formatting, or formal verification. Design the task from the request and available observations, then hand the exact target read to the writer. For the plan, omit the title line and begin with an ordinary paragraph; the host adds its heading.'}
    first = invoke('live-design', spec, 'live')
    assert first['settlement'] == 'Cancelled'
    assert 'PHASE Writing:' in (out / 'live-design.stderr').read_text()
    metrics = trace.with_suffix('.efficiency.jsonl')
    rows = [json.loads(line) for line in metrics.read_text().splitlines()]
    views = [row for row in rows if row['event'] == 'model-view']
    assert len(views) >= 2 and all(view['advertised_tool_count'] == 0 for view in views[1:]), views
    assert any('PHASE Writing:' in line and 'tools=1' in line
               for line in (out / 'live-design.stderr').read_text().splitlines())
    assert trace.with_suffix('.design.md').read_text().startswith('# Implementation task plan\n')
    receipt = json.loads(trace.with_suffix('.observations.json').read_text())
    assert receipt['grounding_complete'] and receipt['files'] == {}
    before = len(rows)
    note('live-peritus-design-one-check-then-tools-closed')
    spec['revision'] = 2
    spec['task'] += ' Also explain that changed files require a targeted refresh.'
    second = invoke('live-design-after-steering', spec, 'live')
    assert second['settlement'] == 'Cancelled'
    assert 'PHASE Writing:' in (out / 'live-design-after-steering.stderr').read_text()
    new_rows = [json.loads(line) for line in metrics.read_text().splitlines()][before:]
    views = [row for row in new_rows if row['event'] == 'model-view']
    assert views and all(view['advertised_tool_count'] == 0 for view in views), views
    assert any('PHASE Writing:' in line and 'tools=0' in line
               for line in (out / 'live-design-after-steering.stderr').read_text().splitlines())
    note('live-peritus-restart-with-steering-zero-new-grounding')

print(json.dumps({'passed': len(results), 'evidence': str(out / 'results.json')}))
