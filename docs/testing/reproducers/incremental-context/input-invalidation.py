#!/usr/bin/env python3
"""Change actual host conversation revision while one production memory owner remains open."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import selectors
import shutil
import sqlite3
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-bug', action='store_true')
    parser.add_argument('--legacy-consumer', type=Path)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    workspace = root / 'workspace'
    workspace.mkdir()
    (workspace / 'alpha.txt').write_text('INDEPENDENT-INPUT-FACT-270481\n')
    spec = {'root': str(workspace), 'trace': str(root / 'private/run.trace'),
            'command_state': str(root / 'private/commands'), 'capacity': 200000,
            'include_memory_tools': True, 'local_context': {'reuse_checkpoint_pages': True},
            'task': 'Inspect alpha.txt; the first conversation selects mode OLD.'}
    path = root / 'spec.json'
    path.write_text(json.dumps(spec))
    log = (root / 'interface.jsonl').open('w')
    errors = (root / 'stderr.log').open('w')
    child = subprocess.Popen([str(args.consumer.resolve()), str(path)], text=True,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)

    def invoke(command):
        child.stdin.write(json.dumps(command) + '\n')
        child.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            assert selector.select(120), command
        line = child.stdout.readline()
        assert line, ('consumer exited', child.poll())
        response = json.loads(line)
        log.write(json.dumps({'request': command, 'response': response}) + '\n')
        log.flush()
        assert response['answer']['ok'], response
        return response['answer']['value']

    def status(view):
        for message in view['messages']:
            for block in message['blocks']:
                text = block.get('text', '')
                if not text.startswith('LOCAL WORKING STATE'):
                    continue
                for line in text.splitlines():
                    if line.startswith('{'):
                        entry = json.loads(line)
                        if entry.get('text') == 'INDEPENDENT-OLD-CONCLUSION-960744':
                            return entry['status']
        return None

    def pages():
        lineage = root / 'private/run.context/writer'
        with sqlite3.connect(f'file:{lineage / "journal.sqlite3"}?mode=ro', uri=True) as db:
            manifest = json.loads(db.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()[0])
        def load(reference):
            digest = bytes(reference['digest']).hex()
            return json.loads((lineage / 'artifacts/objects/sha256' / digest[:2] / digest).read_bytes())
        return load(manifest['working_state'])['pages'][1], manifest['source_index']

    try:
        invoke({'action': 'observe', 'calls': [['workspace_read', {'path': 'alpha.txt'}]]})
        view = invoke({'action': 'assemble'})
        handle = next(json.loads(block['output'])['local_context']['handle']
                      for message in view['messages'] for block in message['blocks']
                      if block.get('type') == 'result')
        revision = int(re.search(r'context_update base_revision=(\d+)', json.dumps(view)).group(1))
        update = {'base_revision': revision, 'operations': [{'id': 'old-input-conclusion',
                  'kind': 'assertion', 'text': 'INDEPENDENT-OLD-CONCLUSION-960744',
                  'supports': [handle], 'contradicts': [], 'depends_on': [], 'status': 'open',
                  'validity': 'conversation', 'files': [], 'supersedes': None}]}
        output = invoke({'action': 'plan', 'tool': 'context_update', 'args': update})
        assert not output['outputs'][0].get('error'), output
        before = invoke({'action': 'assemble'})
        assert status(before) == 'open', before
        old_pages = pages()
        invoke({'action': 'revision', 'value': 2})
        after = invoke({'action': 'assemble', 'governing': 'CURRENT-INPUT-REVISION-2 selects mode NEW.'})
        assert 'CURRENT-INPUT-REVISION-2' in json.dumps(after)
        assert pages() == old_pages, 'conversation refresh must preserve unchanged observation/source page identities'
        actual = status(after)
        reproduced = actual == 'open'
        page = invoke({'action': 'plan', 'tool': 'context_read', 'args': {
            'observation_ids': [], 'query': None, 'cursor': None, 'offset': 0, 'max_bytes': 16384}})['outputs'][0]
        retained = next(entry for entry in page['entries'] if entry['text'] == 'INDEPENDENT-OLD-CONCLUSION-960744')
        assert retained['status'] == ('open' if args.expect_bug else 'stale'), retained
        invoke({'action': 'revision', 'value': 3})
        restored = invoke({'action': 'assemble', 'governing': 'The conversation again selects mode OLD.'})
        if not args.expect_bug:
            assert status(restored) is None, 'staleness must remain sticky after returning to old text'
        if not args.expect_bug:
            child.stdin.close()
            assert child.wait(timeout=30) == 0
            spec['revision'] = 3
            path.write_text(json.dumps(spec))
            child = subprocess.Popen([str(args.consumer.resolve()), str(path)], text=True,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)
            recovered = invoke({'action': 'assemble', 'governing': 'The conversation again selects mode OLD.'})
            assert status(recovered) is None, 'a fresh process must retain invalidation'
        legacy_ok = None
        if args.legacy_consumer:
            child.stdin.close()
            assert child.wait(timeout=30) == 0
            legacy = root / 'legacy'
            shutil.copytree(root / 'private', legacy)
            old_spec = dict(spec, trace=str(legacy / 'run.trace'), command_state=str(legacy / 'commands'),
                            generation=2, operations=[['workspace_read', {'path': 'alpha.txt'}]])
            old_spec.pop('local_context')
            old_path = root / 'legacy-spec.json'
            old_path.write_text(json.dumps(old_spec))
            old = subprocess.run([str(args.legacy_consumer.resolve()), 'context', str(old_path)],
                                 capture_output=True, timeout=120)
            (root / 'legacy.stdout').write_bytes(old.stdout)
            (root / 'legacy.stderr').write_bytes(old.stderr)
            assert old.returncode == 0, old.stderr
            assert 'INDEPENDENT-INPUT-FACT-270481' in json.loads(old.stdout)['views'][-1]['text']
            legacy_ok = True
        result = {'bug_reproduced': reproduced, 'old_status_after_new_input': actual,
                  'current_literal_input_present': True, 'provider_inference': False,
                  'retained_status': retained['status'], 'fresh_process_recovery': not args.expect_bug,
                  'unchanged_source_pages': True,
                  'legacy_reader_continues': legacy_ok,
                  'consumer_sha256': hashlib.sha256(args.consumer.read_bytes()).hexdigest()}
        (root / 'result.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2))
        assert reproduced if args.expect_bug else not reproduced, 'conversation-dependent conclusion freshness violated'
    finally:
        child.stdin.close()
        if child.poll() is None:
            assert child.wait(timeout=30) == 0
        log.close()
        errors.close()


if __name__ == '__main__':
    main()
