#!/usr/bin/env python3
"""Full production Git captures; independent source/index preservation and paired costs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import stat
import statistics
import subprocess
import threading
import time


def git(root, *args):
    return subprocess.check_output(['git', '-c', 'user.name=Peritus disposable replay',
                                    '-c', 'user.email=replay@invalid', *args], cwd=root,
                                   stderr=subprocess.PIPE)


def initialize(root):
    root.mkdir(parents=True, exist_ok=True)
    git(root, 'init', '-q')


def commit(root, message):
    git(root, 'add', '--all')
    git(root, 'commit', '-qm', message)


def source_snapshot(root):
    rows = {}
    for directory, subdirs, names in os.walk(root, followlinks=False):
        subdirs[:] = [name for name in subdirs if name != '.git']
        for name in subdirs + names:
            path = Path(directory) / name
            if name == '.git':
                continue
            info = path.lstat()
            kind = stat.S_IFMT(info.st_mode)
            body = (os.readlink(path).encode() if path.is_symlink()
                    else path.read_bytes() if stat.S_ISREG(info.st_mode) else b'')
            rows[str(path.relative_to(root))] = [kind, stat.S_IMODE(info.st_mode),
                                                hashlib.sha256(body).hexdigest()]
    rows['.'] = stat.S_IMODE(root.stat().st_mode)
    return rows


def index_snapshot(root):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in root.rglob('index') if '.git' in path.parts and path.is_file()}


class Owner:
    def __init__(self, consumer, root):
        self.trace = root / 'git-trace.jsonl'
        self.stream = (root / 'consumer.stderr').open('w')
        env = dict(os.environ, GIT_TRACE2_EVENT=str(self.trace),
                   GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_SYSTEM='/dev/null')
        self.process = subprocess.Popen([str(consumer.resolve())], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.stream, text=True, env=env)
        self.history = (root / 'requests.jsonl').open('w')

    def call(self, workspace, mode):
        start = self.trace.stat().st_size if self.trace.exists() else 0
        request = {'root': str(workspace), 'mode': mode}
        self.process.stdin.write(json.dumps(request) + '\n')
        self.process.stdin.flush()
        response = json.loads(self.process.stdout.readline())
        with self.trace.open('rb') as stream:
            stream.seek(start)
            events = [json.loads(line) for line in stream]
        captures = sum(row.get('event') == 'start' and row.get('argv', [])[1:]
                       == ['ls-files', '-z', '--cached'] for row in events)
        response['git_complete_source_enumerations'] = captures
        self.history.write(json.dumps({'request': request, 'response': response}) + '\n')
        self.history.flush()
        return response

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=10) == 0
        self.history.close()
        self.stream.close()


def identity(row):
    return {key: row[key] for key in ['repository', 'source']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=8)
    args = parser.parse_args()
    assert args.rounds >= 2
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    workspace = root / 'workspace'
    initialize(workspace)
    alpha = workspace / 'alpha.txt'
    alpha.write_text('INDEPENDENT-ALPHA-1111\n')
    (workspace / 'beta.txt').write_text('INDEPENDENT-BETA-2222\n')
    commit(workspace, 'private baseline')
    owner = Owner(args.consumer, root)
    cases = []

    def pair(name, source_changed=None, repository_changed=None):
        before = source_snapshot(workspace)
        indexes = index_snapshot(workspace)
        results = {mode: owner.call(workspace, mode)
                   for mode in (['legacy', 'shared'] if len(cases) % 2 == 0
                                else ['shared', 'legacy'])}
        assert all('error' not in row for row in results.values()), (name, results)
        assert identity(results['legacy']) == identity(results['shared']), (name, results)
        assert source_snapshot(workspace) == before, (name, 'source mutation by observation')
        assert index_snapshot(workspace) == indexes, (name, 'real index mutation by observation')
        current = identity(results['shared'])
        if cases:
            old = cases[-1]['identity']
            for key, changed in [('source', source_changed), ('repository', repository_changed)]:
                if changed is not None:
                    assert (current[key] != old[key]) == changed, (name, key, changed)
        assert results['legacy']['git_complete_source_enumerations'] * 2 == \
               results['shared']['git_complete_source_enumerations'] * 3
        cases.append({'name': name, 'identity': current, 'results': results,
                      'independent_source_and_index_unchanged': True})

    try:
        pair('baseline')
        original = alpha.stat()
        alpha.write_text('INDEPENDENT-ALPHA-3333\n')
        os.utime(alpha, ns=(original.st_atime_ns, original.st_mtime_ns))
        pair('same-size restored-mtime bytes', True, True)
        alpha.chmod(0o640)
        pair('non-executable mode change', True, True)
        workspace.chmod(0o750)
        pair('root permission change', True)
        (workspace / 'new.txt').write_text('INDEPENDENT-UNTRACKED\n')
        pair('new untracked source', True, True)
        (workspace / 'beta.txt').unlink()
        pair('tracked deletion', True, True)
        (workspace / 'new.txt').unlink()
        (workspace / 'new.txt').symlink_to('alpha.txt')
        pair('regular file becomes symlink', True, True)
        (workspace / 'new.txt').unlink()
        (workspace / 'new.txt').symlink_to('missing.txt')
        pair('symlink target changes', True, True)
        git(workspace, 'add', 'alpha.txt')
        pair('staged index remains exact')
        git(workspace, 'commit', '--allow-empty', '-qm', 'private head change')
        pair('committed head with identical current source', False, True)
        nested = workspace / 'nested'
        initialize(nested)
        (nested / 'source.txt').write_text('INDEPENDENT-NESTED-5555\n')
        commit(nested, 'private nested baseline')
        pair('nested committed repository', True, True)
        original = (nested / 'source.txt').stat()
        (nested / 'source.txt').write_text('INDEPENDENT-NESTED-7777\n')
        os.utime(nested / 'source.txt', ns=(original.st_atime_ns, original.st_mtime_ns))
        pair('nested same-size restored-mtime bytes', True, True)
        (nested / 'source.txt').chmod(0o640)
        pair('nested permission change', True, True)
        commit(nested, 'private nested content')
        pair('nested head with identical current source', False, True)
        unborn = workspace / 'unborn'
        initialize(unborn)
        (unborn / 'uncommitted.txt').write_text('INDEPENDENT-UNBORN\n')
        pair('nested unborn repository', True, True)
        (workspace / 'rust-toolchain.toml').write_text('[toolchain]\nchannel = "1.97.1"\n')
        pair('toolchain configuration source', True, True)
        (workspace / '.gitignore').write_text('ignored-private.txt\n')
        (workspace / 'ignored-private.txt').write_text('EXCLUDED-BY-EXISTING-MANAGED-SCOPE\n')
        pair('ignore configuration source', True, True)
        (workspace / 'ignored-private.txt').write_text('AMENDED-OUTSIDE-EXISTING-MANAGED-SCOPE\n')
        pair('ignored source keeps existing scope', False, False)
        fifo = workspace / 'unsupported.fifo'
        os.mkfifo(fifo)
        # Git omits untracked special nodes. Enroll the exact path in its real index so
        # the existing managed source contract actually observes and rejects this kind.
        blob = git(workspace, 'rev-parse', 'HEAD:alpha.txt').decode().strip()
        git(workspace, 'update-index', '--add', '--cacheinfo', '100644', blob, fifo.name)
        rejected = {mode: owner.call(workspace, mode) for mode in ['legacy', 'shared']}
        assert all('error' in row for row in rejected.values()), rejected
        assert rejected['legacy']['error'] == rejected['shared']['error']
        fifo.unlink()
        (root / 'rejected.json').write_text(json.dumps(rejected, indent=2))

        racing = root / 'racing'
        initialize(racing)
        target = racing / 'value.txt'
        alternatives = [b'INDEPENDENT-RACE-A\n' * 16384, b'INDEPENDENT-RACE-B\n' * 16384]
        target.write_bytes(alternatives[0])
        commit(racing, 'private race baseline')
        allowed = []
        for body in alternatives:
            target.write_bytes(body)
            value = owner.call(racing, 'shared')
            assert 'error' not in value
            allowed.append(identity(value))
        assert allowed[0] != allowed[1]
        original_index = index_snapshot(racing)
        staging = root / 'race-staging'
        staging.mkdir()
        stopped = threading.Event()
        writes = []

        def publish():
            number = 0
            while not stopped.is_set():
                temporary = staging / 'next.txt'
                temporary.write_bytes(alternatives[number % 2])
                os.replace(temporary, target)
                writes.append(number)
                number += 1
                stopped.wait(0.003)

        publisher = threading.Thread(target=publish, daemon=True)
        publisher.start()
        raced = []
        try:
            for _ in range(24):
                value = owner.call(racing, 'shared')
                if 'error' not in value:
                    assert identity(value) in allowed, ('mixed/invented identity under actual writer', value)
                else:
                    assert 'workspace changed repeatedly' in value['error'], value
                raced.append(value)
        finally:
            stopped.set()
            publisher.join(timeout=10)
        assert not publisher.is_alive()
        assert len(writes) > 10 and sum('error' not in row for row in raced) >= 3
        assert index_snapshot(racing) == original_index
        assert target.read_bytes() in alternatives
        assert sorted(path.name for path in racing.iterdir()) == ['.git', 'value.txt']
        (root / 'racing.json').write_text(json.dumps({'observations': raced,
            'independent_known_identities': allowed, 'actual_atomic_publications': len(writes)}, indent=2))

        # Linked worktrees use a .git file and a separate real index. Both policies must keep
        # the existing owner/path semantics rather than assuming a root .git directory.
        linked = root / 'linked'
        git(racing, 'worktree', 'add', '--detach', str(linked), 'HEAD')
        linked_index = Path(git(linked, 'rev-parse', '--path-format=absolute', '--git-path', 'index').decode().strip())
        saved_index = linked_index.read_bytes()
        linked_source = source_snapshot(linked)
        linked_rows = {mode: owner.call(linked, mode) for mode in ['legacy', 'shared']}
        assert all('error' not in row for row in linked_rows.values()), linked_rows
        assert identity(linked_rows['legacy']) == identity(linked_rows['shared'])
        assert linked_source == source_snapshot(linked) and linked_index.read_bytes() == saved_index
        (root / 'linked.json').write_text(json.dumps(linked_rows, indent=2))

        # Refuse actual Git object publication, then restore the owned directory and retry.
        # This uses a new blob, so an already-present object cannot hide the write failure.
        refused = root / 'object-publication-refusal'
        initialize(refused)
        (refused / 'value.txt').write_text('INITIAL-OBJECT-716248\n')
        commit(refused, 'private object baseline')
        (refused / 'value.txt').write_text('NEW-UNPUBLISHED-OBJECT-932176 Ω\n')
        objects = refused / '.git/objects'
        old_modes = {p: stat.S_IMODE(p.stat().st_mode) for p in [objects, *objects.rglob('*')]
                     if p.is_dir()}
        for directory in old_modes:
            directory.chmod(0o555)
        try:
            failures = {mode: owner.call(refused, mode) for mode in ['legacy', 'shared']}
            assert all('error' in row for row in failures.values()), failures
        finally:
            for directory, mode in old_modes.items():
                directory.chmod(mode)
        recovered = {mode: owner.call(refused, mode) for mode in ['legacy', 'shared']}
        assert all('error' not in row for row in recovered.values()), recovered
        assert identity(recovered['legacy']) == identity(recovered['shared'])
        (root / 'object-publication.json').write_text(json.dumps(dict(failed=failures, recovered=recovered), indent=2))

        large = root / 'large'
        initialize(large)
        rng = random.Random(192805)
        for number in range(256):
            (large / f'source-{number:04}.bin').write_bytes(rng.randbytes(65536))
        commit(large, 'private large baseline')
        expected = source_snapshot(large)
        indexes = index_snapshot(large)
        # Warm both real paths before timed repeats; no physical cold-cache claim is made.
        for mode in ['legacy', 'shared']:
            assert 'error' not in owner.call(large, mode)
        costs = []
        for number in range(args.rounds):
            rows = []
            for mode in (['legacy', 'shared'] if number % 2 == 0 else ['shared', 'legacy']):
                row = owner.call(large, mode)
                assert 'error' not in row
                assert row['git_complete_source_enumerations'] == (3 if mode == 'legacy' else 2)
                rows.append(dict(row, round=number, mode=mode))
            assert identity(rows[0]) == identity(rows[1])
            costs.extend(rows)
        assert source_snapshot(large) == expected
        assert index_snapshot(large) == indexes
        summary = {'passed': True, 'cases': cases, 'nonfile_rejection': rejected,
                   'actual_writer_observations': len(raced),
                   'linked_worktree': True, 'object_publication_refusal_and_recovery': True,
                   'actual_writer_accepted_exact': sum('error' not in row for row in raced),
                   'cost_rows': costs, 'performance': {},
                   'consumer_sha256': hashlib.sha256(args.consumer.read_bytes()).hexdigest(),
                   'state': 'warm local Git objects and OS caches; actual Git trace enabled symmetrically',
                   'qualification_scope': 'public runtime full captures on Linux; product settlement/control and other platforms remain unqualified'}
        for mode in ['legacy', 'shared']:
            values = [row['elapsed_seconds'] for row in costs if row['mode'] == mode]
            summary['performance'][mode] = {'samples': len(values),
                                           'mean_seconds': statistics.mean(values),
                                           'median_seconds': statistics.median(values)}
        (root / 'result.json').write_text(json.dumps(summary, indent=2))
        print(json.dumps({'passed': True, 'equivalence_groups': len(cases),
                          'performance': summary['performance']}, indent=2))
    finally:
        owner.close()


if __name__ == '__main__':
    main()
