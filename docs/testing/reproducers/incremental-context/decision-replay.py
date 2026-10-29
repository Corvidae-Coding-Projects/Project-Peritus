#!/usr/bin/env python3
"""Operate conservative context compilation against independent disk facts and live controls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import sqlite3
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--require-host-invalidation', action='store_true')
    parser.add_argument('--expect-native-closure-bug', action='store_true')
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    workspace = root / 'workspace'
    workspace.mkdir()
    expected = {'alpha.txt': 'INDEPENDENT-DECISION-ALPHA-741600 Ω\n',
                'beta.txt': 'INDEPENDENT-DECISION-BETA-832711 Ω\n'}
    for name, body in expected.items():
        (workspace / name).write_text(body)
    spec = {'root': str(workspace), 'trace': str(root / 'private/run.trace'),
            'command_state': str(root / 'private/commands'), 'capacity': 200000,
            'task': 'Inspect alpha.txt and beta.txt. Preserve exact facts and current instructions.',
            'include_memory_tools': True, 'local_context': {'decision_view': 'conservative'}}
    spec_path = root / 'spec.json'
    spec_path.write_text(json.dumps(spec))
    errors = (root / 'stderr.log').open('w')
    log = (root / 'interface.jsonl').open('w')
    child = None
    cases = []

    def start():
        nonlocal child
        spec_path.write_text(json.dumps(spec))
        child = subprocess.Popen([str(args.consumer.resolve()), str(spec_path)], text=True,
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors)

    def stop():
        nonlocal child
        if child and child.poll() is None:
            child.stdin.close()
            assert child.wait(timeout=30) == 0
        child = None

    def invoke(command):
        child.stdin.write(json.dumps(command, ensure_ascii=False) + '\n')
        child.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            assert selector.select(120), command
        response = json.loads(child.stdout.readline())
        log.write(json.dumps({'request': command, 'response': response}, ensure_ascii=False) + '\n')
        log.flush()
        assert response['answer']['ok'], response
        return response['answer']['value']

    def assemble(**kw):
        view = invoke(dict(action='assemble', **kw))
        pending = set()
        for message in view['messages']:
            for block in message['blocks']:
                if block['type'] == 'call':
                    assert block['id'] not in pending
                    pending.add(block['id'])
                elif block['type'] == 'result':
                    assert block['id'] in pending, 'orphan native tool result'
                    pending.remove(block['id'])
        assert not pending, 'compiled request contains unclosed native tool calls'
        return view

    def observed(name, arguments):
        invoke({'action': 'observe', 'calls': [[name, arguments]]})

    def tool(name, arguments):
        result = invoke({'action': 'plan', 'tool': name, 'args': arguments})
        return result['outputs'][0]

    def key():
        events = [json.loads(line) for line in (root / 'private/run.efficiency.jsonl').read_text().splitlines()]
        return [event['key'] for event in events if event.get('event') == 'decision-reconstruction'][-1]

    def text(view):
        return '\n'.join(block.get('text', block.get('output', ''))
                         for message in view['messages'] for block in message['blocks'])

    try:
        start()
        observed('workspace_read', {'path': 'alpha.txt'})
        first = assemble()
        scope = re.search(r'Local context scope=([0-9a-f]{64})', text(first)).group(1)
        alpha_handle = next(json.loads(block['output'])['local_context']['handle']
                            for message in first['messages'] for block in message['blocks']
                            if block['type'] == 'result')
        observed('workspace_read', {'path': 'beta.txt'})
        both = assemble(state='CURRENT HOST INVOCATION STATE: independent state 295861')
        assert expected['alpha.txt'].strip() in text(both) and expected['beta.txt'].strip() in text(both)
        assert 'CURRENT HOST INVOCATION STATE: independent state 295861' in text(both)
        base = int(re.search(r'context_update base_revision=(\d+)', text(both)).group(1))
        scratch = 'WRITER-SCRATCH-CONCLUSION-519378 quotes ' + expected['beta.txt'].strip()
        update = {'base_revision': base, 'operations': [{'id': 'narrow-model-proposal', 'kind': 'plan',
                  'text': scratch, 'supports': [alpha_handle], 'contradicts': [], 'depends_on': [],
                  'status': 'open', 'validity': 'files', 'files': ['alpha.txt'], 'supersedes': None}]}
        assert not tool('context_update', update).get('rejected')
        view = assemble()
        assert expected['beta.txt'].strip() in text(view), 'model scope removed other supplied evidence'
        assert 'entire_supplied_view_and_candidate' in text(view)
        cases.append('exact facts, current host state, native closure and conservative model dependencies')

        old_key = key()
        stamp = (workspace / 'beta.txt').stat()
        for replacement in ['943822', '054933']:
            changed = expected['beta.txt'].replace('832711', replacement)
            (workspace / 'beta.txt').write_text(changed)
            os.utime(workspace / 'beta.txt', ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
            view = assemble()
            assert key() != old_key, 'undeclared supplied file change did not invalidate reconstruction'
            assert 'current_content=changed' in text(view)
            assert changed.strip() not in text(view), 'compiler invented an ordinary fresh read result'
            if args.require_host_invalidation:
                entries = []
                for message in view['messages']:
                    for block in message['blocks']:
                        body = block.get('text', '')
                        if body.startswith('LOCAL WORKING STATE'):
                            entries.extend(json.loads(line) for line in body.splitlines()
                                           if line.startswith('{'))
                claimed = next(entry for entry in entries if entry.get('text') == scratch)
                (root / 'dependency-invalidation.json').write_text(json.dumps({
                    'entry': claimed, 'actual_changed_path': 'beta.txt',
                    'independent_old_bytes': expected['beta.txt'], 'independent_current_bytes': changed,
                    'required_host_status': 'stale'}, indent=2))
                assert claimed['status'] == 'stale', \
                    'host compiler left a conclusion current after a supplied undeclared dependency changed'
            old_key = key()
        os.chmod(workspace / 'beta.txt', 0o640)
        assemble()
        assert key() != old_key, 'mode-only change did not invalidate reconstruction'
        os.chmod(workspace / 'beta.txt', 0o644)
        cases.append('two same-size restored-mtime mutations and mode-only invalidation despite narrow model claim')

        for _ in range(2):
            observed('workspace_read', {'path': 'missing.txt'})
        assemble()
        pages, cursor, through = [], None, None
        while True:
            page = tool('context_sources', {'scope': scope, 'cursor': cursor, 'max_bytes': 1400})
            owned = dict(page); owned.pop('local_context', None)
            assert len(json.dumps(owned, ensure_ascii=False, separators=(',', ':')).encode()) <= 1400
            if through is None:
                through = page['through_observation']
            assert page['through_observation'] == through
            pages.extend(page['sources'])
            cursor = page['cursor']
            if cursor is None:
                break
        assert len(pages) == through and len({page['handle'] for page in pages}) == through
        assert len([page for page in pages if page['tool'] == 'workspace_read' and page['is_error']]) == 2
        cases.append('byte-bounded complete snapshot descriptor paging and distinct actual failed attempts')

        invoke({'action': 'opaque', 'path': 'beta.txt'})
        restricted = assemble(governing='Treat beta.txt as an opaque interface; do not inspect its body.')
        assert expected['beta.txt'].strip() not in text(restricted)
        assert 'WRITER-SCRATCH-CONCLUSION-519378' not in text(restricted)
        assert expected['alpha.txt'].strip() in text(restricted)
        invoke({'action': 'deny', 'value': True})
        denied = assemble()
        assert expected['alpha.txt'].strip() not in text(denied)
        assert tool('context_sources', {'scope': scope, 'cursor': None, 'max_bytes': 1400}).get('error')
        invoke({'action': 'deny', 'value': False})
        invoke({'action': 'opaque', 'path': None})
        assert tool('context_sources', {'scope': '0' * 64, 'cursor': None, 'max_bytes': 1400}).get('rejected')
        cases.append('current opaque/read revocation suppresses unknown derived dependencies and exact excluded bytes')
        stop()

        spec['role'] = 'reviewer'
        start()
        observed('workspace_read', {'path': 'alpha.txt'})
        observed('workspace_read', {'path': 'beta.txt'})
        review = assemble()
        assert expected['alpha.txt'].strip() in text(review)
        assert 'WRITER-SCRATCH-CONCLUSION-519378' not in text(review)
        rejected = tool('context_read', {'observation_ids': [alpha_handle], 'query': None,
                        'cursor': None, 'offset': 0, 'max_bytes': 16384})
        assert rejected.get('rejected'), 'reviewer opened writer source namespace'
        cases.append('independent reviewer exact sources and rejection of writer scratch/source namespace')
        stop()

        spec['role'] = 'writer'
        spec['local_context']['decision_view'] = 'history'
        start()
        rollback = assemble()
        assert 'WRITER-SCRATCH-CONCLUSION-519378' in text(rollback), 'rollback discarded committed task progress'
        assert tool('context_read', {'observation_ids': [alpha_handle], 'query': None,
                    'cursor': None, 'offset': 0, 'max_bytes': 16384})['sources']
        cases.append('history rollback reopens current committed progress and exact evidence')
        stop()

        large = ''.join(f'INDEPENDENT-LARGE-DECISION-{row:05d} Ω ' + 'x' * 70 + '\n' for row in range(5400))
        (workspace / 'large.txt').write_text(large)
        spec['capacity'] = 25000
        spec['local_context']['decision_view'] = 'conservative'
        start()
        observed('workspace_read', {'path': 'large.txt', 'max_bytes': 524288})
        observed('workspace_read', {'path': 'alpha.txt'})
        paged = assemble()
        card = next(block['text'] for message in paged['messages'] for block in message['blocks']
                    if block.get('text', '').startswith('DECISION TOOL EVIDENCE V1')
                    and '"path":"large.txt"' in block['text'].split('\n', 4)[2])
        assert 'RAW JSON PREFIX ONLY' in card
        handle = re.search(r'source=(obs:[0-9a-f]{64}:\d+)', card).group(1)
        offset = int(re.search(r'next_offset=(\d+)', card).group(1))
        lineage = root / 'private/run.context/writer'
        with sqlite3.connect(f'file:{lineage / "journal.sqlite3"}?mode=ro', uri=True) as db:
            manifest = json.loads(db.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()[0])
        def artifact(reference):
            digest = bytes(reference['digest']).hex()
            raw = (lineage / 'artifacts/objects/sha256' / digest[:2] / digest).read_bytes()
            assert len(raw) == reference['bytes'] and hashlib.sha256(raw).hexdigest() == digest
            return raw
        source_index = json.loads(artifact(manifest['source_index']))
        source = next(source for page in source_index['pages'] for source in json.loads(artifact(page))
                      if source['sequence'] == int(handle.rsplit(':', 1)[1]))
        raw = artifact(source['artifact'])
        prefix = card.split('named retrieval tool.\n', 1)[1].encode()
        assert prefix == raw[:offset]
        value = json.loads(raw)
        assert value['source_sha256'] == hashlib.sha256(large.encode()).hexdigest()
        numbered = '\n'.join(f'{row + 1}: {line}' for row, line in enumerate(large.splitlines()))
        assert numbered.startswith(value['content']) and value['content']
        page = tool('context_read', {'observation_ids': [handle], 'query': None, 'cursor': None,
                    'offset': offset, 'max_bytes': 1024})['sources'][0]
        chunk = page['data'].encode() if page['encoding'] == 'utf8' else bytes.fromhex(page['data'])
        assert chunk == raw[offset:offset + len(chunk)] and chunk
        assert expected['alpha.txt'].strip() in text(paged), 'paging displaced the current native result'
        cases.append('provider-sized raw JSON prefix, original-byte continuation and current native closure')
        stop()
        spec['capacity'] = 200000
        spec['trace'] = str(root / 'prose/run.trace')
        spec['command_state'] = str(root / 'prose/commands')
        start()
        intent = 'PRIOR-INTENT-731926: I will read alpha.txt now and then continue the task.'
        invoke(dict(action='observe', calls=[['workspace_read', dict(path='alpha.txt')]], prose=intent))
        observed('workspace_read', dict(path='beta.txt'))
        feedback = 'HOST-FEEDBACK-719362: no prerequisite remains; continue the same task.'
        invoke(dict(action='host', text=feedback))
        final_view = assemble()
        continued = text(final_view)
        assert intent in continued and feedback in continued
        assert expected['alpha.txt'].strip() in continued
        native_calls = [block for message in final_view['messages'] for block in message['blocks']
                        if block['type']=='call' and json.loads(block['arguments']).get('path')=='beta.txt']
        native_results = [block for message in final_view['messages'] for block in message['blocks'] if block['type']=='result']
        retained = bool(native_calls and any(result['id']==native_calls[0]['id'] for result in native_results))
        (root/'native-closure.json').write_text(json.dumps(dict(retained=retained, required=True,
            host_feedback=feedback, calls=native_calls, results=native_results),indent=2))
        assert not retained if args.expect_native_closure_bug else retained, 'host feedback displaced the complete current tool exchange'
        cases.append('host feedback follows the retained complete native tool exchange; prior prose and exact cards remain')
        stop()
        result = {'passed': True, 'cases': cases, 'provider_inference': False,
                  'consumer_sha256': hashlib.sha256(args.consumer.read_bytes()).hexdigest()}
        (root / 'result.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2))
    finally:
        stop()
        log.close()
        errors.close()


if __name__ == '__main__':
    main()
