#!/usr/bin/env python3
"""Operate actual CLI/launcher/daemon processes with isolated per-user roots (Linux)."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--baseline', type=Path)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
binary = a.binary.resolve()
output = a.output.resolve()
output.mkdir(parents=True, exist_ok=True)
results = []
instances = []


def record(label, command, result):
    results.append(dict(case=label, command=command, exit=result.returncode,
                        stdout=result.stdout.strip(), stderr=result.stderr.strip()))
    (output / 'results.json').write_text(json.dumps(results, indent=2))


def run(label, env, args, expected=0, executable=binary):
    command = [str(executable), *args]
    started = time.monotonic()
    result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=40)
    record(label, command, result)
    results[-1]['seconds'] = time.monotonic() - started
    assert result.returncode == expected, results[-1]
    return result


def environment(root):
    env = os.environ.copy()
    env.update({f'XDG_{kind}_HOME': str(root / folder) for kind, folder in (
        ('CONFIG', 'config'), ('STATE', 'state'), ('CACHE', 'cache'))})
    env['TERM'] = 'xterm-256color'
    return env


def alive(pid):
    path = Path(f'/proc/{pid}/stat')
    return path.exists() and path.read_text().rsplit(')', 1)[1].split()[0] != 'Z'


def endpoint(pid):
    for line in subprocess.check_output(['ss', '-xlpH'], text=True).splitlines():
        if f'pid={pid},' in line:
            return line.split()[4]
    raise AssertionError(f'No real listening socket for {pid}')


def start(root):
    root.mkdir(parents=True)
    workspace = root / 'workspace'
    workspace.mkdir()
    (workspace / 'notes.md').write_text('Retain these original bytes: café 🦀.\n')
    env = environment(root)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 42, 160, 0, 0))
    process = subprocess.Popen([str(binary), 'open', str(workspace)], cwd=workspace,
                               env=env, stdin=slave, stdout=slave, stderr=slave,
                               start_new_session=True)
    os.close(slave)
    raw = bytearray()
    answered = set()
    try:
        deadline = time.monotonic() + 75
        while process.poll() is None and time.monotonic() < deadline:
            if select.select([master], [], [], .1)[0]:
                data = os.read(master, 65536)
                raw.extend(data)
                if b'\x1b[6n' in data:
                    os.write(master, b'\x1b[1;1R')
                if b'\x1b[?u' in data or b'\x1b[c' in data:
                    os.write(master, b'\x1b[?1;2c')
            for prompt, reply in [(b'Trust this workspace?', b'y\r'),
                                  (b'Providers (comma-separated', b'0\r')]:
                if prompt in raw and prompt not in answered:
                    answered.add(prompt)
                    os.write(master, reply)
            if b'Enter sends' in raw:
                break
        else:
            raise AssertionError(raw[-6000:].decode(errors='replace'))
        os.write(master, b'\x11')
        process.wait(timeout=10)
        assert process.returncode == 0
    finally:
        (output / f'launch-{len(instances)}.ansi').write_bytes(raw)
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=10)
        os.close(master)
    state = root / 'state/peritus/daemon'
    info = dict(line.split('=', 1) for line in (state / 'daemon.instance').read_text().splitlines())
    pid = int(info['pid'])
    item = dict(root=root, env=env, state=state, pid=pid, endpoint=endpoint(pid))
    instances.append(item)
    run('ready', env, ['--endpoint', item['endpoint'], 'status'])
    return item


def stopped(item):
    assert not alive(item['pid']), item
    with (item['state'] / 'daemon.lock').open('r+') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    assert (item['root'] / 'workspace/notes.md').read_text() == 'Retain these original bytes: café 🦀.\n'


with tempfile.TemporaryDirectory(prefix='peritus-shutdown-') as directory:
    base = Path(directory)
    try:
        empty = base / 'never-installed'
        env = environment(empty)
        if a.baseline:
            run('original-reported-failure', env, ['shutdown'], expected=2,
                executable=a.baseline.resolve())
        run('never-installed', env, ['shutdown'])
        assert not empty.exists(), 'Shutdown must not initialize an installation'
        orphan = base / 'held-lock'
        state = orphan / 'state/peritus/daemon'
        state.mkdir(parents=True)
        with (state / 'daemon.lock').open('w+') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = run('held-lock-not-stopped', environment(orphan),
                         ['--timeout-seconds', '1', 'shutdown'], expected=10)
            assert 'not completed' in result.stderr and not result.stdout
            (state / 'daemon.instance').write_text('version=1\nendpoint=../../another-user\n')
            result = run('reject-arbitrary-endpoint', environment(orphan),
                         ['shutdown'], expected=70)
            assert 'invalid live daemon endpoint' in result.stderr
        roots = ['plain', 'space café 🦀', 'long-' + 'x' * 75]
        for i, name in enumerate(roots):
            item = start(base / name)
            if i == 2:
                alias = base / ('alias-' + 'y' * 75)
                alias.symlink_to(item['root'] / 'state', target_is_directory=True)
                item['env']['XDG_STATE_HOME'] = str(alias)
            # Unrelated broken settings must never prevent stopping the running daemon.
            if i == 1:
                for path in (item['root'] / 'config/peritus').glob('peritus-*.toml'):
                    path.write_text('deliberately invalid configuration')
                for path in (item['root'] / 'state/peritus/product-state').glob('state-*.json'):
                    path.write_text('deliberately invalid state')
            args = ['shutdown'] if i == 0 else ['--json', 'shutdown', '--wait']
            run(f'implicit-stop-{i}', item['env'], args)
            stopped(item)
            result = run(f'repeat-stop-{i}', item['env'], ['--json', 'shutdown'])
            assert json.loads(result.stdout)['result']['disposition'] == 'already-stopped'
        first = start(base / 'first')
        second = start(base / 'second')
        # An unknown requested session is closed by the real server during negotiation.
        # It must remain an error and cannot be mistaken for a concurrent completed stop.
        result = run('invalid-session-does-not-stop', first['env'],
                     ['--timeout-seconds', '1', '--session',
                      'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'shutdown'], expected=10)
        assert alive(first['pid'])
        run('explicit-overrides-local-root', first['env'],
            ['--endpoint', second['endpoint'], 'shutdown', '--wait'])
        deadline = time.monotonic() + 10
        while alive(second['pid']) and time.monotonic() < deadline:
            time.sleep(.05)
        stopped(second)
        assert alive(first['pid']), 'Explicit endpoint must not shut down the discovered daemon'
        run('unaffected-neighbor', first['env'], ['--endpoint', first['endpoint'], 'status'])
        commands = [[str(binary), 'shutdown'], [str(binary), '--json', 'shutdown']]
        started = time.monotonic()
        workers = [subprocess.Popen(command, env=first['env'], stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True) for command in commands]
        for i, worker in enumerate(workers):
            stdout, stderr = worker.communicate(timeout=40)
            result = subprocess.CompletedProcess(commands[i], worker.returncode, stdout, stderr)
            record(f'concurrent-stop-{i}', commands[i], result)
            assert worker.returncode == 0, results[-1]
        stopped(first)
        assert time.monotonic() - started < 10, 'Idle concurrent shutdown must not wait for a request timeout'
        for repeat in range(3):
            item = start(base / f'concurrent-{repeat}')
            started = time.monotonic()
            workers = [subprocess.Popen(command, env=item['env'], stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True) for command in commands]
            for i, worker in enumerate(workers):
                stdout, stderr = worker.communicate(timeout=40)
                result = subprocess.CompletedProcess(commands[i], worker.returncode, stdout, stderr)
                record(f'concurrent-repeat-{repeat}-{i}', commands[i], result)
                assert worker.returncode == 0, results[-1]
                if '--json' in commands[i]:
                    assert json.loads(stdout.splitlines()[-1])['ok']
            stopped(item)
            assert time.monotonic() - started < 10, 'Idle concurrent shutdown must not wait for a request timeout'
        print(json.dumps(dict(passed=True, cases=len(results), binary_sha256=
                             hashlib.sha256(binary.read_bytes()).hexdigest())))
    finally:
        for item in instances:
            if alive(item['pid']):
                subprocess.run([str(binary), '--endpoint', item['endpoint'], 'shutdown', '--wait'],
                               env=item['env'], capture_output=True, timeout=40)
