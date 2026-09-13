import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

root = Path('/tmp/peritus-parent-obligations-proof.6ib_z8mo')
manifest = Path('/tmp/peritus-sol-obligations-performance-full-package.sha256')
prefix = Path('/tmp/peritus-parent-obligations356-negative-canonical')
source = root / 'crates/orchestration/peritus-obligations/src/canonical/encode.rs'
command = ['cargo', 'verus', 'verify', '--package', 'peritus-obligations',
           '--all-features', '--locked', '--check-toolchain', '--fwd-verus-args-to',
           'roots', '--', '--no-cheating', '--rlimit', '20']
env = dict(os.environ, CARGO_BUILD_JOBS='2', CCACHE_DISABLE='1',
           CARGO_TARGET_DIR='/home/doll/Project-Peritus/.worktrees/formal-coverage/target/formal-obligations-parent')

def digest(data):
    return hashlib.sha256(data).hexdigest()

def check_sources():
    entries = [line.split('  ', 1) for line in manifest.read_text().splitlines()]
    for expected, name in entries:
        assert digest((root / name).read_bytes()) == expected, name
    return len(entries)

records = {'manifest_sha256': digest(manifest.read_bytes()),
           'source_files_checked': check_sources(), 'cwd': str(root),
           'command': command, 'environment': {k: env[k] for k in
               ['CARGO_BUILD_JOBS', 'CCACHE_DISABLE', 'CARGO_TARGET_DIR']},
           'mutation': 'Change actual Hard obligation encoder tag from 1 to 2; leave byte model unchanged.'}
original = source.read_bytes()
needle = b'ObligationSpec::Hard => 1,'
assert original.count(needle) == 1
mutant = original.replace(needle, b'ObligationSpec::Hard => 2,', 1)
records['original_sha256'] = digest(original)
records['mutant_sha256'] = digest(mutant)
try:
    source.write_bytes(mutant)
    log = Path(str(prefix) + '-mutant.log')
    with log.open('w') as output:
        result = subprocess.run(command, cwd=root, env=env, stdout=output,
                                stderr=subprocess.STDOUT, timeout=240)
    data = log.read_text()
    records['mutant'] = {'exit_code': result.returncode, 'log': str(log),
                         'verification_results': re.findall(r'verification results::[^\n]*', data)}
finally:
    source.write_bytes(original)
    records['restored_files_checked'] = check_sources()
    Path(str(prefix) + '.json').write_text(json.dumps(records, indent=2) + '\n')

assert records['mutant']['exit_code'] != 0
assert 'postcondition not satisfied' in data
assert 'src/canonical/encode.rs' in data
log = Path(str(prefix) + '-restored.log')
with log.open('w') as output:
    result = subprocess.run(command, cwd=root, env=env, stdout=output,
                            stderr=subprocess.STDOUT, timeout=240)
records['restored'] = {'exit_code': result.returncode, 'log': str(log),
                       'verification_results': re.findall(r'verification results::[^\n]*', log.read_text())}
records['final_files_checked'] = check_sources()
Path(str(prefix) + '.json').write_text(json.dumps(records, indent=2) + '\n')
assert result.returncode == 0
assert 'verification results:: 356 verified, 0 errors' in log.read_text()
print(json.dumps(records, indent=2), flush=True)
