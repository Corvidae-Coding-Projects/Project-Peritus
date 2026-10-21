#!/usr/bin/env python3
"""Replay provenance-linked private archive leakage through the actual native consumer.

The oracle is established before calls: adding a private source on cold reopening
must exclude its original bytes, encoded ToolMessage, and retrieval wrappers.
No provider inference, synthetic tool outputs, or implementation substitutes.
"""
import argparse
import hashlib
import json
import selectors
import subprocess
import traceback
from pathlib import Path


def require(value, message):
    if not value:
        raise AssertionError(message)


def outputs(view):
    return [json.loads(block['output']) for message in view for block in message['blocks']
            if block['type'] == 'result']


def source_bytes(item):
    return bytes.fromhex(item['data']) if item['encoding'] == 'hex' else item['data'].encode('utf-8')


def has_marker(value, marker):
    if isinstance(value, dict):
        if value.get('encoding') in ('hex', 'utf8') and isinstance(value.get('data'), str):
            if marker.encode('utf-8') in source_bytes(value):
                return True
        return any(has_marker(child, marker) for child in value.values())
    if isinstance(value, list):
        return any(has_marker(child, marker) for child in value)
    return isinstance(value, str) and marker in value


def context(handles=None, query=None, cursor=None):
    return {'observation_ids': handles or [], 'query': query, 'cursor': cursor,
            'offset': 0, 'max_bytes': 16384}


class Session:
    def __init__(self, consumer, output, name):
        self.consumer = consumer
        self.path = output / name
        self.path.mkdir()
        self.workspace = self.path / 'workspace'
        self.workspace.mkdir()
        (self.workspace / 'alpha.txt').write_text('LOCAL-BODY-ORACLE-48917\n', encoding='utf-8')
        self.external = self.path / 'brief.txt'
        self.external.write_text('EXTERNAL-BODY-ORACLE-19734\n', encoding='utf-8')
        self.spec = {'root': str(self.workspace), 'trace': str(self.path / 'private/run.bin'),
                     'task': f'Use alpha.txt and the exact external reference `{self.external}`.',
                     'capacity': 200000}
        self.actions = 0
        self.processes = 0
        self.start()

    def start(self):
        self.processes += 1
        (self.path / 'spec.json').write_text(json.dumps(self.spec), encoding='utf-8')
        self.log = (self.path / f'actions-{self.processes}.jsonl').open('w', encoding='utf-8')
        self.stderr = (self.path / f'stderr-{self.processes}.log').open('w', encoding='utf-8')
        self.process = subprocess.Popen([str(self.consumer), str(self.path / 'spec.json')],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr, text=True)

    def ask(self, value):
        self.actions += 1
        self.process.stdin.write(json.dumps(value) + '\n')
        self.process.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            require(selector.select(timeout=30), f'consumer response timeout: {self.path}')
        line = self.process.stdout.readline()
        require(line, f'consumer ended: {self.path}, exit={self.process.poll()}')
        answer = json.loads(line)
        self.log.write(json.dumps({'input': value, 'output': answer}) + '\n')
        self.log.flush()
        require(answer['answer']['ok'], f'native consumer rejected command: {answer}')
        return answer['answer']['value']

    def observe(self, tool, arguments):
        return self.ask({'action': 'observe', 'calls': [[tool, arguments]]})['outputs'][0]

    def view(self):
        return self.ask({'action': 'assemble'})['messages']

    def stop(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=5)
                raise
        self.log.close()
        self.stderr.close()
        require(self.process.returncode == 0, f'consumer exit {self.process.returncode}: {self.path}')

    def reopen_private(self, path):
        self.stop()
        self.spec['private_paths'] = [str(path)]
        self.start()


def replay_case(consumer, output, kind):
    session = Session(consumer, output, f'private-archive-{kind}')
    marker = 'LOCAL-BODY-ORACLE-48917' if kind == 'local' else 'EXTERNAL-BODY-ORACLE-19734'
    path = 'alpha.txt' if kind == 'local' else str(session.external)
    absolute = session.workspace / 'alpha.txt' if kind == 'local' else session.external
    try:
        result = session.observe('workspace_read', {'path': path})
        require(not result['is_error'] and has_marker(result, marker), 'initial fixture read failed')
        initial_view = outputs(session.view())
        references = [value['local_context']['handle'] for value in initial_view
                      if value.get('content') and has_marker(value, marker)]
        # The original program had a separate external-body visibility defect. Its
        # stale receipt still names the actual archive source, which is validated
        # against fixture bytes below; this replay does not adopt that stale verdict.
        references += [value['original_observation'] for value in initial_view
                       if isinstance(value.get('original_observation'), str)]
        require(references, 'provider view exposes no archive source handle')
        original = references[0]
        read = session.observe('context_read', context([original]))
        require(not read['is_error'] and has_marker(read, marker), 'initial archive read failed')
        session.view()
        scope, number = original.rsplit(':', 1)
        # Each ordinary consumer exchange archives assistant/output/ToolMessage.
        # Retrieve and validate the actual source before relying on its ordinal;
        # memory receipts intentionally do not advertise themselves as new evidence.
        receipt = f'{scope}:{int(number) + 3:06}'
        wrapper = f'{scope}:{int(number) + 1:06}'
        wrapped = session.observe('context_read', context([wrapper]))
        require(not wrapped['is_error'] and has_marker(wrapped, marker), 'ToolMessage source unavailable')
        payload = source_bytes(wrapped['output']['sources'][0])
        require(payload.startswith(b'P5MS') and original.encode() in payload,
                'adjacent source is not the actual encoded ToolMessage wrapper')
        session.view()
        transitive = session.observe('context_read', context([receipt]))
        require(not transitive['is_error'] and has_marker(transitive, marker),
                'initial transitive archive read failed')
        session.view()
        prior_payload = json.loads(source_bytes(transitive['output']['sources'][0]))
        require(prior_payload.get('sources', [{}])[0].get('handle') == original,
                'candidate receipt is not the actual prior context_read output')
        second_receipt = f'{scope}:{int(number) + 9:06}'
        second = session.observe('context_read', context([second_receipt]))
        require(not second['is_error'] and has_marker(second, marker),
                'second retrieval receipt is unavailable')
        second_payload = json.loads(source_bytes(second['output']['sources'][0]))
        require(second_payload.get('sources', [{}])[0].get('handle') == receipt,
                'candidate transitive receipt does not point at the prior receipt')
        session.view()
        handles = {'original_tool_output': original, 'tool_message_wrapper': wrapper,
                   'prior_context_read_output': receipt, 'transitive_context_read_output': second_receipt}
        state = session.observe('context_read', context())['output']
        operation = {'id': 'private-supported-entry', 'kind': 'observation',
                     'text': marker + ': exact source-backed note.', 'supports': [original],
                     'contradicts': [], 'depends_on': [], 'status': 'open', 'validity': 'task',
                     'files': [], 'supersedes': None}
        updated = session.observe('context_update', {'base_revision': state['base_revision'],
                                                     'operations': [operation]})
        require(not updated['is_error'], f'could not create source-backed entry: {updated}')
        session.view()
        (session.path / 'oracle.json').write_text(json.dumps({
            'marker': marker, 'private_path': str(absolute), 'handles': handles,
            'expected': 'No current source or supported-entry byte page reveals the private marker.'
        }, indent=2) + '\n', encoding='utf-8')
        session.reopen_private(absolute)
        view = session.view()
        mismatches = []
        if has_marker(view, marker):
            mismatches.append('assembled provider view contains provenance-linked private bytes')
        for label, handle in handles.items():
            result = session.observe('context_read', context([handle]))
            if has_marker(result, marker):
                mismatches.append(f'{label} reveals private bytes')
        # Search returns actual archive handles. Free-form prose is not a taint-erasure
        # contract; the invariant concerns these known source/receipt identities.
        cursor = None
        known = set(handles.values())
        search_sources = []
        for _ in range(32):
            result = session.observe('context_read', context(query=marker, cursor=cursor))
            value = result['output']
            for item in value.get('sources', []):
                search_sources.append(item['handle'])
                if item['handle'] in known and has_marker(item, marker):
                    mismatches.append(f'literal search reveals private source {item["handle"]}')
            cursor = value.get('cursor')
            if cursor is None:
                break
        else:
            raise AssertionError('bounded fixture search exceeded 32 continuation pages')
        result = session.observe('context_read', context())
        if has_marker(result, marker):
            mismatches.append('source-supported state entry reveals private bytes')
        return {'kind': kind, 'status': 'FAIL' if mismatches else 'PASS',
                'mismatches': mismatches, 'handles': handles, 'search_returned_handles': search_sources,
                'actions': session.actions, 'processes': session.processes}
    finally:
        session.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    consumer = args.consumer.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    sha = hashlib.sha256(consumer.read_bytes()).hexdigest()
    cases = []
    for kind in ('local', 'external'):
        try:
            row = replay_case(consumer, output, kind)
        except Exception as error:
            row = {'kind': kind, 'status': 'BLOCKED', 'error': repr(error),
                   'traceback': traceback.format_exc()}
        cases.append(row)
        print(json.dumps(row), flush=True)
    result = {'consumer': str(consumer), 'consumer_sha256': sha, 'cases': cases,
              'status': 'PASS' if all(row['status'] == 'PASS' for row in cases) else 'FAIL'}
    (output / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'status': result['status'], 'output': str(output)}), flush=True)
    raise SystemExit(0 if result['status'] == 'PASS' else 1)


if __name__ == '__main__':
    main()
