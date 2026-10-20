#!/usr/bin/env python3
"""Operate the real tool/context API; expected content comes from separately owned fixtures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import re
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--consumer', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
base = a.output.resolve()
base.mkdir(parents=True, exist_ok=False)
root = base / 'workspace'
root.mkdir()
private = base / 'private'
private.mkdir()
spec = {'root': str(root), 'trace': str(private / 'reuse.trace'), 'task': 'Implement focus.txt using the accepted design; preserve completed work. Inspect middle.txt, long.txt and auxiliary sources only when needed.', 'capacity': 200000, 'model': 'gpt-6.1-sol'}
spec_path = base / 'spec.json'
spec_path.write_text(json.dumps(spec))
log = (base / 'operations.jsonl').open('w')
checks = []
child = None
runs = 0


def start():
    global child, runs
    runs += 1
    child = subprocess.Popen([str(a.consumer.resolve()), str(spec_path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=(base / f'stderr-{runs}.log').open('w'), text=True)


def invoke(command):
    child.stdin.write(json.dumps(command, ensure_ascii=False) + '\n')
    child.stdin.flush()
    line = child.stdout.readline()
    if not line:
        raise RuntimeError(('consumer exited', child.poll(), command))
    answer = json.loads(line)
    log.write(json.dumps({'command': command, 'answer': answer}, ensure_ascii=False) + '\n')
    log.flush()
    assert answer['answer']['ok'], answer
    return answer['answer']['value']


def read(path='focus.txt', start=1, end=80, maximum=4096, error=False, **kwargs):
    result = invoke({'action': 'observe', 'calls': [['workspace_read', {'path': path, 'start_line': start, 'end_line': end, 'max_bytes': maximum, **kwargs}]]})['outputs'][0]
    assert result['is_error'] == error, result
    if not error:
        assert len(json.dumps(result['output'], ensure_ascii=False, separators=(',', ':')).encode()) <= maximum, ('page budget', result)
    return result['output']


def body(value):
    return [line.split(': ', 1)[1] for line in value.get('content', '').split('\n') if ': ' in line]


def receipt(value):
    assert value.get('read_reuse', {}).get('state') == 'already_visible' and 'content' not in value, value


def exact(value, expected):
    assert body(value) == expected, (value, expected)


def check(name):
    checks.append({'case': name, 'status': 'PASS'})


def begin():
    invoke({'action': 'begin', 'text': spec['task']})


def stop():
    if child and child.poll() is None:
        child.stdin.close()
        child.wait(timeout=30)
        assert child.returncode == 0, child.returncode


start()
try:
    lines = [f'SOURCE-LINE-{i:03d}-猫Ω' for i in range(1, 81)]
    data = ('\n'.join(lines) + '\n').encode()
    (root / 'focus.txt').write_bytes(data)
    raw=[]
    for _ in range(2):
        value=invoke({'action':'read','args':{'path':'focus.txt','start_line':1,'end_line':40,'max_bytes':4096}})
        assert not value['is_error']
        exact(value['output'],lines[:40])
        raw.append(len(value['output']['content'].encode()))
    exact(read(start=1, end=40), lines[:40])
    for _ in range(20):
        receipt(read(start=1, end=40))
    receipt(read(start=5, end=12))
    check('exact repeats and contained ranges return receipts with no duplicate body')

    for i in range(24):
        name = f'other-{i}.txt'
        (root / name).write_text(f'DISTINCT-OTHER-{i}\n')
        exact(read(name, 1, 1), [f'DISTINCT-OTHER-{i}'])
    (root / 'other-0.txt').write_text('UNRELATED-EDIT\n')
    receipt(read(start=1, end=40))
    check('more than 16 observations and unrelated edits preserve focus evidence')
    exact(read(start=30, end=50), lines[40:50])
    exact(read(), lines[50:])
    receipt(read())
    check('overlaps deliver only missing intervals and adjacent pages jointly cover a request')

    middle = [f'MIDDLE-{i:03d}' for i in range(1, 31)]
    (root / 'middle.txt').write_text('\n'.join(middle))
    exact(read('middle.txt', 10, 20), middle[9:20])
    value = read('middle.txt', 1, 30)
    exact(value, middle[:9])
    assert value['next_line'] == 21, value
    exact(read('middle.txt', 21, 30), middle[20:])
    receipt(read('middle.txt', 1, 30))
    check('a visible middle interval produces correct disjoint-gap continuation')

    begin()
    collected = []
    for _ in range(8):
        value = read()
        assert value.get('read_reuse', {}).get('state') == 'restored', value
        collected.extend(body(value))
        if value['next_line'] is None:
            break
    assert collected == lines, collected
    receipt(read())
    check('omitted evidence restores exact archived pages without falsely claiming visibility')

    begin()
    view = invoke({'action': 'assemble'})['messages']
    text = json.dumps(view, ensure_ascii=False)
    assert all(line in text for line in lines[:40]), 'fixture page not selected into archived preview'
    assert all(text.count(line) == 1 for line in lines[:40]),'context reconstruction duplicated source text'
    receipt(read(start=1, end=40))
    check('host-rendered archive previews count as visible evidence after reconstruction')

    revision = int(re.search(r'context_update base_revision=(\d+)', text).group(1))
    update = {'base_revision': revision, 'operations': [{'id': 'next-task', 'kind': 'plan', 'text': 'PROGRESS-CANARY: completed the first slice; next implement the second slice in focus.txt.', 'supports': [re.findall(r'obs:[0-9a-f]{64}:\d+', text)[0]], 'contradicts': [], 'depends_on': [], 'status': 'open', 'validity': 'files', 'files': ['focus.txt'], 'supersedes': None}]}
    updated = invoke({'action':'observe','calls':[['context_update',update]]})['outputs'][0]
    assert not updated['is_error'], updated
    begin()
    os.chmod(root / 'focus.txt',0o600)
    value=read(start=1,end=5)
    assert value['read_reuse']['state']=='restored' and value['permissions']=='0600',value
    exact(value,lines[:5])
    check('restored content refreshes live permissions metadata')
    changed = [line.replace('SOURCE', 'UPDATE') for line in lines]
    old_stat = (root / 'focus.txt').stat()
    (root / 'focus.txt').write_bytes(('\n'.join(changed) + '\n').encode())
    os.utime(root / 'focus.txt', ns=(old_stat.st_atime_ns, old_stat.st_mtime_ns))
    exact(read(start=10, end=15), changed[9:15])
    receipt(read(start=10, end=15))
    check('same-length edit with restored mtime refreshes the original request')
    progress_view=invoke({'action':'assemble'})['messages']
    progress_text=json.dumps(progress_view,ensure_ascii=False)
    assert 'PROGRESS-CANARY' in progress_text and 'stale' in progress_text.lower(), progress_text[-3000:]
    check('a source edit preserves explicitly stale task-plan progress across assembly')
    replacement = root / 'replacement'
    replacement.write_bytes(data)
    replacement.replace(root / 'focus.txt')
    exact(read(start=1, end=5), lines[:5])
    check('file replacement invalidates stale delivery coverage')

    invoke({'action': 'deny', 'value': True})
    read(start=1, end=5, error=True)
    invoke({'action': 'deny', 'value': False})
    receipt(read(start=1, end=5))
    invoke({'action': 'protect', 'paths': ['focus.txt']})
    read(start=1, end=5, error=True)
    invoke({'action': 'protect', 'paths': []})
    check('revoked read permission and protected paths cannot reuse cached evidence')
    read(start=1, end=5, error=True, unknown_field=True)
    read(start=1, end=5, maximum=0, error=True)
    read(path='../escape.txt', start=1, end=1, error=True)
    read(start=1, end=1, line_byte_offset=999999, error=True)
    check('malformed calls and invalid offsets still follow ordinary rejection')
    saved = root / 'saved-focus'
    (root / 'focus.txt').rename(saved)
    (root / 'focus.txt').symlink_to(saved)
    read(start=1, end=5, error=True)
    (root / 'focus.txt').unlink()
    saved.rename(root / 'focus.txt')
    check('symlink replacement cannot bypass normal path admission')

    long = ('猫Ω🙂-UNIQUE-' * 140).encode()
    (root / 'long.txt').write_bytes(long)
    fragments = []
    for _ in range(60):
        value = read('long.txt', 1, 1, 1024)
        piece = '\n'.join(body(value)).encode()
        assert piece, value
        fragments.append(piece)
        if value['next_line'] is None:
            break
    assert b''.join(fragments) == long, (sum(map(len, fragments)), len(long))
    receipt(read('long.txt', 1, 1, 1024))
    begin()
    restored = []
    for _ in range(80):
        value = read('long.txt', 1, 1, 1024)
        restored.append('\n'.join(body(value)).encode())
        if value['next_line'] is None:
            break
    assert b''.join(restored) == long
    check('truncated Unicode lines restore and advance with exact byte coverage and page budgets')

    middle_bytes=(''.join(f'猫-{i:04d}-Ω🙂;' for i in range(360))).encode()
    (root / 'byte-middle.txt').write_bytes(middle_bytes)
    offset=len(middle_bytes[:100].decode('utf-8',errors='ignore').encode())
    value=read('byte-middle.txt',1,1,1024,line_byte_offset=offset)
    known_bytes=set()
    for _ in range(100):
        if value.get('read_reuse',{}).get('state')=='already_visible':
            assert len(known_bytes)==len(middle_bytes)
            break
        span=value['source_range']
        lo=span['start']['byte']
        hi=len(middle_bytes) if span['end']['line']==2 else span['end']['byte']
        actual='\n'.join(body(value)).encode()
        assert actual==middle_bytes[lo:hi] and not (set(range(lo,hi)) & known_bytes),(span,lo,hi)
        known_bytes.update(range(lo,hi))
        value=read('byte-middle.txt',1,1,1024)
    else: raise AssertionError('byte gaps did not converge')
    check('a middle-of-line first read preserves its visible bytes while filling both missing sides')

    (root/'empty.txt').write_bytes(b'')
    exact(read('empty.txt',1,20),[])
    receipt(read('empty.txt',1,20))
    (root/'blank.txt').write_bytes(b'\n\n\n')
    exact(read('blank.txt',1,20),['','',''])
    receipt(read('blank.txt',1,20))
    check('empty documents and newline-only EOF observations reuse correctly')

    large=[f'PREVIEW-{i:05d}-'+('x'*90) for i in range(1,901)]
    (root/'preview.txt').write_text('\n'.join(large)+'\n')
    exact(read('preview.txt',1,900,180000),large)
    begin()
    preview_view=invoke({'action':'assemble'})['messages']
    preview_text=json.dumps(preview_view,ensure_ascii=False)
    visible_numbers={i for i,line in enumerate(large,1) if line in preview_text}
    assert visible_numbers and len(visible_numbers)<len(large),len(visible_numbers)
    value=read('preview.txt',1,900,180000)
    actual=body(value)
    assert actual and actual[0]!=large[0],value
    assert all(line not in {large[i-1] for i in visible_numbers} for line in actual)
    assert value.get('read_reuse',{}).get('state')=='restored',value
    check('a truncated archived preview credits only displayed bytes and restores the missing suffix')

    # Independent source-token oracle: a successful page may deliver only unseen lines;
    # receipts are valid exactly when every requested source line is already available.
    rng = random.Random(20261020)
    known = set()
    (root / 'generated.txt').write_text('\n'.join(f'GENERATED-{i:03d}' for i in range(1, 91)) + '\n')
    for _ in range(48):
        lo = rng.randint(1, 90)
        hi = min(90, lo + rng.randint(0, 16))
        requested = set(range(lo, hi + 1))
        missing = requested - known
        next_lo = lo
        seen_request = set()
        for _ in range(20):
            value = read('generated.txt', next_lo, hi, 4096)
            if value.get('read_reuse', {}).get('state') == 'already_visible':
                assert requested <= known, (lo, hi, known)
                break
            actual = [int(part.removeprefix('GENERATED-')) for part in body(value)]
            assert actual and not (set(actual) & known), (actual, known)
            assert set(actual) <= missing, (actual, missing)
            known.update(actual)
            seen_request.update(actual)
            if value['next_line'] is None:
                assert requested <= known, (lo, hi, known)
                break
            next_lo = value['next_line']
        else:
            raise AssertionError('generated request did not terminate')
    check('48 seeded overlapping and disjoint-gap requests satisfy an independent source-token oracle')

    stop()
    start()
    value = read(start=1, end=5)
    assert value.get('read_reuse', {}).get('state') == 'restored', value
    exact(value, lines[:5])
    receipt(read(start=1, end=5))
    check('fresh process restores role-scoped evidence and then reuses current delivery')
    invoke({'action': 'role', 'role': 'reviewer'})
    value = read(start=1, end=5)
    assert 'read_reuse' not in value, value
    exact(value, lines[:5])
    receipt(read(start=1, end=5))
    check('independent reviewer acquires its own evidence before reusing its own delivery')
    stop()
    (base / 'results.json').write_text(json.dumps({'seed': 20261020, 'generated_requests': 48, 'fresh_processes': runs, 'raw_base_executor_body_bytes': raw, 'cases': checks}, indent=2) + '\n')
    print(json.dumps({'passed': len(checks), 'results': str(base / 'results.json')}))
except BaseException:
    (base / 'results.json').write_text(json.dumps({'cases': checks, 'status': 'FAIL'}, indent=2) + '\n')
    if child and child.poll() is None:
        child.terminate()
        child.wait(timeout=10)
    raise
finally:
    log.close()
