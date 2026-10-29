#!/usr/bin/env python3
"""Build the actual ProductRunner/provider consumer outside workspace membership."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
here = Path(__file__).resolve().parent
repo = here.parents[3]
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=False)
(output / "src").mkdir()
shutil.copy2(here / "pipeline-handoff.rs", output / "src/main.rs")
manifest = '[package]\nname="peritus-pipeline-handoff-replay"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n'
for package, layer in (
    ("peritus-product-runner", "app"),
    ("peritus-provider-core", "model"),
    ("peritus-provider-openai", "model"),
    ("peritus-provider-opencode", "model"),
    ("peritus-model-protocol", "model"),
    ("peritus-process", "runtime"),
    ("peritus-types", "foundation"),
):
    manifest += f'{package}={{path={json.dumps(str(repo / "crates" / layer / package))}}}\n'
manifest += 'serde_json="=1.0.149"\ntokio={version="=1.53.1",features=["rt-multi-thread"]}\n[profile.dev]\ndebug=0\n'
(output / "Cargo.toml").write_text(manifest)
shutil.copy2(repo / "Cargo.lock", output / "Cargo.lock")
target = Path(os.environ.get("CARGO_TARGET_DIR", output / "target")).resolve()
subprocess.run([
    "cargo", "build", "--offline", "--jobs", "2", "--manifest-path", str(output / "Cargo.toml"),
    "--target-dir", str(target),
], check=True, cwd=repo)
print(target / "debug/peritus-pipeline-handoff-replay")
