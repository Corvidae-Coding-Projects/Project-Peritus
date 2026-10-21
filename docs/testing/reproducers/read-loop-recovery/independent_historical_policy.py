#!/usr/bin/env python3
"""Replay historical/private boundaries on copies of a completed real-provider context.

The expected read bytes come from that run's recorded workspace_write request,
not from its read output. This launches the native API consumer without inference.
The source run must be stopped and its spec must identify retained writer state.
"""
import argparse
import hashlib
import json
import selectors
import shutil
import subprocess
from pathlib import Path

from independent_archive_policy import outputs, require, source_bytes


def expected_write(trace, target):
    text = trace.read_bytes().decode('utf-8', errors='replace')
    decoder = json.JSONDecoder()
    writes = []
    for index, character in enumerate(text):
        if character != '{':
            continue
        try:
            value, _ = decoder.raw_decode(text, index)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and value.get('name') == 'workspace_write':
            arguments = json.loads(value['arguments'])
            if arguments.get('path') == target:
                writes.append(arguments['content'])
    require(len(writes) == 1, 'fixture must contain one recorded write of the target')
    return writes[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source-spec', type=Path, required=True)
    parser.add_argument('--target', default='temporary-operation.json')
    args = parser.parse_args()
    consumer = args.consumer.resolve(strict=True)
    output = args.output.resolve()
    source_spec = args.source_spec.resolve(strict=True)
    spec = json.loads(source_spec.read_text())
    trace = Path(spec['trace']).resolve(strict=True)
    workspace = Path(spec['root']).resolve(strict=True)
    require(not Path(args.target).is_absolute() and '..' not in Path(args.target).parts,
            'target must be a normal workspace-relative path')
    output.mkdir(parents=True, exist_ok=False)
    expected = expected_write(trace, args.target)
    numbered = '\n'.join(f'{index + 1}: {line}' for index, line in enumerate(expected.splitlines()))
    require(not (workspace / args.target).exists(), 'completed fixture target must remain deleted')
    oracle = {'target': args.target, 'expected_write_content': expected,
              'expected_numbered_read': numbered, 'origin': 'recorded workspace_write request',
              'origin_trace_sha256': hashlib.sha256(trace.read_bytes()).hexdigest()}
    (output / 'oracle.json').write_text(json.dumps(oracle, indent=2) + '\n')
    actions = 0
    cases = []
    source_handle = None
    for private in (False, True):
        case = output / ('protected' if private else 'control')
        case.mkdir()
        shutil.copytree(trace.parent, case / 'private', symlinks=True)
        shutil.copytree(workspace, case / 'workspace', symlinks=True)
        current = dict(spec, root=str(case / 'workspace'),
                       trace=str(case / 'private' / trace.name), live=False,
                       restore_existing=True, command_state=str(case / 'command-state'))
        if private:
            current['private_paths'] = [str(case / 'workspace' / args.target)]
        (case / 'spec.json').write_text(json.dumps(current))
        with (case / 'stderr.log').open('w') as stderr, (case / 'actions.jsonl').open('w') as log:
            process = subprocess.Popen([str(consumer), str(case / 'spec.json')],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=stderr, text=True)
            def ask(command):
                nonlocal actions
                actions += 1
                process.stdin.write(json.dumps(command) + '\n')
                process.stdin.flush()
                with selectors.DefaultSelector() as selector:
                    selector.register(process.stdout, selectors.EVENT_READ)
                    require(selector.select(timeout=30), 'native consumer response timed out')
                line = process.stdout.readline()
                require(line, 'native consumer ended before answering')
                answer = json.loads(line)
                log.write(json.dumps({'input': command, 'output': answer}) + '\n')
                log.flush()
                require(answer['answer']['ok'], f'native API rejection: {answer}')
                return answer['answer']['value']
            try:
                view = ask({'action': 'assemble'})['messages']
                historical = [value for value in outputs(view) if value.get('path') == args.target
                              and value.get('observation_time') == 'before_later_settled_workspace_effect'
                              and 'content' in value]
                if private:
                    require(not historical, 'private historical body reached provider context')
                    result = ask({'action': 'observe', 'calls': [['context_read', {
                        'observation_ids': [source_handle], 'query': None, 'cursor': None,
                        'offset': 0, 'max_bytes': 16384}]]})['outputs'][0]
                    for item in result['output'].get('sources', []):
                        value = json.loads(source_bytes(item))
                        require(value.get('content') != numbered, 'private archive source bytes leaked')
                    row = {'case': 'protected', 'status': 'PASS',
                           'private_historical_body_hidden': True,
                           'private_exact_archive_source_not_retrieved': True}
                else:
                    require(historical and all(value['content'] == numbered for value in historical),
                            'exact successful read was not preserved as historical evidence')
                    source_handle = next(value.get('local_context', {}).get('handle')
                                         or value['original_observation'] for value in historical)
                    ask({'action': 'deny', 'value': True})
                    denied = ask({'action': 'assemble'})['messages']
                    require(not any(value.get('path') == args.target and 'content' in value
                                    for value in outputs(denied)),
                            'Read revocation exposed historical source bytes')
                    row = {'case': 'control', 'status': 'PASS',
                           'historical_exact_original_write_bytes': True,
                           'host_Read_revocation_hides_historical_body': True}
                require(not (case / 'workspace' / args.target).exists(), 'replay recreated target')
                row['temp_stays_deleted'] = True
                cases.append(row)
                print(json.dumps(row), flush=True)
            finally:
                process.stdin.close()
                try:
                    process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=5)
                    raise
                require(process.returncode == 0, f'consumer exited {process.returncode}')
    require(not (workspace / args.target).exists(), 'original target changed')
    result = {'status': 'PASS', 'consumer_sha256': hashlib.sha256(consumer.read_bytes()).hexdigest(),
              'actions': actions, 'processes': 2, 'oracle_input': oracle, 'cases': cases}
    (output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
