#!/usr/bin/env python3
"""Interrupt actual full Git captures; fresh owners must reacquire exact identities."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import subprocess

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    binary = args.consumer.resolve()
    helpers = runpy.run_path(str(HERE / 'candidate-observation-replay.py'))
    results = []
    for name in ['change-between-captures', 'kill-between-captures', 'kill-after-second-capture',
                 'invalid-head-and-repair']:
        case = root / name
        workspace = case / 'workspace'
        helpers['initialize'](workspace)
        target = workspace / 'value.txt'
        before = 'EXACT-ORIGINAL-146982 Ω\n'
        after = 'EXACT-MODIFIED-257193 Ω\n'
        assert len(before.encode()) == len(after.encode())
        target.write_text(before)
        helpers['commit'](workspace, 'private original')
        owner = helpers['Owner'](binary, case)
        original = owner.call(workspace, 'shared')
        target.write_text(after)
        expected = owner.call(workspace, 'legacy')
        target.write_text(before)
        owner.close()
        index = (workspace / '.git/index').read_bytes()
        request = case / 'request.jsonl'
        request.write_text(json.dumps(dict(root=str(workspace), mode='shared')) + '\n')
        if name == 'invalid-head-and-repair':
            head = workspace / '.git/HEAD'
            saved = head.read_bytes()
            head.write_text('INVALID-OWNED-HEAD\n')
            owner = helpers['Owner'](binary, case)
            rejected = {mode: owner.call(workspace, mode) for mode in ['legacy', 'shared']}
            owner.close()
            assert all('error' in value for value in rejected.values()), rejected
            (case / 'rejected.json').write_text(json.dumps(rejected, indent=2))
            head.write_bytes(saved)
        else:
            source = REPO / 'crates/runtime/peritus-workspace/src/task_candidate/managed.rs'
            lines = source.read_text().splitlines()
            start = next(n for n, text in enumerate(lines) if 'pub fn repository_and_source_digests' in text)
            fragment = ('Ok((snapshot.repository_digest(root)?' if name == 'kill-after-second-capture'
                        else 'let snapshot = Self::capture')
            line = next(n + 1 for n, text in enumerate(lines) if n > start and fragment in text)
            witness = case / 'boundary.json'
            script = ('set pagination off\nset confirm off\nset detach-on-fork on\n'
                      f'break {source}:{line}\n')
            script += 'ignore 1 1\n'
            script += f'run < {request}\npython\nimport gdb,json,os\n'
            script += (f'open({str(witness)!r},"w").write(json.dumps(dict('
                       'pid=gdb.selected_inferior().pid,stack=gdb.execute("bt 8",to_string=True))))\n')
            if name == 'change-between-captures':
                # The next capture must see a same-size, restored-mtime amendment. No watcher
                # or fake Git response tells the real implementation which bytes changed.
                script += (f'p={str(target)!r}\ns=os.stat(p)\n'
                           f'open(p,"w").write({after!r})\n'
                           'os.utime(p,ns=(s.st_atime_ns,s.st_mtime_ns))\n')
            script += 'end\ndisable 1\n' + ('continue\n' if name == 'change-between-captures' else 'kill\n')
            gate = case / 'gate.gdb'
            gate.write_text(script)
            run = subprocess.run(['gdb', '-nx', '-batch', '-x', str(gate), '--args', str(binary)],
                                 cwd=REPO, text=True, capture_output=True, timeout=90)
            (case / 'gdb.stdout').write_text(run.stdout)
            (case / 'gdb.stderr').write_text(run.stderr)
            assert witness.exists(), (run.stdout[-3000:], run.stderr[-1000:])
            if name == 'change-between-captures':
                rows = [json.loads(line) for line in run.stdout.splitlines()
                        if line.startswith('{"attempts":')]
                assert len(rows) == 1 and rows[0]['attempts'] == 2, run.stdout[-2000:]
                assert helpers['identity'](rows[0]) == helpers['identity'](expected)
            else:
                assert not any(line.startswith('{"attempts":') for line in run.stdout.splitlines())
                target.write_text(after)
        owner = helpers['Owner'](binary, case)
        shared = owner.call(workspace, 'shared')
        separate = owner.call(workspace, 'legacy')
        owner.close()
        wanted = original if name == 'invalid-head-and-repair' else expected
        assert helpers['identity'](shared) == helpers['identity'](separate) == helpers['identity'](wanted)
        assert (workspace / '.git/index').read_bytes() == index
        assert sorted(p.name for p in workspace.iterdir()) == ['.git', 'value.txt']
        result = dict(case=name, passed=True, fresh_process=True, no_inference=True,
                      unchanged_index=True, exact_reacquisition=True)
        results.append(result)
        (root / 'results.json').write_text(json.dumps(results, indent=2))
        print(json.dumps(result), flush=True)
    (root / 'identity.json').write_text(json.dumps(dict(
        consumer_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()), indent=2))


if __name__ == '__main__':
    main()
