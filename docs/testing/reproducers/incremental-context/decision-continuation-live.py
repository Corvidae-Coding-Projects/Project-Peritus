#!/usr/bin/env python3
"""Real same-lineage ProductRunner continuation after fresh process, input and file changes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import time


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--consumer', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--model', default='gpt-6.1-sol')
    a = p.parse_args(); root = a.output.resolve(); root.mkdir(parents=True)
    workspace = root / 'workspace'; workspace.mkdir(); private = root / 'private'; private.mkdir()
    facts = dict(alpha='CONTINUATION-ALPHA-OLD-718392 Ω', beta='CONTINUATION-BETA-EXACT-519627 Ω')
    for name, value in facts.items(): (workspace / f'{name}.txt').write_text(value + '\n')
    helpers = runpy.run_path(str(Path(__file__).with_name('decision-live-compare.py')))
    cases = []
    previous_calls = 0; previous_requests = 0
    for revision in [1, 2]:
        if revision == 2:
            target = workspace / 'alpha.txt'; stamp = target.stat()
            facts['alpha'] = facts['alpha'].replace('OLD', 'NEW')
            target.write_text(facts['alpha'] + '\n')
            os.utime(target, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
        case = root / f'input-{revision}'; case.mkdir()
        spec = dict(root=str(workspace), trace=str(private / 'run.trace'), command_state=str(private / 'commands'),
                    capacity=200000, model=a.model, revision=revision, conversation_mode='chat',
                    task='Read alpha.txt and beta.txt. Return only a JSON object with keys alpha and beta and '
                         'their exact current sole lines without newlines. Earlier file versions are historical; '
                         'inspect changed source through ordinary workspace_read. No commands or file changes.',
                    local_context=dict(decision_view='conservative'))
        path = case / 'spec.json'; path.write_text(json.dumps(spec)); (case / 'expected.json').write_text(json.dumps(facts))
        started = time.monotonic()
        with (case / 'stdout.json').open('wb') as out, (case / 'stderr.log').open('wb') as err:
            completed = subprocess.run([str(a.consumer.resolve()), 'live', str(path)], stdout=out, stderr=err, timeout=420)
        elapsed = time.monotonic()-started
        assert completed.returncode == 0
        outcome = json.loads((case / 'stdout.json').read_text()); assert json.loads(outcome['reply']) == facts, outcome
        calls = helpers['observations'](private / 'run.trace'); new_calls = calls[previous_calls:]
        assert any(call['name']=='workspace_read' and json.loads(call['arguments'])['path']=='alpha.txt'
                   and not call['is_error'] for call in new_calls)
        for name, value in facts.items(): assert (workspace / f'{name}.txt').read_bytes() == (value+'\n').encode()
        audit = subprocess.check_output([str(a.consumer.resolve()), 'audit-usage', str(path)], text=True)
        (case / 'independent-trace-audit.json').write_text(audit)
        requests = json.loads(audit)['requests']; fresh = requests[previous_requests:]
        assert fresh and all('input_tokens' in request['usage'] for request in fresh)
        cases.append(dict(revision=revision, passed=True, exact_outcome=True, elapsed_seconds=elapsed,
            requests=len(fresh), host_calls=len(new_calls), input_tokens=sum(request['usage']['input_tokens'] for request in fresh),
            cached_input_tokens=sum(request['usage']['cached_input_tokens'] for request in fresh)))
        previous_calls, previous_requests = len(calls), len(requests)
        print(json.dumps(cases[-1]), flush=True)
    result = dict(passed=True, cases=cases, same_lineage=True, fresh_native_process_each_input=True,
                  restored_mtime_same_size_source_change=True, model=a.model,
                  consumer_sha256=hashlib.sha256(a.consumer.read_bytes()).hexdigest())
    (root / 'result.json').write_text(json.dumps(result, indent=2))


if __name__ == '__main__': main()
