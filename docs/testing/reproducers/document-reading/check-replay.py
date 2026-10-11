#!/usr/bin/env python3
"""Check real Peritus trace frames and model receipts against generated source files."""
import argparse
import json
from pathlib import Path
import struct

parser = argparse.ArgumentParser()
parser.add_argument('fixture', type=Path)
parser.add_argument('run', type=Path)
parser.add_argument('trace', type=Path)
parser.add_argument('--mode', choices=('ordinary','large','ranges','input-limit'), required=True)
args = parser.parse_args()
run = json.loads(args.run.read_text())
expected = json.loads((args.fixture/'oracle.json').read_text())['files']
raw = args.trace.read_bytes()
position = 0
reads = []
while position < len(raw):
    if position+9 > len(raw):
        raise RuntimeError('Incomplete real trace frame header')
    tag = raw[position]
    size = struct.unpack_from('<Q',raw,position+1)[0]
    position += 9
    payload = raw[position:position+size]
    position += size
    if len(payload) != size:
        raise RuntimeError('Incomplete real trace frame payload')
    if tag == 2:
        tool = json.loads(payload)
        if tool['name'] == 'workspace_read':
            reads.append({'arguments':json.loads(tool['arguments']), 'output':json.loads(tool['output']), 'is_error':tool['is_error']})

def complete_read(name):
    matching = [r for r in reads if r['arguments']['path']==name]
    if len(matching) != 1 or matching[0]['is_error']:
        raise RuntimeError(f'{name} did not have exactly one successful actual read')
    observation = matching[0]['output']
    source = (args.fixture/'workspace'/name).read_text().splitlines()
    content = '\n'.join(f'{n}: {line}' for n,line in enumerate(source,1))
    if observation['content'] != content or observation['next_line'] is not None:
        raise RuntimeError(f'{name} host observation omitted or changed source text')
    if observation['end_line'] != len(source):
        raise RuntimeError(f'{name} did not reach its actual last line')
    return observation

if args.mode == 'input-limit':
    complete_read('multimegabyte.md')
    if run['phase'] != 8 or 'openai.codex_runtime.input_limit' not in run['summary']:
        raise RuntimeError('Native Codex size rejection was not classified as input_limit')
    if 'Read smaller document pages' not in run['summary']:
        raise RuntimeError('Size rejection did not explain the actionable input constraint')
    print('PASS: complete multi-megabyte host read; actual upstream rejection identified correctly')
else:
    if run['phase'] != 11:
        raise RuntimeError('Read-only Peritus run did not reach its ordinary idle completion')
    reply = None
    if args.mode != 'ranges':
        answer = next(a[2] for a in reversed(run['interaction']['activities']) if a[2].startswith(('{','FIRST_MARKER=')))
        reply = json.loads(answer) if answer.startswith('{') else dict(line.split('=',1) for line in answer.splitlines() if '=' in line)
    if args.mode == 'ordinary':
        for name in ('short.md','design.md','long.md'):
            complete_read(name)
        for name in ('short.md','design.md','long.md','escaped.md'):
            for label,value in expected[name]['markers'].items():
                if reply[name][label.upper()+'_MARKER'] != value:
                    raise RuntimeError(f'{name} model did not receive the independently chosen {label} marker')
        chunks = [r for r in reads if r['arguments']['path']=='escaped.md']
        if len(chunks) != 2:
            raise RuntimeError('Escaped text was not read in the two requested pages')
        left = chunks[0]['output']['content'].removeprefix('1: ').encode()
        right = chunks[1]['output']['content'].removeprefix('1: ').encode()
        cursor = chunks[0]['output']['next_line_byte_offset']
        if len(left) != cursor or chunks[1]['arguments']['line_byte_offset'] != cursor:
            raise RuntimeError('UTF-8 byte continuation skipped or repeated data')
        if left+right != (args.fixture/'workspace/escaped.md').read_bytes().removesuffix(b'\n'):
            raise RuntimeError('UTF-8 page reconstruction differs from the actual file')
        blank = next(r['output'] for r in reads if r['arguments']['path']=='blank.md')
        if blank['next_line'] <= blank['end_line'] or len(json.dumps(blank,ensure_ascii=False,separators=(',',':')).encode()) > 4096:
            raise RuntimeError('Blank-line document exceeded its page or lost its continuation')
        print('PASS: complete documents, model markers, exact UTF-8 continuation and bounded blank page')
    elif args.mode == 'large':
        complete_read('large-supported.md')
        for label,value in expected['large-supported.md']['markers'].items():
            if reply[label.upper()+'_MARKER'] != value:
                raise RuntimeError(f'Model did not receive the large document {label} marker')
        print('PASS: complete large document in one actual default read, with independent model marker receipt')
    else:
        ranged = next(r for r in reads if r['arguments']['path']=='long.md')
        observation = ranged['output']
        source = (args.fixture/'workspace/long.md').read_text().splitlines()
        expected_range = '\n'.join(f'{n}: {source[n-1]}' for n in range(1490,1511))
        if observation['content'] != expected_range or observation['end_line'] != 1510 or observation['next_line'] is not None:
            raise RuntimeError('Explicit inclusive line range was not honored')
        empty = next(r for r in reads if r['arguments']['path']=='empty.md')['output']
        invalid = next(r for r in reads if r['arguments']['path']=='invalid.txt')
        if empty['content'] != '' or empty['next_line'] is not None:
            raise RuntimeError('Empty document returned content or an invented continuation')
        if not invalid['is_error'] or 'not UTF-8' not in str(invalid['output']):
            raise RuntimeError('Invalid UTF-8 was not rejected explicitly')
        print('PASS: exact inclusive range, empty document and real invalid UTF-8 rejection')
