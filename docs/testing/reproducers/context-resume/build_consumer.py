#!/usr/bin/env python3
"""Build the public context consumer against this checkout, without test suites."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("build_dir", type=Path)
parser.add_argument("--target-dir", type=Path)
parser.add_argument("--host", action="store_true", help="build the actual durable host/adapter consumer")
args = parser.parse_args()
repo = Path(__file__).resolve().parents[4]
build = args.build_dir.resolve()
(build / "src").mkdir(parents=True, exist_ok=True)
source_name = "host_consumer.rs" if args.host else "consumer.rs"
shutil.copy2(Path(__file__).with_name(source_name), build / "src/main.rs")
shutil.copy2(repo / "Cargo.lock", build / "Cargo.lock")
crates = {
    "peritus-context": "crates/orchestration/peritus-context",
    "peritus-codec": "crates/foundation/peritus-codec",
    "peritus-types": "crates/foundation/peritus-types",
    "peritus-role": "crates/orchestration/peritus-role",
    "peritus-model-protocol": "crates/model/peritus-model-protocol",
    "peritus-product-runner": "crates/app/peritus-product-runner",
    "peritus-agent": "crates/orchestration/peritus-agent",
}
consumer_name = "peritus-context-host-consumer" if args.host else "peritus-context-resume-consumer"
manifest = f'[package]\nname = "{consumer_name}"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\n'
if args.host:
    crates.update({
        "peritus-provider-openai": "crates/model/peritus-provider-openai",
        "peritus-provider-core": "crates/model/peritus-provider-core",
        "peritus-process": "crates/runtime/peritus-process",
    })
for name, path in crates.items():
    manifest += f'{name} = {{ path = "{repo / path}" }}\n'
manifest += 'serde_json = "=1.0.149"\n'
(build / "Cargo.toml").write_text(manifest)
env = dict(os.environ)
env["CARGO_TARGET_DIR"] = str((args.target_dir or repo / "target").resolve())
subprocess.run(["cargo", "build", "--offline", "--manifest-path", str(build / "Cargo.toml"), "-j2"], env=env, check=True)
print(Path(env["CARGO_TARGET_DIR"]) / f"debug/{consumer_name}")
