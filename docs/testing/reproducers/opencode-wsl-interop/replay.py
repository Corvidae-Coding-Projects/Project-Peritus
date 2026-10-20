#!/usr/bin/env python3
"""Build a public-API consumer and check which OpenCode runtime Peritus pins."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", required=True, type=Path)
args = parser.parse_args()
here = Path(__file__).resolve().parent
checkout = here.parents[3]
root = args.output.resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=False)
project = root / "consumer"
(project / "src").mkdir(parents=True)
shutil.copy2(here / "consumer.rs", project / "src/main.rs")
manifest = (
    '[package]\nname="peritus-opencode-wsl-interop-replay"\nversion="0.1.0"\n'
    'edition="2024"\n[workspace]\n[dependencies]\n'
)
for package, path in (
    ("peritus-provider-core", "crates/model/peritus-provider-core"),
    ("peritus-provider-opencode", "crates/model/peritus-provider-opencode"),
):
    manifest += package + "={path=" + json.dumps(str(checkout / path)) + "}\n"
manifest += 'tokio={version="=1.53.1",features=["rt","time","macros"]}\n'
(project / "Cargo.toml").write_text(manifest)
shutil.copy2(checkout / "Cargo.lock", project / "Cargo.lock")
subprocess.run(
    [
        "cargo", "build", "--offline", "--jobs", "2",
        "--manifest-path", str(project / "Cargo.toml"),
        "--target-dir", str(checkout / "target"),
    ],
    check=True,
)
command = [str(checkout / "target/debug/peritus-opencode-wsl-interop-replay")]
with (root / "consumer.log").open("w") as log:
    process = subprocess.Popen(
        command, env=os.environ.copy(), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    for line in process.stdout:
        log.write(line)
        log.flush()
        print(line, end="", flush=True)
    code = process.wait()
(root / "result.json").write_text(json.dumps({
    "command": command,
    "returncode": code,
    "source_commit": subprocess.check_output(
        ["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True
    ).strip(),
}, indent=2) + "\n")
raise SystemExit(code)
