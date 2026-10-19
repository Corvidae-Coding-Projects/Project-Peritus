#!/usr/bin/env python3
"""Build the ordinary native-provider/recorded-transcript consumer; no test target."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path)
parser.add_argument('--baseline', help='Git revision supplying only the pre-fix decoder')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[3]
out = args.output.resolve()
(out / 'src').mkdir(parents=True, exist_ok=True)
prefix = 'crates/model/peritus-provider-openai/src/runtime/'
decoder = out / 'decoder/mod.rs'
decoder.parent.mkdir(parents=True, exist_ok=True)
for relative, filename in (
    ('output.rs', 'mod.rs'),
    ('output/failure.rs', 'failure.rs'),
    ('output/selection.rs', 'selection.rs'),
):
    content = (subprocess.check_output(['git', 'show', f'{args.baseline}:{prefix}{relative}'], cwd=repo)
               if args.baseline else (repo / prefix / relative).read_bytes())
    (decoder.parent / filename).write_bytes(content)
source = Path(__file__).with_name('codex-notification-replay.rs').read_text()
(out / 'src/main.rs').write_text(source.replace(
    '"../../../crates/model/peritus-provider-openai/src/runtime/output.rs"', json.dumps(str(decoder))))
package_name = 'peritus-real-codex-notifications-' + hashlib.sha256(str(out).encode()).hexdigest()[:8]
manifest = f'''[package]
name="{package_name}"
version="0.1.0"
edition="2024"
autobins=false
[workspace]
[[bin]]
name="{package_name}"
path="src/main.rs"
[dependencies]
serde_json="=1.0.149"
serde={{version="=1.0.229",features=["derive"]}}
tokio={{version="=1.53.1",features=["rt-multi-thread","macros"]}}
'''
for package, directory in {
    'peritus-agent': 'crates/orchestration/peritus-agent',
    'peritus-model-protocol': 'crates/model/peritus-model-protocol',
    'peritus-provider-core': 'crates/model/peritus-provider-core',
    'peritus-provider-openai': 'crates/model/peritus-provider-openai',
    'peritus-types': 'crates/foundation/peritus-types',
}.items():
    manifest += f'{package}={{path={json.dumps(str(repo / directory))}}}\n'
manifest += '''[profile.release]
codegen-units=1
incremental=false
lto="thin"
overflow-checks=true
panic="abort"
strip="symbols"
'''
(out / 'Cargo.toml').write_text(manifest)
subprocess.run(['cargo', 'generate-lockfile', '--offline', '--manifest-path', str(out / 'Cargo.toml')], cwd=repo, check=True)
subprocess.run(['cargo', 'build', '--offline', '--locked', '--release', '--jobs', '2', '--target-dir', str(repo / 'target'), '--manifest-path', str(out / 'Cargo.toml')], cwd=repo, check=True)
shutil.copy2(repo / 'target/release' / package_name, out / 'consumer')
print(out / 'consumer')
