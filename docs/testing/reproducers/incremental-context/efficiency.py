#!/usr/bin/env python3
"""Compare real context consumers against independently created source bytes and unchanged views.

Unlike the historical document-cache comparison, this campaign does not require an unrelated
tenfold document-read improvement. It measures the actual before/after context paths.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--after', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--operations', type=int, default=24)
    parser.add_argument('--reference-pages', action='store_true')
    parser.add_argument('--candidate-first', action='store_true')
    parser.add_argument('--after-diagnostics-off', action='store_true')
    args = parser.parse_args()
    if args.operations < 1:
        parser.error('--operations must be positive')
    base = args.output.resolve()
    base.mkdir(parents=True, exist_ok=False)
    workspace = base / 'workspace'
    workspace.mkdir()
    expected = {}
    for index in range(16):
        name = f'source-{index:02d}.txt'
        content = ''.join(f'INDEPENDENT-SOURCE-{index:02d}-ROW-{line:03d} Ω\n' for line in range(80))
        (workspace / name).write_text(content)
        expected[name] = content
    operations = [['workspace_read', {'path': f'source-{index % 16:02d}.txt'}]
                  for index in range(args.operations)]
    outputs = []
    measurements = []
    cohorts = [('before', args.before), ('after', args.after)]
    if args.candidate_first:
        cohorts.reverse()
    for label, binary in cohorts:
        private = base / label
        private.mkdir()
        spec = {
            'root': str(workspace), 'trace': str(private / 'run.trace'),
            'command_state': str(private / 'commands'),
            'task': 'Inspect the exact requested source files. Continue from the current observations.',
            'capacity': 200000, 'operations': operations,
        }
        if args.reference_pages:
            spec['local_context'] = {'reuse_checkpoint_pages': label == 'after'}
        path = private / 'input.json'
        path.write_text(json.dumps(spec))
        started = time.monotonic()
        environment = os.environ.copy()
        if label == 'after' and args.after_diagnostics_off:
            environment['PERITUS_EFFICIENCY_LOG'] = '0'
        child = subprocess.Popen([str(binary.resolve()), 'context', str(path)], env=environment,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0)
        try:
            first = child.stdout.read(1)
            if not first:
                _, stderr = child.communicate(timeout=120)
                raise RuntimeError(f'{label}: consumer returned no output: {stderr.decode()}')
            try:
                io = dict(line.split(': ', 1) for line in
                          Path(f'/proc/{child.pid}/io').read_text().splitlines())
            except FileNotFoundError:
                io = {}
            tail, stderr = child.communicate(timeout=120)
            (private / 'stderr.log').write_bytes(stderr)
            if child.returncode:
                raise RuntimeError(f'{label}: exit {child.returncode}: {stderr.decode()}')
            payload = first + tail
            result = json.loads(payload)
            if len(result['views']) != len(operations):
                raise AssertionError((label, 'missing completed view'))
            checked_pages = 0
            for view in result['views']:
                for message in view['messages']:
                    for block in message['content']:
                        if block['type'] != 'result':
                            continue
                        observed = json.loads(block['output'])
                        if 'content' not in observed:
                            continue
                        name = observed.get('path')
                        if name not in expected:
                            continue
                        if block['is_error']:
                            raise AssertionError((label, name, 'unexpected read rejection'))
                        numbered = '\n'.join(f'{index + 1}: {line}' for index, line in
                                             enumerate(expected[name].splitlines()))
                        if observed['content'] != numbered:
                            raise AssertionError((label, name, 'source bytes differ from fixture'))
                        if observed['source_sha256'] != hashlib.sha256(expected[name].encode()).hexdigest():
                            raise AssertionError((label, name, 'source identity differs from fixture'))
                        checked_pages += 1
            if not checked_pages:
                raise AssertionError((label, 'no actual source pages were examined'))
            events = private / 'run.efficiency.jsonl'
            work = [json.loads(line) for line in events.read_text().splitlines()] if events.exists() else []
            if label == 'after' and args.after_diagnostics_off:
                assert not events.exists(), 'disabled optional diagnostics still wrote a sidecar'
            publications = [event for event in work if event.get('event') == 'context-checkpoint-work']
            counters = {}
            for event in publications:
                for key, value in event['work'].items():
                    counters[key] = counters.get(key, 0) + value
            measurements.append({
                'label': label, 'elapsed_seconds': time.monotonic() - started,
                'process_io': {key: int(io[key]) for key in ['rchar', 'wchar', 'syscr', 'syscw', 'write_bytes'] if key in io},
                'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                'result_sha256': hashlib.sha256(payload).hexdigest(),
                'source_pages_checked': checked_pages,
                'checkpoint_work': counters, 'checkpoint_publications': len(publications),
            })
            outputs.append(result['views'])
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=10)
    for before, after in zip(*outputs):
        if before['messages'] != after['messages']:
            (base / 'mismatched-views.json').write_text(json.dumps({'before': before, 'after': after}, indent=2))
            raise AssertionError(('provider-visible messages changed', before['step']))
    if args.reference_pages:
        by_label = {measurement['label']: measurement['checkpoint_work'] for measurement in measurements}
        before, after = by_label['before'], by_label['after']
        if not after['reused_objects'] or after['finalizations'] >= before['finalizations']:
            raise AssertionError(('page reuse did not reduce finalizations', before, after))
        if after['submitted_bytes'] >= before['submitted_bytes']:
            raise AssertionError(('page reuse did not reduce encoded/store input bytes', before, after))
    summary = {'operations': args.operations, 'exact_model_views_equal': True,
               'independent_source_oracle': 'numbered bytes and SHA-256 from fixture-owned files',
               'reference_pages': args.reference_pages, 'measurements': measurements}
    (base / 'result.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
