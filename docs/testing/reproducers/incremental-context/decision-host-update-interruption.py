#!/usr/bin/env python3
"""Kill the actual host before/after its durable delta, then recover without redispatch."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('host', HERE / 'decision-host-dependencies.py')
host = importlib.util.module_from_spec(spec); spec.loader.exec_module(host)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--consumer', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args(); binary = a.consumer.resolve(); root = a.output.resolve(); root.mkdir(parents=True)
    seed = root / 'seed'; workspace = seed / 'workspace'; workspace.mkdir(parents=True)
    (workspace / 'alpha.txt').write_text('HOST-DELTA-EXACT-ALPHA-719362 Ω\n')
    def git(*args):
        return subprocess.check_output(['git', '-c', 'user.name=Owned fixture', '-c',
            'user.email=replay@invalid', '-c', 'commit.gpgsign=false', *args], cwd=workspace, stderr=subprocess.PIPE)
    git('init', '-q'); git('add', '--all'); git('commit', '-qm', 'owned source')
    config = dict(root=str(workspace), trace=str(seed / 'run.trace'), command_state=str(seed / 'commands'),
                  managed=True, capacity=200000, include_memory_tools=True, task='Inspect alpha.txt and retain exact progress.',
                  local_context=dict(decision_view='conservative'))
    path = seed / 'input.json'; path.write_text(json.dumps(config))
    c = host.Client(binary, path, seed)
    c.send(dict(action='observe', calls=[['workspace_read', dict(path='alpha.txt')]]))
    view = c.assemble(); handle = next(json.loads(b['output'])['local_context']['handle']
        for m in view['messages'] for b in m['blocks'] if b.get('type') == 'result')
    c.send(dict(action='bind')); assert not c.update(view, 'EXISTING-HOST-PLAN-729153', handle).get('rejected')
    view = c.assemble(); base = int(re.search(r'context_update base_revision=(\d+)', host.text(view)).group(1)); c.close()
    source = HERE.parents[3] / 'crates/app/peritus-product-runner/src/local_context/tools/update.rs'
    results = []
    for label, fragment, retained in [('before-host-delta', 'memory.state_event(&event)?;', False),
                                      ('after-host-delta', 'if matches!(event, WorkingEvent::HostDelta', True)]:
        case = root / label; shutil.copytree(seed, case)
        current = dict(config, root=str(case / 'workspace'), trace=str(case / 'run.trace'), command_state=str(case / 'commands'))
        path = case / 'input.json'; path.write_text(json.dumps(current))
        marker = 'NEW-HOST-DELTA-COMMITTED-581629'
        operation = dict(id=marker, kind='plan', text=marker, supports=[handle], contradicts=[], depends_on=[],
                         status='open', validity='files', files=['alpha.txt'], supersedes=None)
        commands = [dict(action='assemble'), dict(action='bind'), dict(action='plan', tool='context_update',
                    args=dict(base_revision=base, operations=[operation]))]
        witness = case / 'debugger-witness.json'
        code = ("import gdb,json; p=gdb.selected_inferior().pid; assert p>0; "
                + f"open({str(witness)!r},'w').write(json.dumps(dict(pid=p,stack=gdb.execute('bt 8',to_string=True)))); "
                + "gdb.execute('kill')")
        line = next(i for i, s in enumerate(source.read_text().splitlines(), 1) if fragment in s)
        killed = subprocess.run(['gdb', '-nx', '-batch', '-ex', 'set confirm off', '-ex', 'set pagination off',
                '-ex', f'break {source}:{line}', '-ex', 'run', '-ex', 'python ' + code, '--args', str(binary), str(path)],
                input=''.join(json.dumps(command) + '\n' for command in commands), text=True, capture_output=True, timeout=90)
        (case / 'gdb.stdout').write_text(killed.stdout); (case / 'gdb.stderr').write_text(killed.stderr)
        assert witness.exists() and 'tools::update' in json.loads(witness.read_text())['stack'], killed.stdout[-2000:]
        c = host.Client(binary, path, case); entries = c.entries()
        assert (marker in entries) == retained, (label, entries)
        assert entries['EXISTING-HOST-PLAN-729153']['status'] == 'open'
        if retained: assert entries[marker]['status'] == 'open'
        c.assemble(); c.close(); host.verified_published_roots(case)
        # Only checkpoint records may be emitted by this direct API path; no C5 submission
        # or host tool completion is invented to reconcile the interrupted context update.
        data = (case / 'run.trace').read_bytes(); offset = 0
        while offset < len(data):
            assert data[offset] == 6
            offset += 9 + int.from_bytes(data[offset+1:offset+9], 'little')
        assert offset == len(data)
        results.append(dict(case=label, passed=True, new_entry_retained=retained, provider_submissions=0, redispatch=False))
    result = dict(passed=True, cases=results)
    (root / 'result.json').write_text(json.dumps(result, indent=2)); print(json.dumps(result))


if __name__ == '__main__': main()
