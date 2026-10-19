#!/usr/bin/env python3
"""Drive real production APIs. Source files, expected pages, and mutations belong to this driver."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess

p = argparse.ArgumentParser()
p.add_argument('--consumer', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
base = a.output.resolve()
base.mkdir(parents=True, exist_ok=False)
root = base / 'workspace'
root.mkdir()
state = base / 'private'
state.mkdir()
spec = {'root': str(root), 'trace': str(state / 'reuse.trace'), 'task': 'Use focus.txt and other.txt as source data; preserve observations and settled effects.', 'capacity': 200000}
spec_path = base / 'spec.json'
spec_path.write_text(json.dumps(spec))
stderr = (base / 'stderr.log').open('w')
child = subprocess.Popen([str(a.consumer.resolve()), str(spec_path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True)
results = []
measurements = []
def invoke(command, expect_error=False):
    child.stdin.write(json.dumps(command, ensure_ascii=False) + '\n')
    child.stdin.flush()
    line = child.stdout.readline()
    assert line, ('consumer exited', child.poll(), command)
    result = json.loads(line)
    answer = result['answer']
    if expect_error:
        assert not answer['ok'] or answer.get('value', {}).get('is_error'), (command, answer)
    else:
        assert answer['ok'], (command, answer)
        assert not answer.get('value', {}).get('is_error'), (command, answer)
    measurements.append({'action': command.get('action', 'read'), 'rchar': result['rchar'], 'elapsed_us': result['elapsed_us']})
    return answer.get('value', answer)
def check(label):
    results.append({'case': label, 'status': 'PASS'})
def read(path, **kwargs):
    return invoke({'args': {'path': path, **kwargs}})['output']
def exact_page(path, data, start, end):
    value = read(path, start_line=start, end_line=end, max_bytes=1048576)
    lines = data.split(b'\n')
    if lines[-1] == b'': lines.pop()
    expected = '\n'.join(f'{i + 1}: {lines[i].decode()}' for i in range(start - 1, min(end, len(lines))))
    assert value['content'] == expected, (path, start, end, value['content'][:100], expected[:100])
    assert value['source_sha256'] == hashlib.sha256(data).hexdigest()
    assert value['next_line'] is None
    return value
def assemble(**kwargs):
    return invoke({'action': 'assemble', **kwargs})['messages']
def observe(calls, action='observe'):
    return invoke({'action': action, 'calls': calls})
def flat(messages):
    return json.dumps(messages, ensure_ascii=False)
try:
    rng = random.Random(731944)
    lines = []
    for i in range(12000):
        suffix = rng.choice(['alpha', 'Ω猫', '\\"quoted\\"', '\tspaced', '\r'])
        lines.append(f'line-{i + 1:05d} {suffix} ' + ('x' * (i % 101)))
    data = '\n'.join(lines).encode()
    (root / 'document.txt').write_bytes(data)
    for i in range(96):
        start = rng.randint(1, len(lines) + 5)
        exact_page('document.txt', data, start, start + rng.randint(0, 6))
    check('96 seeded ordinary/overlapping/EOF Unicode and CRLF pages')
    cold = measurements[0]['rchar']
    warm = [m['rchar'] for m in measurements[1:96]]
    assert max(warm) < len(data) // 8, ('warm reads still scan source prefixes', cold, max(warm), len(data))
    (root / 'empty.txt').write_bytes(b'')
    exact_page('empty.txt', b'', 1, 1)
    for i, content in enumerate([b'\n\n\n', b'one\r\ntwo\r\n', b'last-no-newline', '猫Ω🙂'.encode() * 5000]):
        name = f'edge-{i}.txt'
        (root / name).write_bytes(content)
        if i < 3:
            exact_page(name, content, 1, 9)
        else:
            offset = 0
            accumulated = b''
            while True:
                value = read(name, start_line=1, end_line=1, line_byte_offset=offset, max_bytes=768)
                piece = value['content'].removeprefix('1: ').encode()
                accumulated += piece
                if value['next_line'] is None: break
                assert value['next_line'] == 1 and value['next_line_byte_offset'] > offset
                offset = value['next_line_byte_offset']
            assert accumulated == content
    (root / 'invalid.txt').write_bytes(b'ok\n\xff\n')
    assert read('invalid.txt', start_line=1, end_line=1)['content'] == '1: ok'
    invoke({'args': {'path': 'invalid.txt', 'start_line': 2}}, expect_error=True)
    invoke({'args': {'path': 'edge-3.txt', 'line_byte_offset': 1, 'end_line': 1}}, expect_error=True)
    check('empty/CRLF/final-line/long-line byte continuation and UTF-8 errors')
    current = b'CURRENT-ALPHA-491173\n'
    focus = root / 'focus.txt'
    focus.write_bytes(current)
    exact_page('focus.txt', current, 1, 2)
    before = focus.stat()
    current = b'CURRENT-BRAVO-593287\n'
    focus.write_bytes(current)
    os.utime(focus, ns=(before.st_atime_ns, before.st_mtime_ns))
    exact_page('focus.txt', current, 1, 2)
    replacement = root / 'replacement'
    current = b'CURRENT-DELTA-729391\n'
    replacement.write_bytes(current)
    os.replace(replacement, focus)
    exact_page('focus.txt', current, 1, 2)
    focus.unlink()
    invoke({'args': {'path': 'focus.txt'}}, expect_error=True)
    focus.write_bytes(current)
    exact_page('focus.txt', current, 1, 2)
    focus.chmod(0)
    invoke({'args': {'path': 'focus.txt'}}, expect_error=True)
    focus.chmod(0o600)
    exact_page('focus.txt', current, 1, 2)
    focus.unlink()
    focus.symlink_to('/etc/hostname')
    invoke({'args': {'path': 'focus.txt'}}, expect_error=True)
    focus.unlink()
    focus.write_bytes(current)
    exact_page('focus.txt', current, 1, 2)
    check('same-size restored-mtime/inode/delete/recreate/mode/symlink invalidation')
    invoke({'action': 'deny', 'value': True})
    invoke({'args': {'path': 'focus.txt'}}, expect_error=True)
    invoke({'action': 'deny', 'value': False})
    invoke({'action': 'protect', 'paths': ['focus.txt']})
    invoke({'args': {'path': 'focus.txt'}}, expect_error=True)
    invoke({'action': 'protect', 'paths': []})
    invoke({'action': 'cancel', 'value': True})
    invoke({'args': {'path': 'focus.txt'}}, expect_error=True)
    invoke({'action': 'cancel', 'value': False})
    exact_page('focus.txt', current, 1, 2)
    check('current authority/protected paths/cancellation override warm data')
    # More retained bytes than the cache holds; then reread the oldest entry.
    for i in range(7):
        line = f'EVICTION-{i}-'.encode() + b'z' * 1000 + b'\n'
        content = line * 10500
        (root / f'large-{i}.txt').write_bytes(content)
        exact_page(f'large-{i}.txt', content, 10480, 10481)
    content = (b'EVICTION-0-' + b'z' * 1000 + b'\n') * 10500
    exact_page('large-0.txt', content, 10480, 10481)
    content = b'oversize-line\n' * 1400000
    (root / 'oversize.txt').write_bytes(content)
    exact_page('oversize.txt', content, 1399997, 1399999)
    exact_page('oversize.txt', content, 1399997, 1399999)
    check('retention eviction and oversized-document streaming preserve content')
    (root / 'other.txt').write_text('OTHER-SETTLED-833921\n')
    observe([['workspace_read', {'path': 'focus.txt'}], ['workspace_search', {'path': 'other.txt', 'query': 'OTHER-SETTLED-833921'}]])
    first = assemble(state='CURRENT HOST INVOCATION STATE exact-A')
    assert 'CURRENT-DELTA-729391' in flat(first) and 'OTHER-SETTLED-833921' in flat(first)
    for _ in range(6):
        repeated = assemble(state='CURRENT HOST INVOCATION STATE exact-B')
        assert 'exact-B' in flat(repeated) and 'exact-A' not in flat(repeated)
    check('unchanged context reuses decoded messages and exact current host state')
    current = b'CURRENT-ECHOX-928317\n'
    before = focus.stat()
    focus.write_bytes(current)
    os.utime(focus, ns=(before.st_atime_ns, before.st_mtime_ns))
    changed = assemble()
    text = flat(changed)
    assert 'cached_observation_stale' in text and 'CURRENT-DELTA-729391' not in text
    assert 'OTHER-SETTLED-833921' in text
    observe([['workspace_read', {'path': 'focus.txt'}]])
    changed = assemble()
    assert 'CURRENT-ECHOX-928317' in flat(changed)
    check('warm context invalidates stale read while preserving unrelated settled batch')
    invoke({'action': 'deny', 'value': True})
    denied = assemble()
    assert 'CURRENT-ECHOX-928317' not in flat(denied)
    invoke({'action': 'deny', 'value': False})
    observe([['workspace_read', {'path': 'other.txt'}]], action='propose')
    pending = assemble()
    assert 'PENDING HOST OPERATIONS' in flat(pending)
    invoke({'action': 'settle'})
    settled = assemble()
    assert 'PENDING HOST OPERATIONS' not in flat(settled)
    calls = [b['id'] for m in settled for b in m['blocks'] if b['type'] == 'call']
    outcomes = [b['id'] for m in settled for b in m['blocks'] if b['type'] == 'result']
    assert len(calls) == len(set(calls)) and sorted(calls) == sorted(outcomes)
    check('warm context permissions and pending-to-settled exchange ordering')
    invoke({'action': 'policy', 'text': 'CURRENT-POLICY-REPLACED-55839'})
    switched = assemble(governing='CURRENT-USER-STEERING-76931')
    assert 'CURRENT-POLICY-REPLACED-55839' in flat(switched) and 'CURRENT-POLICY-INITIAL-7193' not in flat(switched)
    assert 'CURRENT-USER-STEERING-76931' in flat(switched)
    invoke({'action': 'begin', 'text': 'NEW-INVOCATION-REQUIREMENT-672939'})
    restarted = assemble()
    assert 'NEW-INVOCATION-REQUIREMENT-672939' in flat(restarted)
    assert not [b for m in restarted for b in m['blocks'] if b['type'] in ('call', 'result')]
    observe([['workspace_read', {'path': 'focus.txt'}]])
    assemble()
    check('host policy/governing steering/new invocation reset cache topology')
    efficiency = [json.loads(line) for line in (state / 'reuse.efficiency.jsonl').read_text().splitlines()]
    views = [v for v in efficiency if v.get('event') == 'context-view']
    assert len(views) >= 15
    for view in views[1:7]:
        assert view['decoded_message_cache_misses'] == 0 and view['newly_indexed_messages'] == 0, view
    assert [v['newly_indexed_messages'] for v in views if v['newly_indexed_messages']] == [6, 2, 1, 1, 3, 2], views
    result = {'seed': 731944, 'cases': results, 'actions': len(measurements), 'document_bytes': len(data), 'cold_read_rchar': cold, 'maximum_warm_read_rchar': max(warm), 'context_views': [{k:v[k] for k in ['decoded_message_cache_hits','decoded_message_cache_misses','newly_indexed_messages','assembly_elapsed_ms']} for v in views]}
    (base / 'result.json').write_text(json.dumps(result, indent=2))
    (base / 'measurements.json').write_text(json.dumps(measurements, indent=2))
    print(json.dumps(result, indent=2))
finally:
    child.stdin.close()
    child.wait(timeout=30)
    stderr.close()
    if child.returncode:
        raise RuntimeError((child.returncode, (base / 'stderr.log').read_text()))
