#!/usr/bin/env python3
"""An ignored, undeclared read is not covered by the managed candidate fingerprint."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
s = importlib.util.spec_from_file_location('host', HERE / 'decision-host-dependencies.py')
h = importlib.util.module_from_spec(s); s.loader.exec_module(h)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--consumer', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--expect-bug', action='store_true')
    a = p.parse_args(); root = a.output.resolve(); root.mkdir(parents=True)
    results = []
    for ignored in [True, False]:
        case = root / ('ignored' if ignored else 'owned'); workspace = case / 'workspace'
        workspace.mkdir(parents=True)
        (workspace / 'alpha.txt').write_text('ALPHA-OWNED-591726 Ω\n')
        (workspace / '.gitignore').write_text('beta.txt\n' if ignored else '')
        git = lambda *args: subprocess.check_output(['git', '-c', 'user.name=Owned fixture', '-c',
            'user.email=replay@invalid', '-c', 'commit.gpgsign=false', *args], cwd=workspace, stderr=subprocess.PIPE)
        git('init', '-q'); git('add', '--all'); git('commit', '-qm', 'owned source')
        target = workspace / 'beta.txt'; target.write_text('BETA-OLD-827451 Ω\n')
        spec = dict(root=str(workspace), trace=str(case / 'run.trace'), command_state=str(case / 'commands'),
            managed=True, capacity=200000, include_memory_tools=True,
            task='Read alpha.txt and beta.txt. Retain exact investigation progress.',
            local_context=dict(decision_view='conservative'))
        path = case / 'input.json'; path.write_text(json.dumps(spec))
        c = h.Client(a.consumer.resolve(), path, case)
        c.send(dict(action='observe', calls=[['workspace_read', dict(path='alpha.txt')],
                                           ['workspace_read', dict(path='beta.txt')]]))
        view = c.assemble()
        handle = next(json.loads(b['output'])['local_context']['handle'] for m in view['messages']
                      for b in m['blocks'] if b.get('type') == 'result' and 'alpha.txt' in b['output'])
        c.send(dict(action='bind')); marker = 'DERIVED-FROM-BOTH-READS-827159'
        out = c.update(view, marker, handle); assert not out.get('rejected'), out
        initial = c.entries()[marker]['status']
        stamp = target.stat(); target.write_text('BETA-NEW-827451 Ω\n')
        os.utime(target, ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
        c.assemble(); changed = c.entries()[marker]['status']; c.close()
        if a.expect_bug and ignored:
            assert initial == changed == 'open', (initial, changed)
        else:
            assert initial == ('stale' if ignored else 'open'), initial
            assert changed == 'stale', changed
            c = h.Client(a.consumer.resolve(), path, case); c.assemble()
            assert c.entries()[marker]['status'] == 'stale'; c.close()
            h.verified_published_roots(case)
        results.append(dict(ignored=ignored, initial_status=initial, changed_status=changed))
    result = dict(passed=not a.expect_bug, expected_bug=a.expect_bug, cases=results)
    (root / 'result.json').write_text(json.dumps(result, indent=2)); print(json.dumps(result))


if __name__ == '__main__':
    main()
