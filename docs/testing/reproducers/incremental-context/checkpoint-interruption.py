#!/usr/bin/env python3
"""Kill real native consumers at actual durability boundaries; recover owned C0/C6 state."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('checkpoint_pages', HERE / 'checkpoint-pages.py')
pages = importlib.util.module_from_spec(spec); spec.loader.exec_module(pages)


def source_line(path, fragment):
    return next(i for i, line in enumerate(path.read_text().splitlines(), 1) if fragment in line)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--seed', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(); root = args.output.resolve(); root.mkdir(parents=True, exist_ok=False)
    original = json.loads((args.seed / 'input.json').read_text())
    workspace = Path(original['root']); target = workspace / 'source-00.txt'; old_bytes = target.read_bytes()
    repository = HERE.parents[3]
    writer = repository / 'crates/state/peritus-artifact-store/src/writer.rs'
    append = repository / 'crates/app/peritus-product-runner/src/local_context/storage/append.rs'
    pre_artifact = source_line(writer, 'synchronize_temporary(file)?')
    pre_publication = source_line(append, 'StateInstall::new(')
    post_publication = source_line(append, 'self.generation = next_generation;') + 1
    results = []
    try:
        for reuse in [False, True]:
            for kind in ['before-artifact-finalization', 'before-checkpoint-publication',
                         'after-checkpoint-publication', 'after-batch-before-request',
                         'after-published-batch-before-request', 'publication-failure', 'batch-publication-failure']:
                case = root / f'{reuse}-{kind}'; shutil.copytree(args.seed, case)
                marker = f'INTERRUPTION-EXACT-{reuse}-{kind}-719846 Ω'
                target.write_text(marker + '\n'); (case / 'new-source.txt').write_text(marker + '\n')
                spec = dict(original, trace=str(case / 'run.trace'), command_state=str(case / 'commands'),
                            local_context={'reuse_checkpoint_pages': reuse})
                path = case / 'input.json'; path.write_text(json.dumps(spec))
                _, _, before, _ = pages.page.checkpoint(case)
                commands = [dict(action='observe', calls=[['workspace_read', {'path': 'source-00.txt'}]]),
                            dict(action='assemble')]
                if kind in ['before-artifact-finalization', 'before-checkpoint-publication', 'after-checkpoint-publication']:
                    stop_file, stop_line = (writer, pre_artifact) if kind == 'before-artifact-finalization' else (append, pre_publication)
                    launch = ['gdb', '-nx', '-batch', '-ex', 'set confirm off', '-ex', 'set pagination off',
                              '-ex', f'break {stop_file}:{stop_line}', '-ex', 'run']
                    if kind == 'after-checkpoint-publication':
                        launch += ['-ex', f'tbreak {append}:{post_publication}', '-ex', 'continue']
                    witness = case / 'debugger-witness.json'
                    code = ("import gdb,json; p=gdb.selected_inferior().pid; assert p>0; "
                            + f"open({str(witness)!r},'w').write(json.dumps(dict(pid=p,stack=gdb.execute('bt 6',to_string=True)))); "
                            + "gdb.execute('kill')")
                    launch += ['-ex', 'python ' + code, '--args', str(args.consumer.resolve()), str(path)]
                    run = subprocess.run(launch, input=''.join(json.dumps(x)+'\n' for x in commands),
                                         text=True, capture_output=True, timeout=60, cwd=repository)
                    (case / 'gdb.stdout').write_text(run.stdout); (case / 'gdb.stderr').write_text(run.stderr)
                    assert witness.exists(), (kind, run.stdout[-2000:], run.stderr)
                    witness_value = json.loads(witness.read_text())
                    expected = 'ArtifactWriter::finalize' if kind == 'before-artifact-finalization' else 'storage::LocalStore::append'
                    assert expected in witness_value['stack'], witness_value
                elif kind in ['after-batch-before-request', 'after-published-batch-before-request']:
                    client = pages.Client(args.consumer.resolve(), path, case)
                    if kind == 'after-published-batch-before-request':
                        client.request(dict(action='assemble'))
                    client.request(commands[0])
                    client.process.kill(); client.process.wait(timeout=10); client.errors.close()
                else:
                    client = pages.Client(args.consumer.resolve(), path, case)
                    if kind == 'batch-publication-failure':
                        client.request(dict(action='assemble'))
                        _, _, before, _ = pages.page.checkpoint(case)
                    else:
                        client.request(commands[0])
                    database = case / 'run.context/writer/journal.sqlite3'
                    with sqlite3.connect(database) as db:
                        db.execute("CREATE TRIGGER fixture_refuse_checkpoint BEFORE UPDATE ON state_records BEGIN SELECT RAISE(ABORT, 'owned checkpoint publication failure'); END")
                    client.request(commands[0] if kind == 'batch-publication-failure' else commands[1], rejected=True)
                    client.close()
                    with sqlite3.connect(database) as db:
                        db.execute('DROP TRIGGER fixture_refuse_checkpoint')
                _, _, interrupted, _ = pages.page.checkpoint(case)
                if kind in ['after-checkpoint-publication', 'after-published-batch-before-request']:
                    assert interrupted != before, 'checkpoint did not commit before interruption'
                else:
                    assert interrupted == before, 'unpublished checkpoint changed the committed root'
                # No effect redispatch: a fresh process assembles retained data without a read call.
                recovery = pages.Client(args.consumer.resolve(), path, case)
                try:
                    recovered = recovery.request(dict(action='assemble'))
                    text = json.dumps(recovered, ensure_ascii=False)
                    if kind == 'before-artifact-finalization':
                        assert marker not in text, 'unobserved source became an established read'
                    else:
                        assert marker in text, 'completed observation was lost'
                    assert 'PENDING HOST OPERATIONS' not in text or 'DO NOT REDISPATCH' in text
                finally:
                    recovery.close()
                pages.verified_roots(case)
                trace = case / 'run.trace'
                data = trace.read_bytes(); offset = 0; tags = []
                while offset < len(data):
                    tags.append(data[offset]); size = int.from_bytes(data[offset+1:offset+9], 'little'); offset += 9+size
                assert offset == len(data) and set(tags) <= {6}, 'local recovery invented a provider/tool submission'
                results.append(dict(reuse=reuse, case=kind, passed=True, provider_submissions=0,
                                    committed_root_changed=interrupted != before))
                target.write_bytes(old_bytes)
    finally:
        target.write_bytes(old_bytes)
    result = dict(passed=True, cases=results, consumer_sha256=hashlib.sha256(args.consumer.read_bytes()).hexdigest(),
                  scope='actual native kills and SQLite publication failure; no injected provider or synthetic success')
    (root / 'result.json').write_text(json.dumps(result, indent=2)); print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
