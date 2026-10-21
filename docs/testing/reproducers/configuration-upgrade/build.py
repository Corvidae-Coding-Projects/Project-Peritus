#!/usr/bin/env python3
"""Link a public bootstrap consumer against the actual CLI build's artifact cohort."""
import argparse
import json
import subprocess
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--cargo-output', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
artifacts = {}
for line in a.cargo_output.read_text().splitlines():
    record = json.loads(line)
    if record.get('reason') == 'compiler-artifact':
        for filename in record['filenames']:
            if filename.endswith('.rlib'):
                artifacts[record['target']['name']] = filename
deps = Path(artifacts['peritus_launcher']).parent
command = ['rustc', '--edition=2024', '-C', 'debuginfo=0',
           str(Path(__file__).with_name('consumer.rs')), '-L', 'dependency=' + str(deps),
           '-o', str(a.output)]
for name in ('peritus_launcher', 'peritus_product_state', 'peritus_workspace', 'serde_json'):
    command += ['--extern', name + '=' + artifacts[name]]
for native in deps.parent.glob('build/*/out'):
    if any(native.glob('*.a')):
        command += ['-L', 'native=' + str(native)]
subprocess.run(command, check=True)
