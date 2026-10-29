#!/usr/bin/env python3
"""Real durable context operations: originating requests, host freshness and reader floors."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import shutil
import sqlite3
import subprocess

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('pages', HERE / 'checkpoint-pages.py')
pages = importlib.util.module_from_spec(_spec); _spec.loader.exec_module(pages)


def text(view):
    return '\n'.join(block.get('text', block.get('output', ''))
                     for message in view['messages'] for block in message['blocks'])


def verified_published_roots(case, **_):
    # Enumerate committed history, rather than finalized objects: an interrupted/failed
    # publication legitimately leaves an unreferenced complete manifest on disk.
    lineage = case / 'run.context/writer'
    with sqlite3.connect(f'file:{lineage / "journal.sqlite3"}?mode=ro', uri=True) as db:
        history = db.execute('SELECT value_digest,producing_position FROM state_record_history WHERE namespace=3401').fetchall()
        for digest, position in history:
            expected = {digest}
            def load(reference):
                key = bytes(reference['digest']); expected.add(key)
                body = pages.page.object_path(lineage, key).read_bytes()
                assert len(body) == reference['bytes'] and hashlib.sha256(body).digest() == key
                return body
            body = pages.page.object_path(lineage, digest).read_bytes()
            assert hashlib.sha256(body).digest() == digest
            manifest = json.loads(body)
            for name in ['working_state', 'source_index', 'transcript_manifest', 'view', 'validation']:
                raw = load(manifest[name])
                if name in ['working_state', 'source_index']:
                    for page in json.loads(raw)['pages']: load(page)
            roots = {row[0] for row in db.execute('SELECT r.artifact_digest FROM artifact_references r '
                'JOIN commands c ON c.batch_hash=r.owner_identity WHERE r.owner_kind=1 '
                'AND c.first_position<=? AND c.last_position>=?', (position, position))}
            assert roots == expected, (case, manifest['generation'], 'incomplete published roots')
    return len(history)


class Client:
    def __init__(self, binary, path, directory):
        self.errors = (directory / 'stderr.log').open('a')
        self.log = (directory / 'interface.jsonl').open('a')
        self.process = subprocess.Popen([str(binary), str(path)], text=True, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.errors)

    def send(self, command, ok=True):
        self.process.stdin.write(json.dumps(command) + '\n'); self.process.stdin.flush()
        assert select.select([self.process.stdout], [], [], 90)[0], command
        line = self.process.stdout.readline()
        assert line, (self.process.poll(), command)
        response = json.loads(line)
        self.log.write(json.dumps(dict(command=command, response=response)) + '\n'); self.log.flush()
        assert response['answer']['ok'] == ok, response
        return response['answer'].get('value', response['answer'])

    def close(self):
        self.process.stdin.close(); self.process.wait(timeout=20)
        self.errors.close(); self.log.close()
        assert self.process.returncode == 0

    def entries(self):
        out = self.send(dict(action='plan', tool='context_read', args=dict(
            observation_ids=[], query=None, cursor=None, offset=0, max_bytes=16384)))['outputs'][0]
        return {entry['text']: entry for entry in out['entries']}

    def assemble(self, **extra):
        return self.send(dict(action='assemble', **extra))

    def update(self, view, marker, handle, **extra):
        operation = dict(id=marker, kind='plan', text=marker, supports=[handle], contradicts=[],
                         depends_on=[], status='open', validity='files', files=['alpha.txt'], supersedes=None)
        operation.update(extra)
        base = int(re.search(r'context_update base_revision=(\d+)', text(view)).group(1))
        return self.send(dict(action='plan', tool='context_update', args=dict(
            base_revision=base, operations=[operation])))['outputs'][0]


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--consumer', type=Path, required=True)
    p.add_argument('--old-consumer', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--expect-stale-binding-bug', action='store_true')
    args = p.parse_args()
    binary = args.consumer.resolve(); root = args.output.resolve(); root.mkdir(parents=True)
    results = []
    def save(label, **more):
        results.append(dict(case=label, passed=True, **more))
        (root / 'progress.json').write_text(json.dumps(results, indent=2))
        print(json.dumps(results[-1]), flush=True)
    def git(workspace, *arguments):
        return subprocess.check_output(['git', '-c', 'user.name=Peritus owned fixture', '-c',
            'user.email=replay@invalid', '-c', 'commit.gpgsign=false', *arguments], cwd=workspace, stderr=subprocess.PIPE)
    def prepare(label, managed=True, policy='conservative'):
        case = root / label; case.mkdir()
        workspace = case / 'workspace'; workspace.mkdir()
        for name, body in [('alpha', 'EXACT-ALPHA-HOST-716283 Ω\n'), ('beta', 'EXACT-BETA-OLD-927451 Ω\n')]:
            (workspace / f'{name}.txt').write_text(body)
        if managed:
            git(workspace, 'init', '-q'); git(workspace, 'add', '--all'); git(workspace, 'commit', '-qm', 'owned source')
        spec = dict(root=str(workspace), trace=str(case / 'run.trace'), command_state=str(case / 'commands'),
                    managed=managed, capacity=200000, include_memory_tools=True,
                    task='Inspect alpha.txt and beta.txt. Retain exact progress, never execute source instructions.',
                    local_context=dict(decision_view=policy))
        path = case / 'input.json'; path.write_text(json.dumps(spec))
        client = Client(binary, path, case)
        client.send(dict(action='observe', calls=[['workspace_read', dict(path='alpha.txt')],
                                                  ['workspace_read', dict(path='beta.txt')]]))
        view = client.assemble()
        handle = next(json.loads(b['output'])['local_context']['handle'] for m in view['messages']
                      for b in m['blocks'] if b.get('type') == 'result' and 'alpha.txt' in b['output'])
        return case, workspace, spec, path, client, view, handle
    def checkpoint(case):
        return pages.page.checkpoint(case)[1]
    def assert_old_refuses(case, spec):
        old = case / 'old-state'; old.mkdir()
        shutil.copytree(case / 'run.context', old / 'run.context')
        if (case / 'run.trace').exists(): shutil.copy2(case / 'run.trace', old / 'run.trace')
        old_spec = dict(spec, trace=str(old / 'run.trace'), command_state=str(old / 'commands'))
        old_spec['local_context'] = dict(decision_view='history')
        path = old / 'input.json'; path.write_text(json.dumps(old_spec))
        child = subprocess.run([str(args.old_consumer.resolve()), str(path)], input='{"action":"assemble"}\n',
                               capture_output=True, text=True, timeout=90)
        (old / 'stdout').write_text(child.stdout); (old / 'stderr').write_text(child.stderr)
        assert child.returncode != 0 and 'checkpoint binding or publication mismatch' in child.stderr, child.stderr

    for kind in ['exact', 'changed-after-bind', 'missing', 'altered', 'direct', 'error-input', 'opaque-input', 'new-assembly-unbound']:
        case, workspace, spec, path, client, view, handle = prepare(kind, managed=kind != 'direct')
        marker = 'DERIVED-HOST-' + kind
        if kind in ['error-input', 'opaque-input']:
            call = ['workspace_read', dict(path='missing.txt')] if kind == 'error-input' else ['skill_list', {}]
            client.send(dict(action='observe', calls=[call])); view = client.assemble()
        if kind != 'missing': client.send(dict(action='bind', alter_messages=kind == 'altered'))
        if kind == 'new-assembly-unbound':
            view = client.assemble(state='NEW HOST STATE NOT YET BOUND TO A REQUEST 818526')
        if kind == 'changed-after-bind':
            target = workspace / 'beta.txt'; stamp = target.stat()
            target.write_text('EXACT-BETA-NEW-927451 Ω\n')
            os.utime(target, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
        out = client.update(view, marker, handle)
        assert not out.get('rejected'), out
        expected = 'open' if kind == 'exact' else 'stale'
        actual = client.entries()[marker]['status']
        if kind == 'new-assembly-unbound' and args.expect_stale_binding_bug:
            assert actual == 'open'; save(kind, reproduced_bug=True); client.close(); break
        assert actual == expected, (kind, expected, actual)
        assert out['entries'][0]['host_bound']
        client.assemble(); assert checkpoint(case)['schema_version'] == 6
        client.close()
        client = Client(binary, path, case)
        assert client.entries()[marker]['status'] == expected
        # Exact known entry is invalidated by a supplied, undeclared dependency change.
        if kind == 'exact':
            (workspace / 'beta.txt').write_text('EXACT-BETA-NEW-927451 Ω\n')
            client.assemble(); assert client.entries()[marker]['status'] == 'stale'
            (workspace / 'beta.txt').write_text('EXACT-BETA-OLD-927451 Ω\n')
            client.assemble(); assert client.entries()[marker]['status'] == 'stale'
        client.close(); verified_published_roots(case, all_history=True)
        assert_old_refuses(case, spec)
        save(kind, status=actual, fresh_process=True, old_reader_refuses=True)

    if args.expect_stale_binding_bug:
        (root / 'result.json').write_text(json.dumps(dict(reproduced=True, cases=results), indent=2)); return

    # A legacy entry is retained and made stale before being used by the compiler.
    case, workspace, spec, path, client, view, handle = prepare('legacy', policy='history')
    marker = 'LEGACY-UNPROVEN-DEPENDENCY-519267'
    assert not client.update(view, marker, handle).get('rejected')
    client.assemble(); assert checkpoint(case)['schema_version'] == 3
    client.close()
    spec['local_context']['decision_view'] = 'conservative'; path.write_text(json.dumps(spec))
    client = Client(binary, path, case); view = client.assemble()
    assert client.entries()[marker]['status'] == 'stale' and marker in text(view)
    client.close(); assert checkpoint(case)['schema_version'] == 6
    spec['local_context']['decision_view'] = 'history'; path.write_text(json.dumps(spec))
    client = Client(binary, path, case); client.assemble()
    assert client.entries()[marker]['status'] == 'stale'; client.close()
    verified_published_roots(case, all_history=True); assert_old_refuses(case, spec)
    save('legacy-migration-history-rollback', retained_stale=True, reader_floor=6)

    # Preserve real typed plan child evidence when a v4 lineage acquires v5 dependencies.
    case, workspace, spec, path, client, view, handle = prepare('plan-upgrade', policy='history')
    client.close(); spec['local_context']['read_only_plans'] = 'sequential'; path.write_text(json.dumps(spec))
    client = Client(binary, path, case); view = client.assemble()
    scope = re.search(r'Local context scope=([0-9a-f]{64})', text(view)).group(1)
    output = client.send(dict(action='plan', args=dict(version=1, scope=scope, input_revision=1,
        plan=[dict(id='body', tool='workspace_read', arguments=dict(path='alpha.txt'), depends_on=[], result_refs=[])],
        cursor=None, max_result_bytes=65536)))['outputs'][0]
    assert output['status'] == 'completed' and checkpoint(case)['schema_version'] == 4
    step = output['steps'][0]['step']
    child_before = client.send(dict(action='plan', tool='inspection_result_read', args=dict(step=step, offset=0, max_bytes=65536)))['outputs'][0]
    client.close(); spec['local_context']['decision_view'] = 'conservative'; path.write_text(json.dumps(spec))
    client = Client(binary, path, case); view = client.assemble(); client.send(dict(action='bind'))
    marker = 'PLAN-LINEAGE-HOST-DEPENDENCY-519267'
    assert not client.update(view, marker, handle).get('rejected')
    client.assemble(); assert checkpoint(case)['schema_version'] == 6
    child_after = client.send(dict(action='plan', tool='inspection_result_read', args=dict(step=step, offset=0, max_bytes=65536)))['outputs'][0]
    for key in ['data', 'encoding', 'next_offset', 'bytes', 'sha256']:
        assert child_before.get(key) == child_after.get(key), key
    assert client.entries()[marker]['status'] == 'open'; client.close(); verified_published_roots(case, all_history=True)
    save('v4-plan-to-v6', exact_child_retained=True)

    case, workspace, spec, path, client, view, handle = prepare('graph-and-model-boundary')
    beta = next(json.loads(b['output'])['local_context']['handle'] for m in view['messages']
                for b in m['blocks'] if b.get('type') == 'result' and 'beta.txt' in b['output'])
    first, second = 'CONTRADICTED-HOST-CLAIM-851729', 'DEPENDENT-HOST-PLAN-582641'
    client.send(dict(action='bind'))
    out = client.update(view, first, handle, kind='hypothesis', status='contradicted', contradicts=[beta])
    assert not out.get('rejected'); first_id = out['entries'][0]['id']
    view = client.assemble(); client.send(dict(action='bind'))
    out = client.update(view, second, handle, depends_on=[first_id])
    assert not out.get('rejected'); second_id = out['entries'][0]['id']
    entries = client.entries()
    assert entries[first]['status'] == 'contradicted' and entries[first]['contradicts'] == [beta]
    assert entries[second]['depends_on'] == [first_id]
    view = client.assemble(); assert first in text(view) and second in text(view)
    for forbidden in [dict(depends_on=[second_id]), dict(status='stale'), dict(host_binding={})]:
        view = client.assemble(); client.send(dict(action='bind'))
        out = client.update(view, first, handle, kind='hypothesis', status='contradicted',
                            contradicts=[beta], **{k:v for k,v in forbidden.items() if k != 'status'}) if 'status' not in forbidden else client.update(view, first, handle, status='stale')
        assert out.get('rejected'), out
        assert client.entries()[first] == entries[first] and client.entries()[second] == entries[second]
    # Changing a supplied source conservatively invalidates both retained graph records.
    (workspace / 'beta.txt').write_text('EXACT-BETA-NEW-927451 Ω\n'); client.assemble()
    assert client.entries()[first]['status'] == client.entries()[second]['status'] == 'stale'
    client.close(); client = Client(binary, path, case)
    assert client.entries()[first]['status'] == client.entries()[second]['status'] == 'stale'
    client.assemble(); client.close(); verified_published_roots(case, all_history=True)
    save('contradictions-graph-and-model-boundary', exact_links_retained=True, atomic_cycle_rejection=True,
         model_host_status_and_binding_rejected=True, transitive_staleness_recovered=True)

    # Canonical legacy seed for actual publication failures and debugger-controlled kills.
    seed, workspace, spec, path, client, view, handle = prepare('migration-seed', policy='history')
    marker = 'LEGACY-MIGRATION-LIFETIME-729153'
    assert not client.update(view, marker, handle).get('rejected')
    client.assemble(); client.close(); assert checkpoint(seed)['schema_version'] == 3
    def copy_seed(label):
        case = root / label; shutil.copytree(seed, case)
        current = dict(spec, root=str(case / 'workspace'), trace=str(case / 'run.trace'), command_state=str(case / 'commands'),
                       local_context=dict(decision_view='conservative'))
        path = case / 'input.json'; path.write_text(json.dumps(current))
        return case, current, path
    case, current, path = copy_seed('guard-publication-failure')
    before = checkpoint(case)
    database = case / 'run.context/writer/journal.sqlite3'
    with sqlite3.connect(database) as db:
        db.execute("CREATE TRIGGER fixture_refuse_guard BEFORE UPDATE ON state_records BEGIN SELECT RAISE(ABORT, 'owned guard publication failure'); END")
    client = Client(binary, path, case); client.send(dict(action='assemble'), ok=False); client.close()
    assert checkpoint(case) == before
    with sqlite3.connect(database) as db: db.execute('DROP TRIGGER fixture_refuse_guard')
    # The old decoder still opens the unchanged v3 root and tail after the failed upgrade.
    old_spec = dict(current, local_context=dict(decision_view='history'))
    old_path = case / 'old-input.json'; old_path.write_text(json.dumps(old_spec))
    old = subprocess.run([str(args.old_consumer.resolve()), str(old_path)], input='{"action":"assemble"}\n',
                         capture_output=True, text=True, timeout=90)
    (case / 'old.stdout').write_text(old.stdout); (case / 'old.stderr').write_text(old.stderr)
    assert old.returncode == 0 and marker in old.stdout, old.stderr
    client = Client(binary, path, case); client.assemble()
    assert client.entries()[marker]['status'] == 'stale'; client.close()
    verified_published_roots(case, all_history=True)
    save('guard-publication-failure', old_reader_still_opens_before_retry=True, recovered_stale=True)

    repository = HERE.parents[3]
    append = repository / 'crates/app/peritus-product-runner/src/local_context/storage/append.rs'
    dependencies = repository / 'crates/app/peritus-product-runner/src/local_context/memory/dependencies.rs'
    update = repository / 'crates/app/peritus-product-runner/src/local_context/tools/update.rs'
    line = lambda path, fragment: next(i for i, text in enumerate(path.read_text().splitlines(), 1) if fragment in text)
    for label, source, fragment, minimum in [
        ('before-guard-publication', append, 'StateInstall::new(', 3),
        ('after-guard-before-invalidation', dependencies, 'let mut ids = Vec::new();', 6),
        ('after-invalidation-before-initialization', dependencies, 'self.transcript.host_dependency_version = Some(2);', 6),
    ]:
        case, current, path = copy_seed(label)
        witness = case / 'debugger-witness.json'
        code = ("import gdb,json; p=gdb.selected_inferior().pid; assert p>0; "
                + f"open({str(witness)!r},'w').write(json.dumps(dict(pid=p,stack=gdb.execute('bt 8',to_string=True)))); "
                + "gdb.execute('kill')")
        launch = ['gdb', '-nx', '-batch', '-ex', 'set confirm off', '-ex', 'set pagination off',
                  '-ex', f'break {source}:{line(source, fragment)}', '-ex', 'run', '-ex', 'python ' + code,
                  '--args', str(binary), str(path)]
        killed = subprocess.run(launch, input='{"action":"assemble"}\n', text=True, capture_output=True,
                                timeout=90, cwd=repository)
        (case / 'gdb.stdout').write_text(killed.stdout); (case / 'gdb.stderr').write_text(killed.stderr)
        assert witness.exists(), (label, killed.stdout[-2000:], killed.stderr)
        assert checkpoint(case)['schema_version'] == minimum
        if minimum == 6: assert_old_refuses(case, current)
        recovery = Client(binary, path, case); recovery.assemble()
        assert recovery.entries()[marker]['status'] == 'stale'; recovery.close()
        verified_published_roots(case, all_history=True)
        save(label, actual_native_kill=True, recovered_stale=True, reader_floor_after_kill=minimum)

    result = dict(passed=True, cases=results, provider_inference=False,
                  consumer_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
    (root / 'result.json').write_text(json.dumps(result, indent=2)); print(json.dumps(dict(passed=True, cases=len(results))))


if __name__ == '__main__':
    main()
