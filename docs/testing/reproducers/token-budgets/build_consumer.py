#!/usr/bin/env python3
"""Build real public API consumers outside the workspace with bounded Cargo workers."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("output", type=Path)
parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[4])
parser.add_argument("--host", action="store_true")
args = parser.parse_args()
repo = args.repo.resolve()
output = args.output.resolve()
output.mkdir(mode=0o700, parents=True, exist_ok=False)
(output / "src").mkdir()
source = "host_consumer.rs" if args.host else "consumer.rs"
shutil.copy2(Path(__file__).with_name(source), output / "src/main.rs")
packages = {
    "peritus-context": "crates/orchestration/peritus-context",
    "peritus-codec": "crates/foundation/peritus-codec",
    "peritus-types": "crates/foundation/peritus-types",
    "peritus-role": "crates/orchestration/peritus-role",
}
if args.host:
    packages.update({
        "peritus-agent": "crates/orchestration/peritus-agent",
        "peritus-model-protocol": "crates/model/peritus-model-protocol",
        "peritus-product-runner": "crates/app/peritus-product-runner",
        "peritus-provider-openai": "crates/model/peritus-provider-openai",
        "peritus-provider-core": "crates/model/peritus-provider-core",
        "peritus-process": "crates/runtime/peritus-process",
    })
name = "peritus-token-budget-host-consumer" if args.host else "peritus-token-budget-consumer"
manifest = f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2024"\n[workspace]\n'
manifest += '[features]\ncomplete-api = []\n[dependencies]\nserde_json = "=1.0.149"\n'
for package, relative in packages.items():
    manifest += f'{package} = {{ path = {json.dumps(str(repo / relative))} }}\n'
manifest += '\n[profile.release]\ncodegen-units = 1\nincremental = false\nlto = "thin"\noverflow-checks = true\npanic = "abort"\nstrip = "symbols"\n'
(output / "Cargo.toml").write_text(manifest)
subprocess.run(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(output / "Cargo.toml")], cwd=repo, check=True)
subprocess.run(["cargo", "build", "--offline", "--locked", "--release", "--jobs", "2", "--features", "complete-api", "--target-dir", str(repo / "target"), "--manifest-path", str(output / "Cargo.toml")], cwd=repo, check=True)
print(repo / "target/release" / name)
