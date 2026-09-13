import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

root = Path('/home/doll/Project-Peritus/.worktrees/formal-coverage')
frozen = Path('/tmp/peritus-sol-obligations-performance-source-01')
package = Path('crates/orchestration/peritus-obligations')
record = Path('/tmp/peritus-parent-obligations356-qualification-integrated.json')
env = dict(os.environ, CARGO_BUILD_JOBS='2', CCACHE_DISABLE='1',
           CARGO_TARGET_DIR=str(root / 'target'))

def read_manifest(path):
    return {name: digest for digest, name in
            (line.split('  ', 1) for line in Path(path).read_text().splitlines())}

def verify(base, entries):
    found = {str(p.relative_to(base)) for p in (base / package).rglob('*') if p.is_file()}
    assert found == set(entries), (str(base), found - set(entries), set(entries) - found)
    for name, digest in entries.items():
        assert hashlib.sha256((base / name).read_bytes()).hexdigest() == digest, name

prior = read_manifest('/tmp/peritus-parent-obligations-foundations-full-package.sha256')
final = read_manifest('/tmp/peritus-sol-obligations-performance-full-package.sha256')
changed = read_manifest('/tmp/peritus-parent-obligations225-to-356-changed.sha256')
verify(root, prior)
verify(frozen, final)
assert set(prior) <= set(final)
assert set(changed) == {n for n in final if final[n] != prior.get(n)}
for name in changed:
    dest = root / name
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(frozen / name, dest)
verify(root, final)
results = {'integrated_files': len(changed), 'prior_files_checked': len(prior),
           'final_files_checked': len(final), 'cwd': str(root),
           'environment': {k: env[k] for k in
               ['CARGO_BUILD_JOBS', 'CCACHE_DISABLE', 'CARGO_TARGET_DIR']}, 'commands': []}
record.write_text(json.dumps(results, indent=2) + '\n')
commands = [
    ('verus', ['cargo', 'verus', 'verify', '--package', 'peritus-obligations',
               '--all-features', '--locked', '--check-toolchain', '--fwd-verus-args-to',
               'roots', '--', '--no-cheating', '--rlimit', '20']),
    ('tests-all', ['cargo', 'test', '--package', 'peritus-obligations', '--all-features', '--locked']),
    ('tests-default', ['cargo', 'test', '--package', 'peritus-obligations', '--locked']),
    ('clippy', ['cargo', 'clippy', '--package', 'peritus-obligations', '--all-features',
                '--all-targets', '--locked', '--', '-D', 'warnings']),
    ('fmt', ['cargo', 'fmt', '--package', 'peritus-obligations', '--', '--check']),
    ('caller-tests', ['cargo', 'test', '--package', 'peritus-gates', '--package',
                      'peritus-product-runner', '--all-features', '--lib', '--locked', 'obligation']),
    ('api', [str(root / 'target/debug/xtask'), 'ordinary-api-check']),
    ('layout', [str(root / 'target/debug/xtask'), 'source-layout-check']),
]
for label, command in commands:
    verify(root, final)
    log = Path('/tmp/peritus-parent-obligations356-' + label + '-integrated.log')
    print('START ' + label, flush=True)
    with log.open('w') as output:
        run = subprocess.run(command, cwd=root, env=env, stdout=output,
                             stderr=subprocess.STDOUT, timeout=600)
    verify(root, final)
    results['commands'].append({'command': command, 'log': str(log), 'exit_code': run.returncode})
    record.write_text(json.dumps(results, indent=2) + '\n')
    print(label + ' exit=' + str(run.returncode), flush=True)
    if run.returncode:
        print(log.read_text()[-6000:], flush=True)
        raise SystemExit(run.returncode)
print('Integrated qualification passed; all51 final source identities retained.', flush=True)
