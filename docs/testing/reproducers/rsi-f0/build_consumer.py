#!/usr/bin/env python3
"""Build the real public E3/F0 consumer against this checkout, with two jobs."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

p = argparse.ArgumentParser()
p.add_argument("--output", type=Path, required=True, help="Fresh disposable consumer crate directory")
p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[4])
a = p.parse_args()
repo = a.repo.resolve()
out = a.output.resolve()
if out.exists():
    raise SystemExit("Use a fresh output directory; existing artifacts are preserved.")
(out / "src").mkdir(parents=True)
shutil.copyfile(Path(__file__).with_name("consumer.rs"), out / "src" / "main.rs")
shutil.copyfile(repo / "Cargo.lock", out / "Cargo.lock")
workspace = tomllib.loads((repo / "Cargo.toml").read_text())
crates = {
    "peritus-eval": "crates/analysis/peritus-eval",
    "peritus-evolution": "crates/analysis/peritus-evolution",
    "peritus-harness": "crates/orchestration/peritus-harness",
    "peritus-model-protocol": "crates/model/peritus-model-protocol",
    "peritus-artifact-store": "crates/state/peritus-artifact-store",
    "peritus-journal": "crates/state/peritus-journal",
    "peritus-codec": "crates/foundation/peritus-codec",
    "peritus-types": "crates/foundation/peritus-types",
    "peritus-scheduler": "crates/orchestration/peritus-scheduler",
}
manifest = '[package]\nname = "rsi-f0-real-consumer"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\n'
manifest += ''.join(f'{name} = {{ path = {json.dumps(str(repo / path))} }}\n' for name, path in crates.items())
deps = workspace["workspace"]["dependencies"]
serde = deps["serde"]["version"]
serde_json = deps["serde_json"]
manifest += f'serde = {{ version = {json.dumps(serde)}, features = ["derive"] }}\n'
manifest += f'serde_json = {json.dumps(serde_json)}\n\n[profile.release]\n'
for key, value in workspace["profile"]["release"].items():
    manifest += f'{key} = {json.dumps(value)}\n'
(out / "Cargo.toml").write_text(manifest)
env = dict(os.environ, CARGO_TARGET_DIR=str(repo / "target"))
subprocess.run(["cargo", "build", "--offline", "--release", "-j", "2", "--manifest-path", str(out / "Cargo.toml")], env=env, check=True)
binary = repo / "target" / "release" / "rsi-f0-real-consumer"
print(binary)
