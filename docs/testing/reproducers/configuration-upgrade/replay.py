#!/usr/bin/env python3
"""Replay generated-config upgrades through real public bootstrap and packaged CLI paths."""
import argparse
import fcntl
import hashlib
import itertools
import json
import os
from pathlib import Path
import pty
import random
import re
import select
import struct
import stat
import subprocess
import tempfile
import termios
import time
import tomllib

p = argparse.ArgumentParser()
p.add_argument('--consumer', type=Path, required=True)
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--baseline', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--seed', type=int, default=271821)
a = p.parse_args()
for name in ('consumer', 'binary', 'baseline', 'output'):
    setattr(a, name, getattr(a, name).resolve())
a.output.mkdir(parents=True, exist_ok=True)
results = []
hosted = ['open-code-zen', 'open-code-go', 'open-router', 'groq', 'together-ai', 'fireworks-ai', 'deep-seek']
direct = ['open-ai-api', 'anthropic-api', 'google-gemini-api', 'compatible-endpoint']
accounts = ['codex-account', 'claude-account']
rng = random.Random(a.seed)


def run(command, env):
    return subprocess.run([str(value) for value in command], env=env, stdin=subprocess.DEVNULL,
                          capture_output=True, text=True, timeout=30)


def prepare(env):
    result = run([a.consumer, 'prepare'], env)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def snapshot(paths):
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}


def baseline_launch(env):
    master, slave = pty.openpty()
    process = subprocess.Popen([str(a.baseline)], env=env, stdin=slave, stdout=slave, stderr=slave)
    os.close(slave)
    raw = bytearray()
    try:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if select.select([master], [], [], .1)[0]:
                try:
                    raw.extend(os.read(master, 65536))
                except OSError:
                    break
            elif process.poll() is not None:
                break
        process.wait(timeout=5)
        assert process.returncode != 0 and b'configuration generation 21 has different content' in raw, raw
        (a.output / 'original-v0.1.9.stderr').write_bytes(raw)
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        os.close(master)


def legacy(text, discovery=False, parallel=False, timeout=False, folders=False):
    # Concrete historical input formats. The checker below never uses this transformation
    # to decide whether the resulting current file or state is correct.
    if discovery:
        text = text.replace('discover_model_limits = true\n', '')
    if parallel:
        sections = text.split('\n[[providers]]\n')
        for index, section in enumerate(sections):
            if 'hosted_service = "opencode-zen"' in section or 'hosted_service = "opencode-go"' in section:
                sections[index] = section.replace(
                    'capabilities = ["parallel-tool-calls", "streaming", "tool-calls", "usage-detail"]',
                    'capabilities = ["streaming", "tool-calls", "usage-detail"]', 1)
        text = '\n[[providers]]\n'.join(sections)
    if timeout:
        text = text.replace('\n\n[telemetry]\n', '\nprovider_turn_timeout_seconds = 600\n\n[telemetry]\n')
    if folders:
        text = re.sub(r'(?m)^allow = .*$', 'allow = []', text)
        text = '\n[[folders]]\n'.join(text.split('\n[[folders]]\n')[:2])
    return text


def fixture(base, index, providers):
    root = base / str(index)
    env = os.environ.copy()
    env.update({f'XDG_{key}_HOME': str(root / name) for key, name in
                [('CONFIG', 'c'), ('STATE', 's'), ('CACHE', 'k')]})
    env['TERM'] = 'xterm-256color'
    env['PERITUS_INSTALL_DEPS'] = '0'
    result = run([a.consumer, 'seed', json.dumps(providers)], env)
    assert result.returncode == 0, result.stderr
    seeded = json.loads(result.stdout)
    state = seeded['state']
    state['generation'] = 21
    config_root = root / 'c/peritus'
    state_root = root / 's/peritus/product-state'
    config = config_root / 'peritus-00000000000000000021.toml'
    current = Path(seeded['configuration']).read_text()
    parsed = tomllib.loads(current)
    assert all(route.get('discover_model_limits') is True for route in parsed.get('providers', [])
               if route['kind'] == 'hosted')
    assert len(parsed['folders']) == 2
    config.write_text(current)
    config.chmod(0o600)
    state_file = state_root / 'state-00000000000000000021.json'
    state_file.write_text(json.dumps(state) + '\n')
    state_file.chmod(0o600)
    (config_root / 'update-checks').write_text('disabled\n')
    sentinel = root / 's/peritus/unrelated-retained-data.bin'
    sentinel.write_bytes(b'Untouched retained bytes: \x00\xff\x17\x03')
    return root, env, config, current, state, sentinel


def check_current(result, expected, state, changed):
    assert result['state']['generation'] == 21 + int(changed), result
    after = dict(result['state'])
    after['generation'] = 21
    assert after == state, 'Migration changed durable choices'
    path = Path(result['configuration'])
    assert path.read_text() == expected, 'Published bytes are not the current generated format'
    assert stat.S_IMODE(path.stat().st_mode) == 0o600, 'Generated configuration is not private'
    return path


def terminal(env, workspace, label):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 42, 160, 0, 0))
    process = subprocess.Popen([str(a.binary), 'open', str(workspace)], env=env, cwd=workspace,
                               stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
    os.close(slave)
    raw = bytearray()
    try:
        deadline = time.monotonic() + 60
        while process.poll() is None and time.monotonic() < deadline:
            if select.select([master], [], [], .1)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    break
                raw.extend(chunk)
                if b'\x1b[6n' in chunk:
                    os.write(master, b'\x1b[1;1R')
                if b'\x1b[?u' in chunk or b'\x1b[c' in chunk:
                    os.write(master, b'\x1b[?1;2c')
            if b'Enter sends' in raw:
                os.write(master, b'\x11')
                process.wait(timeout=10)
                assert process.returncode == 0
                return
        raise AssertionError(raw[-5000:].decode(errors='replace'))
    finally:
        (a.output / (label + '.ansi')).write_bytes(raw)
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        os.close(master)
        stopped = run([a.binary, 'shutdown', '--wait'], env)
        assert stopped.returncode == 0, stopped.stderr


cases = [('current-all-hosted', hosted, {})]
cases += [(kind, [kind], dict(discovery=True)) for kind in hosted]
cases += [(f'{kind}-caps-{discovery}-{timeout}', [kind],
           dict(discovery=discovery, parallel=True, timeout=timeout))
          for kind, discovery, timeout in itertools.product(hosted[:2], (False, True), (False, True))]
cases += [('mixed-skipped-upgrade', hosted + direct + accounts,
           dict(discovery=True, parallel=True, timeout=True, folders=True)),
          ('current-nonhosted', direct + accounts, {}),
          ('nonhosted-timeout-and-folders', direct + accounts, dict(timeout=True, folders=True)),
          ('current-offline', [], {}), ('offline-timeout', [], dict(timeout=True))]
for index in range(12):
    providers = rng.sample(hosted + direct, rng.randrange(1, len(hosted + direct) + 1))
    cases.append((f'generated-{index}', providers,
                  dict(discovery=True, parallel=bool(rng.getrandbits(1)),
                       timeout=bool(rng.getrandbits(1)), folders=bool(rng.getrandbits(1)))))

with tempfile.TemporaryDirectory(prefix='pcu-') as directory:
    base = Path(directory)
    for index, (label, providers, options) in enumerate(cases):
        root, env, config, expected, state, sentinel = fixture(base, index, providers)
        old = legacy(expected, **options)
        config.write_text(old)
        before = snapshot([*config.parent.glob('*.toml'),
                           *(root / 's/peritus/product-state').glob('*.json'), sentinel])
        if label == 'open-code-zen':
            baseline_launch(env)
        try:
            result = prepare(env)
        except AssertionError as error:
            raise AssertionError(label, providers, options, str(error)) from error
        check_current(result, expected, state, old != expected)
        assert snapshot(map(Path, before)) == before, 'Old immutable or unrelated files changed'
        assert prepare(env) == result, 'Fresh repeated launch advanced the generation again'
        results.append(dict(label=label, providers=providers, historical_format=options,
                            generation=result['state']['generation'], passed=True))

    # Unrecognized edits must fail without publishing a new state or overwriting evidence.
    edits = {
        'explicit-false': lambda text: text.replace('discover_model_limits = true', 'discover_model_limits = false'),
        'model': lambda text: text.replace('fixture-route-model', 'changed-model'),
        'credential': lambda text: text.replace(':00000000000000000000000000000001:', ':0000000000000000000000000000000f:'),
        'folder-authority': lambda text: text.replace('writable = false', 'writable = true'),
        'unknown-field': lambda text: 'unknown_field = true\n' + text,
    }
    for index, (label, edit) in enumerate(edits.items(), len(cases)):
        root, env, config, expected, state, sentinel = fixture(base, index, hosted)
        changed = edit(expected)
        assert changed != expected
        config.write_text(changed)
        before = snapshot([*config.parent.glob('*.toml'), *(root / 's/peritus/product-state').glob('*.json'), sentinel])
        result = run([a.consumer, 'prepare'], env)
        assert result.returncode != 0 and 'different content' in result.stderr, result
        assert snapshot(map(Path, before)) == before
        assert len(list((root / 's/peritus/product-state').glob('*.json'))) == len([path for path in before if path.endswith('.json')])
        config.write_text(expected)
        check_current(prepare(env), expected, state, False)
        results.append(dict(label='reject-' + label, passed=True))

    root, env, config, expected, state, sentinel = fixture(base, 'interrupted', hosted)
    config.write_text(legacy(expected, discovery=True, parallel=True, timeout=True))
    original_hash = snapshot([config, sentinel])
    blocked = config.with_name('peritus-00000000000000000022.toml')
    blocked.mkdir()
    result = run([a.consumer, 'prepare'], env)
    assert result.returncode != 0
    assert (root / 's/peritus/product-state/state-00000000000000000022.json').exists()
    blocked.rmdir()
    check_current(prepare(env), expected, state, True)
    assert snapshot(map(Path, original_hash)) == original_hash
    results.append(dict(label='interrupted-publication-retry', passed=True))

    root, env, config, expected, state, sentinel = fixture(base, 'backed-up', hosted)
    backup = config.with_suffix('.toml.bak')
    config.rename(backup)
    before = snapshot([backup, sentinel])
    check_current(prepare(env), expected, state, False)
    assert snapshot(map(Path, before)) == before
    results.append(dict(label='backed-up-configuration-regeneration', passed=True))

    root, env, config, expected, state, sentinel = fixture(base, 'direct-capabilities', direct)
    altered = expected.replace('"parallel-tool-calls", ', '', 1)
    assert altered != expected
    config.write_text(altered)
    result = run([a.consumer, 'prepare'], env)
    assert result.returncode != 0 and 'different content' in result.stderr, result
    assert config.read_text() == altered
    assert not (root / 's/peritus/product-state/state-00000000000000000022.json').exists()
    results.append(dict(label='reject-unrelated-direct-capabilities', passed=True))

    root, env, config, expected, state, sentinel = fixture(base, 'concurrent', hosted)
    config.write_text(legacy(expected, discovery=True))
    workers = [subprocess.Popen([str(a.consumer), 'prepare'], env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, text=True) for _ in range(4)]
    outcomes = []
    for worker in workers:
        stdout, stderr = worker.communicate(timeout=30)
        if worker.returncode == 0:
            check_current(json.loads(stdout), expected, state, True)
            outcomes.append('prepared')
        else:
            assert 'BootstrapBusy' in stderr, stderr
            outcomes.append('bootstrap-busy')
    check_current(prepare(env), expected, state, True)
    results.append(dict(label='concurrent-upgrade', outcomes=outcomes, passed=True))

    root, env, config, expected, state, sentinel = fixture(base, 'terminal', [])
    config.write_text(legacy(expected, timeout=True))
    old_hash = snapshot([config, sentinel])
    workspace = Path(state['workspaces']['recent'][0]['repository_root'])
    terminal(env, workspace, 'terminal-upgrade')
    check_current(prepare(env), expected, state, True)
    terminal(env, workspace, 'terminal-reopen')
    check_current(prepare(env), expected, state, True)
    assert snapshot(map(Path, old_hash)) == old_hash
    results.append(dict(label='actual-terminal-upgrade-and-reopen', passed=True))

report = dict(passed=True, seed=a.seed, cases=len(results), results=results,
              artifacts={name: hashlib.sha256(getattr(a, name).read_bytes()).hexdigest()
                         for name in ('consumer', 'binary', 'baseline')})
(a.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(dict(passed=True, seed=a.seed, cases=len(results))))
