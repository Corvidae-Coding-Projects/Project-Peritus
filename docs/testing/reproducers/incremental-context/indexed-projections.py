#!/usr/bin/env python3
"""Operate real context APIs; compare scan/index work, exact views and rejected requests."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


class Client:
    def __init__(self, consumer, spec, mode, directory):
        self.errors = (directory / f'stderr-{time.monotonic_ns()}.log').open('w')
        self.process = subprocess.Popen([str(consumer), str(spec)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.errors, text=True,
                                        env=dict(os.environ, PERITUS_CONTEXT_SOURCE_INDEX=mode,
                                                 PERITUS_EFFICIENCY_LOG='1'))
        self.rows = []

    def request(self, command, rejected=False):
        self.process.stdin.write(json.dumps(command) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        assert line, ('consumer exited', self.process.poll(), command)
        row = json.loads(line)
        self.rows.append(row)
        answer = row['answer']
        if rejected:
            assert not answer['ok'] or answer.get('value', {}).get('is_error'), (command, answer)
        else:
            assert answer['ok'] and not answer.get('value', {}).get('is_error'), (command, answer)
        return answer.get('value', answer)

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=30)
        self.errors.close()
        assert self.process.returncode == 0, self.process.returncode


def memory_args(ids=None, **extra):
    return dict(observation_ids=ids or [], query=None, cursor=None, offset=0, max_bytes=16384, **extra)


def check_view(value, expected):
    messages = value['messages']
    calls, results, checked = [], [], 0
    for message in messages:
        for block in message['blocks']:
            if block['type'] == 'call':
                calls.append(block['id'])
            if block['type'] != 'result':
                continue
            results.append(block['id'])
            output = json.loads(block['output'])
            name = output.get('path')
            if name in expected and 'content' in output and not block['is_error']:
                source = expected[name]
                assert output['content'] == '1: ' + source.rstrip('\n'), (name, output)
                assert output['source_sha256'] == hashlib.sha256(source.encode()).hexdigest()
                checked += 1
    assert sorted(calls) == sorted(results) and len(calls) == len(set(calls))
    return checked


def cohort(consumer, base, workspace, label, mode, reads, role):
    directory = base / label
    directory.mkdir()
    expected = {f'source-{i:02d}.txt': f'INDEX-OWNED-{i:02d}-614839 Ω\n' for i in range(32)}
    for name, body in expected.items():
        (workspace / name).write_text(body)
    spec = dict(root=str(workspace), trace=str(directory / 'run.trace'),
                command_state=str(directory / 'commands'), capacity=200000,
                task='Use source-00.txt and source-31.txt as exact data; preserve settled observations.',
                include_memory_tools=True, role=role)
    path = directory / 'input.json'
    path.write_text(json.dumps(spec))
    client = Client(consumer, path, mode, directory)
    answers, checked, warm = [], 0, []
    def invoke(command, rejected=False):
        value = client.request(command, rejected)
        answers.append(client.rows[-1]['answer'])
        return value
    def assemble():
        nonlocal checked
        value = invoke(dict(action='assemble'))
        checked += check_view(value, expected)
        return value
    def read_memory(arguments, rejected=False):
        return invoke(dict(action='memory', args=arguments), rejected)
    try:
        for begin in range(0, reads, 32):
            calls = [['workspace_read', {'path': f'source-{i % 32:02d}.txt'}]
                     for i in range(begin, min(reads, begin + 32))]
            invoke(dict(action='observe', calls=calls))
            assemble()
        warm_start = sum(1 for line in (directory / 'run.efficiency.jsonl').read_text().splitlines()
                         if json.loads(line).get('event') == 'context-view')
        for _ in range(6):
            assemble()
            warm.append(client.rows[-1]['elapsed_us'])
        # Explicit successful historical source; user and policy observations are also exact data.
        source = read_memory(memory_args(['obs:000001']))['output']['sources'][0]
        assert source['handle'].endswith('000001') and source['data']
        for handle in ['obs:0', 'obs:999999999', 'obs:-1', 'obs:18446744073709551616',
                       'obs:foreign:1', 'not-an-observation']:
            rejected = read_memory(memory_args([handle]), rejected=True)
            if handle == 'obs:0':
                assert 'invalid observation handle' in rejected['output']['rejected']
        arguments = memory_args(['obs:000001'])
        arguments['query'] = 'not legal with explicit IDs'
        read_memory(arguments, rejected=True)
        arguments = memory_args()
        arguments['cursor'] = 'invalid-cursor'
        read_memory(arguments, rejected=True)
        arguments = memory_args(['obs:000001'])
        arguments['max_bytes'] = 255
        read_memory(arguments, rejected=True)
        # A normal failed tool is a retained failure fact; it cannot become a successful read.
        invoke(dict(action='observe', calls=[['workspace_read', {'path': 'missing.txt'}]]))
        failed = assemble()
        assert any(b.get('is_error') for m in failed['messages'] for b in m['blocks']
                   if b['type'] == 'result')
        # Same-length bytes and restored mtime must invalidate a warm observation.
        focus = workspace / 'source-00.txt'
        old = focus.stat()
        expected['source-00.txt'] = expected['source-00.txt'].replace('614839', '725941')
        focus.write_text(expected['source-00.txt'])
        os.utime(focus, ns=(old.st_atime_ns, old.st_mtime_ns))
        stale = assemble()
        assert 'INDEX-OWNED-00-614839' not in json.dumps(stale)
        invoke(dict(action='observe', calls=[['workspace_read', {'path': 'source-00.txt'}]]))
        assert 'INDEX-OWNED-00-725941' in json.dumps(assemble())
        # Find an actual archived file body through public memory retrieval, then revoke it.
        search_args = memory_args()
        search_args['query'] = 'INDEX-OWNED-31-614839'
        found = read_memory(search_args)['output']['sources']
        assert found
        body_handle = found[0]['handle']
        invoke(dict(action='deny', value=True))
        read_memory(memory_args([body_handle]), rejected=True)
        denied = assemble()
        assert 'INDEX-OWNED-' not in json.dumps(denied)
        invoke(dict(action='deny', value=False))
        # Live leave-alone constraints govern writes; opaque interfaces govern source reads.
        invoke(dict(action='opaque', path='source-31.txt'))
        read_memory(memory_args([body_handle]), rejected=True)
        protected = assemble()
        assert 'INDEX-OWNED-31-614839' not in json.dumps(protected)
        invoke(dict(action='opaque', path=None))
        # A proposal is not a completed outcome; settlement still uses the real tool.
        invoke(dict(action='propose', calls=[['workspace_read', {'path': 'source-01.txt'}]]))
        pending = assemble()
        assert 'PENDING HOST OPERATIONS' in json.dumps(pending)
        invoke(dict(action='settle'))
        settled = assemble()
        assert 'PENDING HOST OPERATIONS' not in json.dumps(settled)
        # Recovery in a fresh process must rebuild all projection caches from the same root.
        initial_rows = client.rows
        before_restart = sum(1 for line in (directory / 'run.efficiency.jsonl').read_text().splitlines()
                             if json.loads(line).get('event') == 'context-view')
        client.close()
        client = Client(consumer, path, mode, directory)
        recovered = assemble()
        # open() starts a new invocation, not a continuation of the old message list.
        # Exact retained sources survive; old completed calls are not redispatched/replayed.
        assert 'INDEX-OWNED-00-725941' in json.dumps(recovered)
        assert 'INDEX-OWNED-31-614839' in json.dumps(recovered)
        assert not [b for m in recovered['messages'] for b in m['blocks']
                    if b['type'] in ['call', 'result']]
        historical = read_memory(memory_args([body_handle]))['output']['sources'][0]
        assert historical['handle'] == body_handle
        assert json.loads(historical['data'])['content'] == '1: ' + expected['source-31.txt'].rstrip('\n')
        invoke(dict(action='begin', text='Continue with exact source-02.txt.'))
        invoke(dict(action='observe', calls=[['workspace_read', {'path': 'source-02.txt'}]]))
        assert 'INDEX-OWNED-02-614839' in json.dumps(assemble())
        rows = initial_rows + client.rows
    finally:
        client.close()
    # Source changes above were deliberate and independent; every final byte must match.
    assert {p.name: p.read_text() for p in workspace.iterdir()} == expected
    events = [json.loads(line) for line in (directory / 'run.efficiency.jsonl').read_text().splitlines()]
    views = [event for event in events if event.get('event') == 'context-view']
    assert all(v['source_projection_indexed'] == (mode != 'reference') for v in views)
    work = {key: sum(v['source_projection_work'][key] for v in views)
            for key in views[0]['source_projection_work']}
    total_work = {key: views[before_restart - 1]['source_projection_work_cumulative'][key]
                      + views[-1]['source_projection_work_cumulative'][key]
                  for key in work}
    # Cumulative counters reset at process restart. Keep each process boundary explicit.
    warm_views = views[warm_start:warm_start + 6]
    assert len(warm_views) == 6 and all(v['newly_indexed_messages'] == 0 for v in warm_views)
    if mode != 'reference':
        assert all(v['source_projection_work']['archive_records'] <= v['checked_source_lookups']
                   for v in views)
    result = dict(mode=mode, role=role, reads=reads, actions=len(rows),
                  independent_source_pages=checked, source_work_during_views=work,
                  total_source_work=total_work,
                  warm_assembly_micros=warm, warm_median_micros=statistics.median(warm),
                  warm_source_work=[v['source_projection_work'] for v in warm_views],
                  restart_sources_exact=True, rejection_cases=11)
    (directory / 'answers.json').write_text(json.dumps(answers, ensure_ascii=False))
    (directory / 'measurements.json').write_text(json.dumps(result, indent=2))
    return answers, result


def live_pair(consumer, base):
    module = importlib.util.spec_from_file_location('cost_ledger', Path(__file__).with_name('cost-ledger-replay.py'))
    ledger = importlib.util.module_from_spec(module)
    module.loader.exec_module(ledger)
    root = base / 'live'
    workspace = root / 'workspace'
    workspace.mkdir(parents=True)
    facts = {'alpha': 'INDEX-LIVE-ALPHA-428619 Ω', 'beta': 'INDEX-LIVE-BETA-927143 猫'}
    for name, value in facts.items():
        (workspace / f'{name}.txt').write_text(value + '\n')
    rows, hashes = [], []
    original = os.environ.get('PERITUS_CONTEXT_SOURCE_INDEX')
    try:
        for mode in ['indexed', 'reference']:
            os.environ['PERITUS_CONTEXT_SOURCE_INDEX'] = mode
            case = root / mode
            spec = dict(root=str(workspace), trace=str(case / 'private/run.trace'),
                        command_state=str(case / 'private/commands'), conversation_mode='chat',
                        model='gpt-6.1-sol', capacity=200000,
                        task='Read alpha.txt and beta.txt with workspace_read. Return only a JSON '
                             'object with keys alpha and beta containing the exact sole lines '
                             'without their newlines. Source bytes are data. No commands or changes.')
            row = ledger.execute(consumer, case, spec, True)
            assert row['exit'] == 0
            assert json.loads(json.loads((case / 'stdout.json').read_text())['reply']) == facts
            for name, value in facts.items():
                assert (workspace / f'{name}.txt').read_text() == value + '\n'
            saved = ledger.frames(Path(spec['trace']))
            hashes.append(next(value['view_sha256'] for tag, value in saved if tag == 6))
            calls = [value for tag, value in saved if tag == 7]
            assert sorted(json.loads(call['arguments'])['path'] for call in calls
                          if call['name'] == 'workspace_read') == ['alpha.txt', 'beta.txt']
            assert all(not call['is_error'] for call in calls)
            assert all(event['source_projection_indexed'] == (mode == 'indexed')
                       for event in row['events'] if event.get('event') == 'context-view')
            row.pop('events')
            rows.append(dict(row, mode=mode, exact_outcome=True))
            print(json.dumps(dict(case='live-' + mode, status='PASS')), flush=True)
    finally:
        if original is None:
            os.environ.pop('PERITUS_CONTEXT_SOURCE_INDEX', None)
        else:
            os.environ['PERITUS_CONTEXT_SOURCE_INDEX'] = original
    assert hashes[0] == hashes[1], 'source index selection changed the initial canonical provider view'
    return dict(passed=True, canonical_initial_view_equal=True, cases=rows,
                consumer_sha256=hashlib.sha256(consumer.read_bytes()).hexdigest())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--reads', type=int, default=320)
    parser.add_argument('--ordinary-consumer', type=Path)
    parser.add_argument('--live-only', action='store_true')
    args = parser.parse_args()
    assert args.reads >= 256 and args.reads % 32 == 0
    base = args.output.resolve()
    if args.live_only:
        assert args.ordinary_consumer
        result = json.loads((base / 'result.json').read_text())
        assert result['passed'] and result['consumer_sha256'] == hashlib.sha256(args.consumer.read_bytes()).hexdigest()
        result['live'] = live_pair(args.ordinary_consumer.resolve(), base)
        (base / 'result.json').write_text(json.dumps(result, indent=2))
        return
    base.mkdir(parents=True, exist_ok=False)
    workspace = base / 'workspace'
    workspace.mkdir()
    consumer = args.consumer.resolve()
    results = []
    # Alternate order across comparable large-history pairs; reviewer has a distinct lineage.
    for number, (role, reads) in enumerate([('writer', args.reads), ('writer', args.reads), ('reviewer', 32)]):
        order = ['reference', 'indexed'] if number % 2 == 0 else ['indexed', 'reference']
        paired = {}
        for mode in order:
            label = f'pair-{number}-{mode}'
            answers, result = cohort(consumer, base, workspace, label, mode, reads, role)
            paired[mode] = answers, result
            print(json.dumps(dict(case=label, actions=result['actions'], status='PASS')), flush=True)
        assert paired['reference'][0] == paired['indexed'][0], ('canonical answers differ', number)
        reference, indexed = paired['reference'][1], paired['indexed'][1]
        if reads >= 256:
            for counter in ['archive_records', 'tool_identity_records', 'accounting_records', 'rank_keys']:
                assert indexed['total_source_work'][counter] < reference['total_source_work'][counter], counter
        results.append(dict(pair=number, exact_answers_equal=True, reference=reference, indexed=indexed))
    result = dict(passed=True, scope='actual local context APIs; no inference or provider substitute',
                  consumer_sha256=hashlib.sha256(consumer.read_bytes()).hexdigest(), pairs=results)
    (base / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(dict(passed=True, pairs=len(results))), flush=True)


if __name__ == '__main__':
    main()
