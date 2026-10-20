#!/usr/bin/env python3
"""Link the real API consumer against one freshly built Cargo artifact cohort."""
import argparse
import json
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument("--artifacts", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--source", type=Path, default=Path(__file__).with_name("consumer.rs"))
a = p.parse_args()
libraries = {}
for line in a.artifacts.read_text().splitlines():
    row = json.loads(line)
    if row.get("reason") == "compiler-artifact":
        for name in row["filenames"]:
            if name.endswith(".rlib"):
                libraries[row["target"]["name"]] = name
deps = Path(libraries["peritus_model_protocol"]).parent
command = ["rustc", "--edition=2024", "-C", "debuginfo=0", str(a.source), "-L", "dependency=" + str(deps), "-o", str(a.output)]
for name in ["peritus_daemon", "peritus_product_runner", "peritus_model_protocol", "peritus_provider_core", "serde_json", "tokio"]:
    command.extend(["--extern", name + "=" + libraries[name]])
for native in deps.parent.glob("build/*/out"):
    if any(native.glob("*.a")):
        command.extend(["-L", "native=" + str(native)])
subprocess.run(command, check=True)
