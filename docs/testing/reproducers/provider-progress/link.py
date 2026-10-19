#!/usr/bin/env python3
"""Link the ordinary real-provider consumer against a built daemon's actual dependencies."""
import argparse
from pathlib import Path
import subprocess
import sys

here = Path(__file__).resolve().parent
sys.path.insert(0, str(here.parent / 'checkpoint-cache'))
from cohort import select

parser = argparse.ArgumentParser()
parser.add_argument('--deps', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.parent.mkdir(parents=True, exist_ok=True)
command = ['rustc', '--edition=2024', str(here / 'provider-consumer.rs'),
           '-L', 'dependency=' + str(args.deps), '-o', str(args.output)]
for library, path in select(args.deps, ['peritus_model_protocol', 'peritus_provider_core',
                                      'peritus_provider_openai', 'peritus_types',
                                      'serde_json', 'tokio']).items():
    command.extend(['--extern', library + '=' + str(path)])
for native in sorted(args.deps.parent.glob('build/*/out')):
    if any(native.glob('*.a')):
        command.extend(['-L', 'native=' + str(native)])
subprocess.run(command, check=True)
