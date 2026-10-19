#!/usr/bin/env python3
"""Link the replay against the actual freshly built production dependency cohort."""
import argparse
from pathlib import Path
import subprocess
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'checkpoint-cache'))
from cohort import select
p = argparse.ArgumentParser()
p.add_argument('--deps', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
p.add_argument('--ordinary-output', type=Path)
a = p.parse_args()
libraries = ['peritus_agent', 'peritus_product_runner', 'peritus_process', 'peritus_role', 'peritus_provider_core', 'peritus_provider_openai', 'peritus_model_protocol', 'peritus_types', 'serde_json', 'tokio']
command = ['rustc', '--edition=2024', '-C', 'debuginfo=0', str(Path(__file__).with_name('reuse-consumer.rs')), '-L', 'dependency=' + str(a.deps), '-o', str(a.output)]
for name, rlib in select(a.deps, libraries).items():
    command.extend(['--extern', name + '=' + str(rlib)])
for native in a.deps.parent.glob('build/*/out'):
    if any(native.glob('*.a')):
        command.extend(['-L', 'native=' + str(native)])
subprocess.run(command, check=True)
if a.ordinary_output:
    command[command.index(str(Path(__file__).with_name('reuse-consumer.rs')))] = str(Path(__file__).resolve().parent.parent / 'checkpoint-cache/context-consumer.rs')
    command[command.index('-o') + 1] = str(a.ordinary_output)
    subprocess.run(command, check=True)
