#!/usr/bin/env python3
"""Reopen actual committed context pages; damage only owned fixture copies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess


def object_path(lineage, digest):
    value = bytes(digest).hex()
    return lineage / 'artifacts/objects/sha256' / value[:2] / value


def checkpoint(private, role='writer'):
    lineage = private / 'run.context' / role
    with sqlite3.connect(f'file:{lineage / "journal.sqlite3"}?mode=ro', uri=True) as database:
        value, position = database.execute(
            'SELECT value,producing_position FROM state_records WHERE namespace=3401').fetchone()
        roots = {row[0] for row in database.execute(
            'SELECT r.artifact_digest FROM artifact_references r '
            'JOIN commands c ON c.batch_hash=r.owner_identity WHERE r.owner_kind=1 '
            'AND c.first_position<=? AND c.last_position>=?', (position, position))}
    return lineage, json.loads(value), value, roots


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--baseline-consumer', type=Path)
    parser.add_argument('--seed', type=Path, required=True,
                        help='Completed efficiency.py private directory, never live state')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expected-source-marker', default='INDEPENDENT-SOURCE-00-ROW-000',
                        help='Independent expected content for source-00.txt in the supplied fixture')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    seed = args.seed.resolve(strict=True)
    spec = json.loads((seed / 'input.json').read_text())
    lineage, manifest, encoded, roots = checkpoint(seed)
    expected = {hashlib.sha256(encoded).digest()}
    pages = {}
    for name in ['working_state', 'source_index', 'transcript_manifest', 'view', 'validation']:
        reference = manifest[name]
        digest = bytes(reference['digest'])
        expected.add(digest)
        content = object_path(lineage, digest).read_bytes()
        assert len(content) == reference['bytes'] and hashlib.sha256(content).digest() == digest, name
        if name in ['working_state', 'source_index']:
            pages[name] = json.loads(content)['pages']
            for page in pages[name]:
                digest = bytes(page['digest'])
                content = object_path(lineage, digest).read_bytes()
                assert len(content) == page['bytes'] and hashlib.sha256(content).digest() == digest
                expected.add(digest)
    assert roots == expected, ('checkpoint dependency roots differ', len(roots), len(expected))
    results = [{'case': 'exact-current-checkpoint-roots', 'roots': len(roots), 'ok': True}]
    cases = [('fresh-process-reuse', True, None), ('rollback-from-current-state', False, None),
             ('missing-page', True, 'remove'), ('truncated-page', True, 'truncate'),
             ('same-size-preserved-mtime-corrupt-page', True, 'corrupt'),
             ('replaced-page', True, 'replace')]
    if args.baseline_consumer:
        cases.insert(2, ('older-binary-current-checkpoint', False, None))
    for label, reuse, damage in cases:
        private = output / label
        shutil.copytree(seed, private)
        local = dict(spec, trace=str(private / 'run.trace'), command_state=str(private / 'commands'),
                     generation=2, operations=[['workspace_read', {'path': 'source-00.txt'}]],
                     local_context={'reuse_checkpoint_pages': reuse})
        binary = args.consumer
        if label == 'older-binary-current-checkpoint':
            binary = args.baseline_consumer
            del local['local_context']
        (private / 'input.json').write_text(json.dumps(local))
        local_lineage, _, previous, _ = checkpoint(private)
        if damage:
            target = object_path(local_lineage, pages['working_state'][1]['digest'])
            content = target.read_bytes()
            metadata = target.stat()
            if damage == 'remove':
                target.unlink()
            elif damage == 'truncate':
                target.write_bytes(content[:len(content) // 2])
            elif damage == 'corrupt':
                damaged = bytearray(content)
                damaged[len(damaged) // 2] ^= 1
                target.write_bytes(damaged)
                os.utime(target, ns=(metadata.st_atime_ns, metadata.st_mtime_ns))
            elif damage == 'replace':
                target.unlink()
                target.write_bytes(b'UNRELATED-ARTIFACT-' + content[19:])
        process = subprocess.run([str(binary.resolve()), 'context', str(private / 'input.json')],
                                 capture_output=True, timeout=120)
        (private / 'stdout.json').write_bytes(process.stdout)
        (private / 'stderr.log').write_bytes(process.stderr)
        _, current, current_encoded, _ = checkpoint(private)
        if damage:
            assert process.returncode != 0, (label, 'corruption was accepted')
            assert b'artifact' in process.stderr or b'checkpoint' in process.stderr, (label, process.stderr)
            assert current_encoded == previous, (label, 'failed recovery changed committed checkpoint')
        else:
            assert process.returncode == 0, (label, process.stderr)
            response = json.loads(process.stdout)
            assert args.expected_source_marker in response['views'][-1]['text'], label
            assert current['generation'] > manifest['generation'], (label, 'did not continue current history')
        results.append({'case': label, 'ok': True, 'returncode': process.returncode})
    result = {'consumer_sha256': hashlib.sha256(args.consumer.read_bytes()).hexdigest(),
              'seed_generation': manifest['generation'], 'results': results,
              'scope': 'real public context API; owned artifact copies; no inference or live user state'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
