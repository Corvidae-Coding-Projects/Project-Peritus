#!/usr/bin/env python3
"""Replay real Codex transcripts and varied notification placements against its decoder."""
import argparse
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('consumer', type=Path)
parser.add_argument('supported_stdout', type=Path)
parser.add_argument('supported_final', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
consumer = args.consumer.resolve()
args.output.mkdir(parents=True, exist_ok=False)
good = [json.loads(line) for line in args.supported_stdout.read_text().splitlines() if line]
bad_path = Path(__file__).with_name('codex-model-unavailable.jsonl')
bad = [json.loads(line) for line in bad_path.read_text().splitlines() if line]
notice = next(event for event in bad if event.get('item', {}).get('type') == 'error')
assert good[0]['type'] == 'thread.started' and good[-1]['type'] == 'turn.completed'
assert any(event.get('item', {}).get('type') == 'agent_message' for event in good)
cases = [('actual-success', good, 'complete'), ('actual-rejection', bad, 'ModelUnavailable')]
for position in (1, 2):
    for copies in (1, 2, 4):
        cases.append((f'notice-position-{position}-copies-{copies}',
                      good[:position] + [notice] * copies + good[position:], 'complete'))
cases += [
    ('notice-without-terminal', [good[0], notice, good[1]], 'Incomplete'),
    ('notice-then-native-command', good[:2] + [notice, {
        'type': 'item.completed', 'item': {'id': 'forbidden-command',
        'type': 'command_execution', 'command': 'must never execute'}}] + good[2:], 'NativeTool'),
    ('notice-then-unknown-item', good[:2] + [notice, {
        'type': 'item.completed', 'item': {'id': 'unknown', 'type': 'unknown_extension'}}] + good[2:], 'UnsupportedEvent'),
    ('notice-missing-message', good[:2] + [{'type': 'item.completed',
        'item': {'id': 'malformed-notice', 'type': 'error'}}] + good[2:], 'Malformed'),
    ('notice-invalid-control', good[:2] + [{'type': 'item.completed',
        'item': {'id': 'invalid-notice', 'type': 'error', 'message': '\0'}}] + good[2:], 'OutputLimit'),
    ('notice-then-safety-failure', [good[0], notice, good[1], {'type': 'error',
        'error': {'type': 'content_policy', 'message': 'Policy rejection'}}], 'Safety'),
    ('nested-model-code', [good[0], notice, good[1], {'type': 'error', 'message': json.dumps({
        'error': {'type': 'invalid_request_error', 'code': 'model_not_found',
                  'message': 'Selected identifier unavailable'}})}], 'ModelUnavailable'),
]
results = []
for name, events, expected in cases:
    path = args.output / f'{name}.jsonl'
    path.write_text(''.join(json.dumps(event, separators=(',', ':')) + '\n' for event in events))
    command = [str(consumer), 'replay', str(path), expected]
    if expected == 'complete':
        command.append(str(args.supported_final.resolve()))
    completed = subprocess.run(command, text=True, capture_output=True)
    result = {'case': name, 'expected': expected, 'exit': completed.returncode,
              'output': completed.stdout.strip(), 'error': completed.stderr.strip()}
    results.append(result)
    if completed.returncode:
        print(json.dumps(result))
        raise SystemExit('Actual decoder sequence failed; concrete input retained')
(args.output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps({'status': 'PASS', 'cases': len(results),
                  'scope': 'Actual production decoder; real captured success/rejection plus notification placement, duplication, terminal, native-tool and malformed-input boundaries'}))
