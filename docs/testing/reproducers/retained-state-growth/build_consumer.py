#!/usr/bin/env python3
"""Build actual public consumers outside the checkout; never run a test framework."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('output', type=Path)
parser.add_argument('--kind', choices=('c6', 'protocol', 'scope', 'host'), required=True)
parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[4])
args = parser.parse_args()
repo, output = args.repo.resolve(), args.output.resolve()
output.mkdir(mode=0o700, parents=True, exist_ok=False)
(output/'src').mkdir()
base = Path(__file__).resolve().parent
source = {'c6': base/'consumer.rs', 'protocol': base/'protocol_consumer.rs',
          'scope': base.parent/'in-place-scope/consumer.rs', 'host': base/'host_consumer.rs'}[args.kind]
shutil.copy2(source, output/'src/main.rs')
packages = {
    'peritus-context': 'crates/orchestration/peritus-context',
    'peritus-codec': 'crates/foundation/peritus-codec',
    'peritus-types': 'crates/foundation/peritus-types',
    'peritus-role': 'crates/orchestration/peritus-role',
}
if args.kind == 'protocol':
    packages = {'peritus-model-protocol': 'crates/model/peritus-model-protocol'}
elif args.kind == 'scope':
    packages = {'peritus-product-runner': 'crates/app/peritus-product-runner'}
elif args.kind == 'host':
    packages.update({
        'peritus-agent': 'crates/orchestration/peritus-agent',
        'peritus-model-protocol': 'crates/model/peritus-model-protocol',
        'peritus-product-runner': 'crates/app/peritus-product-runner',
        'peritus-provider-openai': 'crates/model/peritus-provider-openai',
        'peritus-provider-core': 'crates/model/peritus-provider-core',
        'peritus-process': 'crates/runtime/peritus-process',
    })
name = 'peritus-retained-' + args.kind + '-consumer'
manifest = f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2024"\n[workspace]\n'
manifest += '[features]\ngrowth-api = []\ncomplete-api = []\n[dependencies]\nserde_json = "=1.0.149"\n'
for package, relative in packages.items():
    manifest += f'{package} = {{ path = {json.dumps(str(repo/relative))} }}\n'
manifest += '\n[profile.release]\ncodegen-units = 1\nincremental = false\nlto = "thin"\noverflow-checks = true\npanic = "abort"\nstrip = "symbols"\n'
(output/'Cargo.toml').write_text(manifest)
if args.kind == 'c6':
    fixtures = output/'original-inputs'
    fixtures.mkdir()
    for filename, expected in json.loads((base/'fixtures/manifest.json').read_text()).items():
        data = gzip.decompress((base/'fixtures'/f'{filename}.gz').read_bytes())
        assert len(data) == expected['bytes'] and hashlib.sha256(data).hexdigest() == expected['sha256']
        (fixtures/filename).write_bytes(data)
subprocess.run(['cargo', 'build', '--offline', '--release', '--jobs', '2', '--features', 'growth-api',
                '--target-dir', str(repo/'target'), '--manifest-path', str(output/'Cargo.toml')], cwd=repo, check=True)
print(repo/'target/release'/name)
