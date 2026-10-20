#!/usr/bin/env python3
"""Link the replay against the actual freshly built production dependency cohort."""
import argparse
import json
from pathlib import Path
import subprocess
p = argparse.ArgumentParser()
p.add_argument('--deps', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--artifacts', type=Path, required=True)
a = p.parse_args()
libraries = ['peritus_agent', 'peritus_product_runner', 'peritus_process', 'peritus_role', 'peritus_provider_core', 'peritus_provider_openai', 'peritus_model_protocol', 'peritus_types', 'serde_json', 'tokio']
command = ['rustc', '--edition=2024', '-C', 'debuginfo=0', str(Path(__file__).with_name('consumer.rs')), '-L', 'dependency=' + str(a.deps), '-o', str(a.output)]
artifacts={}
for line in a.artifacts.read_text().splitlines():
    row=json.loads(line)
    if row.get('reason')=='compiler-artifact':
        for filename in row['filenames']:
            if filename.endswith('.rlib'): artifacts[row['target']['name']]=filename
for name in libraries:
    rlib=artifacts[name]
    command.extend(['--extern', name + '=' + str(rlib)])
for native in a.deps.parent.glob('build/*/out'):
    if any(native.glob('*.a')):
        command.extend(['-L', 'native=' + str(native)])
subprocess.run(command, check=True)
