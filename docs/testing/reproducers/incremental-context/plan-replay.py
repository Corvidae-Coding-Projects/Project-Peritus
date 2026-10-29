#!/usr/bin/env python3
"""Operate actual plan/context/tool APIs against independent disposable file facts."""
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
    parser.add_argument('--legacy-consumer', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--decision-view', choices=['history', 'conservative'], default='history')
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    workspace = root / 'workspace'
    workspace.mkdir()
    expected = {'alpha.txt': 'row-one\nrow-two\nINDEPENDENT-PLAN-MATCH-739201 Ω\nlast\n',
                'beta.txt': 'INDEPENDENT-PRIVATE-PLAN-991377 \\"quoted"\n',
                'large.txt': ''.join(f'INDEPENDENT-LARGE-ROW-{n:05d} Ω\n' for n in range(600))}
    for name, value in expected.items():
        (workspace / name).write_text(value)
    spec = {'root': str(workspace), 'trace': str(root / 'private/run.trace'),
            'command_state': str(root / 'private/commands'), 'capacity': 200000,
            'task': 'Inspect the exact facts in alpha.txt, beta.txt and large.txt; retain real read outcomes.',
            'local_context': {'read_only_plans': 'sequential', 'decision_view': args.decision_view},
            'include_memory_tools': True}
    path = root / 'spec.json'
    path.write_text(json.dumps(spec))
    transcript = (root / 'interface.jsonl').open('w')
    stderr = (root / 'stderr.log').open('w')
    child = None
    cases = []

    def start():
        nonlocal child
        child = subprocess.Popen([str(args.consumer.resolve()), str(path)], stdin=subprocess.PIPE,
                                 stdout=subprocess.PIPE, stderr=stderr, text=True)

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
            assert selector.select(120), ('API consumer timed out', command)
        line = child.stdout.readline()
        assert line, ('API consumer ended', child.poll(), command)
        response = json.loads(line)
        transcript.write(json.dumps({'request': command, 'response': response}, ensure_ascii=False) + '\n')
        transcript.flush()
        assert response['answer']['ok'], response
        return response['answer']['value']

    def node(name, tool, arguments, dependencies=None, refs=None):
        return {'id': name, 'tool': tool, 'arguments': arguments,
                'depends_on': dependencies or [], 'result_refs': refs or []}

    def plan(nodes=None, cursor=None, maximum=65536, **changes):
        request = {'version': 1, 'scope': scope, 'input_revision': 1, 'plan': nodes,
                   'cursor': cursor, 'max_result_bytes': maximum}
        request.update(changes)
        result = invoke({'action': 'plan', 'args': request})
        output = result['outputs'][0]
        owned = dict(output)
        owned.pop('local_context', None)
        assert len(json.dumps(owned, ensure_ascii=False, separators=(',', ':')).encode()) <= maximum
        return output, result['host_calls']

    def read_step(step, offset=0, maximum=65536):
        return invoke({'action': 'plan', 'tool': 'inspection_result_read',
                       'args': {'step': step, 'offset': offset, 'max_bytes': maximum}})['outputs'][0]

    def checkpoint():
        db = root / 'private/run.context/writer/journal.sqlite3'
        with sqlite3.connect(f'file:{db}?mode=ro', uri=True) as connection:
            return json.loads(connection.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()[0])

    try:
        start()
        invoke({'action': 'observe', 'calls': [['workspace_list', {}]]})
        view = invoke({'action': 'assemble'})
        scope = re.search(r'Local context scope=([0-9a-f]{64})', json.dumps(view)).group(1)
        assert checkpoint()['schema_version'] == 3
        malformed = {'version': 1, 'scope': scope, 'input_revision': 1,
                     'plan': [node('read', 'workspace_read', {'path': 'alpha.txt'})],
                     'max_result_bytes': 2048}
        rejected = invoke({'action': 'plan', 'args': malformed})
        assert rejected['outputs'][0].get('error') and rejected['host_calls'] == 0
        bad = [node('first', 'workspace_read', {'path': 'alpha.txt'}),
               node('later', 'workspace_write', {'path': 'unexpected.txt', 'content': 'MUST-NOT-WRITE'})]
        result, calls = plan(bad)
        assert result.get('error') and calls == 0 and not (workspace / 'unexpected.txt').exists()
        cycle = [node('first', 'workspace_read', {'path': 'alpha.txt'}, ['later']),
                 node('later', 'workspace_read', {'path': 'beta.txt'}, ['first'])]
        result, calls = plan(cycle)
        assert result.get('error') and calls == 0 and checkpoint()['schema_version'] == 3
        conflicting = [node('find', 'workspace_search', {'path':'alpha.txt','query':'INDEPENDENT-PLAN-MATCH'}),
                       node('read', 'workspace_read', {'path':''}, ['find'],
                            [{'argument':'path','node':'find','pointer':'/matches/0/path'}])]
        result, calls = plan(conflicting)
        assert 'omit a result_refs argument' in result.get('error','') and calls == 0 and checkpoint()['schema_version'] == 3
        cases.append('whole inert graph and mutation rejection before reader upgrade or child dispatch')

        nodes = [node('find', 'workspace_search', {'path': 'alpha.txt', 'query': 'INDEPENDENT-PLAN-MATCH'}),
                 node('body', 'workspace_read', {'end_line': 3, 'max_bytes': 4096}, ['find'],
                      [{'argument': 'path', 'node': 'find', 'pointer': '/matches/0/path'},
                       {'argument': 'start_line', 'node': 'find', 'pointer': '/matches/0/line'}]),
                 node('other', 'workspace_read', {'path': 'beta.txt'})]
        result, calls = plan(nodes)
        assert result['status'] == 'completed' and result['continuation'] is None and calls == 3, result
        assert [s['id'] for s in result['steps']] == ['find', 'body', 'other']
        assert result['steps'][1]['output']['content'] == '3: INDEPENDENT-PLAN-MATCH-739201 Ω'
        assert result['steps'][1]['output']['source_sha256'] == hashlib.sha256(expected['alpha.txt'].encode()).hexdigest()
        private_step = result['steps'][2]['step']
        assert checkpoint()['schema_version'] == 4
        raw = read_step(private_step)
        assert json.loads(raw['data'])['content'] == '1: ' + expected['beta.txt'].rstrip('\n')
        assert hashlib.sha256(raw['data'].encode()).hexdigest() == raw['source_sha256']
        cases.append('dependent search/read result references, exact fixture bytes and child artifact identity')

        result, calls = plan([node('absent', 'workspace_search', {'path': 'alpha.txt', 'query': 'ABSENT-LITERAL-0000'}),
                              node('requires_match', 'workspace_read', {}, ['absent'],
                                   [{'argument': 'path', 'node': 'absent', 'pointer': '/matches/0/path'}])])
        assert result['status'] == 'needs_judgment' and result['completed_nodes'] == 1 and calls == 1
        cases.append('no-match ambiguity returns to model without inventing a target')

        result, calls = plan([node('large', 'workspace_read', {'path': 'large.txt', 'max_bytes': 65536}),
                              node('next', 'workspace_read', {'path': 'alpha.txt'})], maximum=2048)
        assert calls == 1 and result['continuation'] and 'output' not in result['steps'][0]
        large_step, cursor = result['steps'][0]['step'], result['continuation']
        pieces = []
        offset = 0
        while True:
            page = read_step(large_step, offset, 512)
            pieces.append(page['data'])
            if page['next_offset'] is None:
                break
            assert page['next_offset'] > offset
            offset = page['next_offset']
        exact = ''.join(pieces).encode()
        assert hashlib.sha256(exact).hexdigest() == page['source_sha256']
        expected_numbered = '\n'.join(f'{n + 1}: {line}' for n, line in enumerate(expected['large.txt'].splitlines()))
        assert json.loads(exact)['content'] == expected_numbered
        omega = exact.index('Ω'.encode())
        assert read_step(large_step, omega + 1).get('error')
        stop()
        start()
        invoke({'action': 'assemble'})
        result, calls = plan(cursor=cursor, maximum=2048)
        assert calls == 1 and result['completed_nodes'] == 2 and result['continuation'] is None
        cases.append('bounded aggregate, exact UTF-8 byte pages, fresh-process continuation with no child redispatch')

        replacement = expected['alpha.txt'].replace('739201', '824006')
        (workspace / 'alpha.txt').write_text(replacement)
        result, calls = plan([node('changed', 'workspace_read', {'path': 'alpha.txt'})])
        assert calls == 1 and result['steps'][0]['output']['source_sha256'] == hashlib.sha256(replacement.encode()).hexdigest()
        assert '824006' in result['steps'][0]['output']['content']
        invoke({'action': 'cancel', 'value': True})
        result, calls = plan([node('cancelled', 'workspace_read', {'path': 'alpha.txt'})])
        assert result.get('error') and calls == 0
        invoke({'action': 'cancel', 'value': False})
        result, calls = plan([node('stale_input', 'workspace_read', {'path': 'alpha.txt'})], input_revision=2)
        assert result.get('error') and calls == 0
        cases.append('current changed source bytes, cancelled admission and stale-input refusal')

        invoke({'action': 'opaque', 'path': 'beta.txt'})
        denied = read_step(private_step)
        assert denied.get('error') and 'INDEPENDENT-PRIVATE-PLAN-991377' not in json.dumps(denied)
        result, calls = plan([node('blocked', 'workspace_read', {'path': 'beta.txt'}),
                              node('unstarted', 'workspace_read', {'path': 'alpha.txt'})])
        assert result['status'] == 'needs_judgment' and calls == 1 and result['completed_nodes'] == 1
        cases.append('current opaque-interface restriction denies archived child bytes and stops dependent execution')
        invoke({'action': 'opaque', 'path': None})
        invoke({'action': 'deny', 'value': True})
        assert read_step(private_step).get('error')
        result, calls = plan([node('not_started', 'workspace_read', {'path': 'alpha.txt'})])
        assert result.get('error') and calls == 0
        invoke({'action': 'deny', 'value': False})
        assert read_step('obs:' + '0' * 64 + ':000001').get('error')
        cases.append('current read revocation and foreign-scope rejection')
        stop()

        if args.legacy_consumer:
            legacy = root / 'legacy'
            shutil.copytree(root / 'private', legacy)
            original = (legacy / 'run.context/writer/journal.sqlite3')
            with sqlite3.connect(f'file:{original}?mode=ro', uri=True) as db:
                before = db.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()[0]
            legacy_spec = dict(spec, trace=str(legacy / 'run.trace'), command_state=str(legacy / 'fresh-commands'),
                               operations=[['workspace_list', {}]])
            del legacy_spec['local_context']
            legacy_input = root / 'legacy.json'
            legacy_input.write_text(json.dumps(legacy_spec))
            old = subprocess.run([str(args.legacy_consumer.resolve()), 'context', str(legacy_input)], capture_output=True, timeout=120)
            (root / 'legacy.stderr').write_bytes(old.stderr)
            assert old.returncode != 0 and b'checkpoint' in old.stderr and b'read_only_plans' not in old.stderr
            assert b'INDEPENDENT-PRIVATE-PLAN-991377' not in old.stdout + old.stderr
            with sqlite3.connect(f'file:{original}?mode=ro', uri=True) as db:
                assert db.execute('SELECT value FROM state_records WHERE namespace=3401').fetchone()[0] == before
            cases.append('older reader explicitly refuses v4 without exposing plan data or replacing current root')

        spec['local_context']['read_only_plans'] = 'disabled'
        path.write_text(json.dumps(spec))
        start()
        invoke({'action': 'assemble'})
        assert json.loads(read_step(private_step)['data'])['content'] == '1: ' + expected['beta.txt'].rstrip('\n')
        result, calls = plan([node('disabled', 'workspace_read', {'path': 'alpha.txt'})])
        assert result.get('error') and calls == 0 and checkpoint()['schema_version'] == 4
        cases.append('execution-disabled rollback preserves current v4 history and exact child retrieval')
        stop()
        result = {'cases': cases, 'passed': True, 'provider_inference': False,
                  'consumer_sha256': hashlib.sha256(args.consumer.read_bytes()).hexdigest()}
        (root / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2))
    finally:
        if child and child.poll() is None:
            child.kill()
            child.wait(timeout=30)
        transcript.close()
        stderr.close()


if __name__ == '__main__':
    main()
