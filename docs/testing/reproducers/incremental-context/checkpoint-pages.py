#!/usr/bin/env python3
"""Actual checkpoint APIs, alternating reuse comparisons and active-cache corruption replay."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import shutil
import statistics
import subprocess

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('page_integrity', HERE / 'page-integrity.py')
page = importlib.util.module_from_spec(spec)
spec.loader.exec_module(page)


class Client:
    def __init__(self, binary, spec_path, directory, trace=False):
        self.errors = (directory / 'stderr.log').open('w')
        command = [str(binary), str(spec_path)]
        if trace:
            command = ['strace', '-f', '-yy', '-s', '1', '-e',
                       'trace=openat,write,fsync,fdatasync,link,unlink',
                       '-o', str(directory / 'syscalls.log'), *command]
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.errors, text=True,
                                        env=dict(os.environ, PERITUS_EFFICIENCY_LOG='1',
                                                 PERITUS_CONTEXT_SOURCE_INDEX='indexed'))
        self.rows = []

    def request(self, command, rejected=False):
        self.process.stdin.write(json.dumps(command) + '\n')
        self.process.stdin.flush()
        ready, _, _ = select.select([self.process.stdout], [], [], 45)
        assert ready, ('public API deadline', command)
        line = self.process.stdout.readline()
        assert line, ('consumer exited', self.process.poll(), command)
        row = json.loads(line)
        self.rows.append(row)
        answer = row['answer']
        if rejected:
            assert not answer['ok'] or answer.get('value', {}).get('is_error'), answer
        else:
            assert answer['ok'] and not answer.get('value', {}).get('is_error'), answer
        return answer.get('value', answer)

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill(); self.process.wait()
            raise
        finally:
            self.errors.close()
        assert self.process.returncode == 0, self.process.returncode


def events(directory):
    return [json.loads(line) for line in (directory / 'run.efficiency.jsonl').read_text().splitlines()]


def verified_roots(directory, all_history=False, role='writer'):
    lineage, manifest, encoded, current = page.checkpoint(directory, role)
    def verify(manifest, encoded, roots):
        expected = {hashlib.sha256(encoded).digest()}
        for key in ['working_state', 'source_index', 'transcript_manifest', 'view', 'validation']:
            ref = manifest[key]; digest = bytes(ref['digest']); expected.add(digest)
            body = page.object_path(lineage, digest).read_bytes()
            assert len(body) == ref['bytes'] and hashlib.sha256(body).digest() == digest
            if key in ['working_state', 'source_index']:
                for ref in json.loads(body)['pages']:
                    digest = bytes(ref['digest']); expected.add(digest)
                    body = page.object_path(lineage, digest).read_bytes()
                    assert len(body) == ref['bytes'] and hashlib.sha256(body).digest() == digest
        assert roots == expected, ('incomplete checkpoint roots', manifest['generation'])
    verify(manifest, encoded, current)
    count = 1
    if all_history:
        import sqlite3
        with sqlite3.connect(f'file:{lineage / "journal.sqlite3"}?mode=ro', uri=True) as db:
            owners = {}
            for owner, digest in db.execute('SELECT owner_identity,artifact_digest FROM artifact_references WHERE owner_kind=1'):
                owners.setdefault(owner, set()).add(digest)
        count = 0
        for candidate in (lineage / 'artifacts/objects/sha256').glob('*/*'):
            body = candidate.read_bytes()
            if not body.startswith(b'{'):
                continue
            try:
                value = json.loads(body)
            except ValueError:
                continue
            if not isinstance(value, dict) or not {'working_state', 'source_index', 'view_binding', 'generation'} <= value.keys():
                continue
            digest = hashlib.sha256(body).digest()
            matches = [roots for roots in owners.values() if digest in roots]
            assert len(matches) == 1, 'healthy checkpoint has no unique publishing owner'
            verify(value, body, matches[0]); count += 1
    return manifest, count


def check_view(value, expected):
    calls, results, checked = [], [], 0
    for message in value['messages']:
        for block in message['blocks']:
            if block['type'] == 'call':
                calls.append(block['id']); assert 'arguments' in block
            if block['type'] != 'result':
                continue
            results.append(block['id']); output = json.loads(block['output'])
            if output.get('path') in expected and 'content' in output:
                source = expected[output['path']]
                assert not block['is_error']
                assert output['content'] == '1: ' + source.rstrip('\n')
                assert output['source_sha256'] == hashlib.sha256(source.encode()).hexdigest()
                checked += 1
    assert sorted(calls) == sorted(results) and len(calls) == len(set(calls))
    return checked


def paired(args, root):
    workspace = root / 'workspace'; workspace.mkdir()
    expected = {f'source-{i:02d}.txt': f'PAGE-INDEPENDENT-{i:02d}-682419 Ω\n' for i in range(32)}
    for name, body in expected.items():
        (workspace / name).write_text(body)
    clients, directories, measured, checked = {}, {}, {}, 0
    try:
        for mode in ['reference', 'reuse']:
            directory = root / mode; directory.mkdir(); directories[mode] = directory
            spec = dict(root=str(workspace), trace=str(directory / 'run.trace'),
                        command_state=str(directory / 'commands'), capacity=200000,
                        task='Inspect source-00.txt and source-31.txt. Preserve exact facts and settled operations.',
                        local_context={'reuse_checkpoint_pages': mode == 'reuse'})
            path = directory / 'input.json'; path.write_text(json.dumps(spec))
            clients[mode] = Client(args.consumer.resolve(), path, directory, args.physical)
        for batch in range(16):
            answers = {}
            for mode in (['reference', 'reuse'] if batch % 2 == 0 else ['reuse', 'reference']):
                client = clients[mode]
                client.request(dict(action='observe', calls=[['workspace_read', {'path': f'source-{i:02d}.txt'}] for i in range(32)]))
                answers[mode] = client.request(dict(action='assemble'))
                checked += check_view(answers[mode], expected)
            assert answers['reference'] == answers['reuse'], ('seed view mismatch', batch)
        starts = {mode: len(events(directory)) for mode, directory in directories.items()}
        for mode in clients:
            measured[mode] = []
        for turn in range(args.steps):
            answers = {}
            for mode in (['reference', 'reuse'] if turn % 2 == 0 else ['reuse', 'reference']):
                client = clients[mode]
                if not args.physical:
                    client.request(dict(action='observe', calls=[['workspace_read', {'path': f'source-{turn % 32:02d}.txt'}]]))
                    elapsed = client.rows[-1]['elapsed_us']
                else:
                    elapsed = 0
                answers[mode] = client.request(dict(action='assemble'))
                elapsed += client.rows[-1]['elapsed_us']
                measured[mode].append(elapsed)
                checked += check_view(answers[mode], expected)
            assert answers['reference'] == answers['reuse'], ('measured view mismatch', turn)
            before, _ = verified_roots(directories['reference'])
            after, _ = verified_roots(directories['reuse'])
            for key in ['working_state', 'source_index', 'transcript_manifest', 'view', 'validation']:
                assert before[key] == after[key], ('canonical component changed', key, turn)
    finally:
        for client in clients.values():
            client.close()
    results = {}
    for mode, directory in directories.items():
        manifest, count = verified_roots(directory, all_history=True)
        all_events = events(directory); checkpoints = [e for e in all_events if e.get('event') == 'context-checkpoint-work']
        assert count == len(checkpoints) and all(e['committed'] and e['completed'] for e in checkpoints)
        measured_events = [e for e in all_events[starts[mode]:] if e.get('event') == 'context-checkpoint-work']
        assert len(measured_events) == args.steps * (1 if args.physical else 2), 'required checkpoint boundary changed'
        sums = lambda key: {name: sum(e[key][name] for e in measured_events) for name in measured_events[0][key]}
        samples = measured[mode]
        result = dict(checkpoints=count, measured_checkpoints=len(measured_events),
                      work=sums('work'), pages=sums('pages'), samples_micros=samples,
                      median_micros=statistics.median(samples),
                      minimum_micros=min(samples), maximum_micros=max(samples))
        if mode == 'reuse':
            assert all(e['pages']['encoded_observations'] <= 1 for e in measured_events)
            assert result['pages']['reused_observations'] >= 2 * len(measured_events)
        if args.physical:
            lineage = directory / 'run.context/writer'
            source_pages = json.loads(page.object_path(lineage, manifest['source_index']['digest']).read_bytes())['pages']
            working_pages = json.loads(page.object_path(lineage, manifest['working_state']['digest']).read_bytes())['pages']
            digests = [bytes(ref['digest']).hex() for ref in [*source_pages[:2], *working_pages[1:3]]]
            lines = (directory / 'syscalls.log').read_text().splitlines()
            creations = {digest: sum('openat(' in line and 'O_CREAT' in line and '.artifact-' in line and digest in line for line in lines) for digest in digests}
            assert all(n == 1 if mode == 'reuse' else n > 1 for n in creations.values()), creations
            result['sealed_page_temporary_creations'] = creations
            writes = {}
            for digest, count in creations.items():
                values = [int(match.group(1)) for line in lines
                          if '.artifact-' in line and digest in line and 'write(' in line
                          and (match := re.search(r'\)\s+=\s+(\d+)$', line))]
                artifact = page.object_path(lineage, bytes.fromhex(digest))
                assert values and sum(values) == count * artifact.stat().st_size
                writes[digest] = sum(values)
            result['sealed_page_temporary_write_bytes'] = writes
        results[mode] = result
    assert results['reuse']['work']['completed_write_bytes'] < results['reference']['work']['completed_write_bytes']
    assert {p.name: p.read_text() for p in workspace.iterdir()} == expected
    result = dict(passed=True, physical=args.physical, alternating_order=True, steps=args.steps,
                  independent_source_deliveries=checked, results=results,
                  consumer_sha256=hashlib.sha256(args.consumer.read_bytes()).hexdigest(),
                  scope='actual public context APIs, exact component bytes and all published dependency roots')
    (root / 'result.json').write_text(json.dumps(result, indent=2)); print(json.dumps(result, indent=2))


def warm_corruption(args, root):
    original = json.loads((args.seed / 'input.json').read_text())
    results = []
    kinds = ['symlink'] if args.expect_bug else ['remove', 'truncate', 'corrupt', 'replace', 'symlink', 'directory', 'fifo']
    for reuse in [False, True]:
        for kind in kinds:
            directory = root / f'{reuse}-{kind}'; shutil.copytree(args.seed, directory)
            spec = dict(original, trace=str(directory / 'run.trace'), command_state=str(directory / 'commands'),
                        local_context={'reuse_checkpoint_pages': reuse})
            path = directory / 'input.json'; path.write_text(json.dumps(spec))
            client = Client(args.consumer.resolve(), path, directory)
            try:
                client.request(dict(action='assemble'))
                lineage, manifest, before, _ = page.checkpoint(directory)
                refs = json.loads(page.object_path(lineage, manifest['working_state']['digest']).read_bytes())['pages']
                target = page.object_path(lineage, refs[1]['digest']); data = target.read_bytes(); stat = target.stat()
                if kind == 'remove': target.unlink()
                if kind == 'truncate': target.write_bytes(data[:len(data)//2])
                if kind == 'corrupt':
                    damaged = bytearray(data); damaged[len(data)//2] ^= 1
                    target.write_bytes(damaged); os.utime(target, ns=(stat.st_atime_ns, stat.st_mtime_ns))
                if kind == 'replace':
                    target.unlink(); target.write_bytes(b'X' + data[1:])
                if kind == 'symlink':
                    saved = directory / 'exact-page'; saved.write_bytes(data); target.unlink(); target.symlink_to(saved)
                if kind == 'directory': target.unlink(); target.mkdir()
                if kind == 'fifo': target.unlink(); os.mkfifo(target)
                repairs_missing = kind == 'remove' and not reuse
                client.request(dict(action='assemble'), rejected=not args.expect_bug and not repairs_missing)
                _, _, after, _ = page.checkpoint(directory)
                if repairs_missing:
                    assert target.read_bytes() == data and after != before
                else:
                    assert (after != before) if args.expect_bug else (after == before)
                results.append(dict(reuse=reuse, kind=kind, rejected=not args.expect_bug and not repairs_missing,
                                    regenerated_exact_missing_page=repairs_missing))
            finally:
                client.close()
    result = dict(passed=True, expected_prior_bug=args.expect_bug, cases=results,
                  consumer_sha256=hashlib.sha256(args.consumer.read_bytes()).hexdigest())
    (root / 'result.json').write_text(json.dumps(result, indent=2)); print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--steps', type=int, default=32)
    parser.add_argument('--physical', action='store_true')
    parser.add_argument('--warm-corruption', action='store_true')
    parser.add_argument('--expect-bug', action='store_true')
    parser.add_argument('--seed', type=Path)
    args = parser.parse_args(); root = args.output.resolve(); root.mkdir(parents=True, exist_ok=False)
    if args.warm_corruption:
        assert args.seed; warm_corruption(args, root)
    else:
        paired(args, root)


if __name__ == '__main__':
    main()
