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
p.add_argument('--candidate-output', type=Path)
p.add_argument('--provider-output', type=Path)
p.add_argument('--config-output', type=Path)
p.add_argument('--control-output', type=Path)
p.add_argument('--command-output', type=Path)
a = p.parse_args()
libraries = ['peritus_daemon', 'peritus_agent', 'peritus_product_runner', 'peritus_process', 'peritus_role', 'peritus_provider_core', 'peritus_provider_openai', 'peritus_provider_opencode', 'peritus_model_protocol', 'peritus_types', 'serde_json', 'tokio', 'zeroize']
if a.candidate_output:
    libraries.append('peritus_workspace')
if a.control_output:
    libraries.extend(['peritus_workspace', 'peritus_app_client', 'peritus_app_protocol', 'peritus_approval'])
command = ['rustc', '--edition=2024', '-C', 'debuginfo=0', str(Path(__file__).with_name('reuse-consumer.rs')), '-L', 'dependency=' + str(a.deps), '-o', str(a.output)]
for name, rlib in select(a.deps, libraries, anchor='peritus_daemon').items():
    command.extend(['--extern', name + '=' + str(rlib)])
for native in a.deps.parent.glob('build/*/out'):
    if any(native.glob('*.a')):
        command.extend(['-L', 'native=' + str(native)])
subprocess.run(command, check=True)
if a.ordinary_output:
    command[command.index(str(Path(__file__).with_name('reuse-consumer.rs')))] = str(Path(__file__).resolve().parent.parent / 'checkpoint-cache/context-consumer.rs')
    command[command.index('-o') + 1] = str(a.ordinary_output)
    subprocess.run(command, check=True)
if a.command_output:
    command[command.index('-C') + 2] = str(Path(__file__).with_name('command-owner-consumer.rs'))
    command[command.index('-o') + 1] = str(a.command_output)
    subprocess.run(command, check=True)
if a.provider_output:
    command[command.index('-C') + 2] = str(Path(__file__).with_name('opencode-layout-consumer.rs'))
    command[command.index('-o') + 1] = str(a.provider_output)
    subprocess.run(command, check=True)
if a.config_output:
    command[command.index('-C') + 2] = str(Path(__file__).with_name('opencode-layout-config.rs'))
    command[command.index('-o') + 1] = str(a.config_output)
    subprocess.run(command, check=True)
if a.candidate_output:
    command[command.index('-C') + 2] = str(Path(__file__).with_name('candidate-observation-consumer.rs'))
    command[command.index('-o') + 1] = str(a.candidate_output)
    subprocess.run(command, check=True)
if a.control_output:
    command[command.index('-C') + 2] = str(Path(__file__).with_name('plan-control-client.rs'))
    command[command.index('-o') + 1] = str(a.control_output)
    subprocess.run(command, check=True)
