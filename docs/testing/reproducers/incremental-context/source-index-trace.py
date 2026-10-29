#!/usr/bin/env python3
"""Reopen real native-run traces; damage only owned C7 tool records for negative cases."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess


def root_bytes(directory):
    path = directory / 'run.context/writer/journal.sqlite3'
    with sqlite3.connect(f'file:{path}?mode=ro', uri=True) as database:
        return database.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()[0]


def damage_tool_record(trace, kind):
    data, offset, output, changed = trace.read_bytes(), 0, bytearray(), False
    while offset < len(data):
        tag = data[offset]
        size = int.from_bytes(data[offset + 1:offset + 9], 'little')
        body = data[offset + 9:offset + 9 + size]
        assert len(body) == size
        if tag == 7 and not changed:
            record = json.loads(body)
            if kind == 'conflicting-outcome':
                record['is_error'] = not record['is_error']
            else:
                record['tool_sequence'] = 999999
            body = json.dumps(record, separators=(',', ':')).encode()
            changed = True
        output.extend(bytes([tag]) + len(body).to_bytes(8, 'little') + body)
        offset += 9 + size
    assert changed
    trace.write_bytes(output)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--seed', type=Path, required=True, help='indexed-projections.py live output')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    seed = args.seed.resolve()
    original = json.loads((seed / 'spec.json').read_text())
    root = Path(original['root'])
    sources = {p.name: p.read_bytes() for p in root.iterdir()}
    results, answers = [], []
    for mode in ['reference', 'indexed']:
        for kind in ['unchanged-native-trace', 'conflicting-outcome', 'noncontiguous-sequence']:
            case = output / f'{mode}-{kind}'
            private = case / 'private'
            shutil.copytree(seed / 'private', private)
            before = root_bytes(private)
            trace = private / 'run.trace'
            if kind != 'unchanged-native-trace':
                damage_tool_record(trace, kind)
            spec = dict(original, trace=str(trace), command_state=str(private / 'commands'),
                        generation=9, operations=[['workspace_read', {'path': 'alpha.txt'}],
                                                  ['workspace_read', {'path': 'beta.txt'}]])
            path = case / 'input.json'
            path.write_text(json.dumps(spec))
            process = subprocess.run([str(args.consumer.resolve()), 'context', str(path)],
                                     capture_output=True, timeout=90,
                                     env=dict(os.environ, PERITUS_CONTEXT_SOURCE_INDEX=mode))
            (case / 'stdout.json').write_bytes(process.stdout)
            (case / 'stderr.log').write_bytes(process.stderr)
            if kind == 'unchanged-native-trace':
                assert process.returncode == 0, process.stderr
                response = json.loads(process.stdout)
                for body in sources.values():
                    assert body.decode().rstrip('\n') in response['views'][-1]['text']
                inspected = json.loads(response['last_checkpoint'])
                identities = [(s['invocation'], s['tool_sequence']) for s in inspected['source_index']
                              if s['tool_sequence'] is not None]
                assert len(identities) == len(set(identities)), identities
                answers.append(response['views'])
            else:
                assert process.returncode != 0, (mode, kind, 'altered observation was accepted')
                expected = b'conflicting reuse' if kind == 'conflicting-outcome' else b'not contiguous'
                assert expected in process.stderr, (mode, kind, process.stderr)
                assert root_bytes(private) == before, 'failed recovery changed the committed root'
            assert {p.name: p.read_bytes() for p in root.iterdir()} == sources
            results.append(dict(mode=mode, case=kind, passed=True, returncode=process.returncode))
    assert answers[0] == answers[1], 'index selection changed recovered native-run views'
    result = dict(passed=True, cases=results, exact_recovered_views_equal=True,
                  consumer_sha256=hashlib.sha256(args.consumer.read_bytes()).hexdigest(),
                  scope='actual native C7 observations; C5 provider bytes left untouched; owned copies only')
    (output / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
