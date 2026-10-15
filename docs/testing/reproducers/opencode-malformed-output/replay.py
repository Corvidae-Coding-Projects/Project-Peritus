#!/usr/bin/env python3
"""Build a public consumer and run real inference using disposable host-read fixtures."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", required=True, type=Path)
parser.add_argument("--config", required=True, type=Path)
parser.add_argument("--key-file", required=True, type=Path)
parser.add_argument("--opencode", type=Path, default=shutil.which("opencode"))
parser.add_argument("--model", default="deepseek-v4.1-flash")
parser.add_argument("--service", choices=("zen", "go"), default="zen")
parser.add_argument("--attempts", type=int, default=8)
parser.add_argument("--cancel", action="store_true")
args = parser.parse_args()
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", args.model):
    parser.error("use the bare advertised model ID, without a service prefix")
if args.attempts < 1 or args.opencode is None:
    parser.error("at least one workflow and an installed OpenCode executable are required")
config = args.config.resolve(strict=True)
key_file = args.key_file.resolve(strict=True)
vendor = Path(args.opencode).resolve(strict=True)
here = Path(__file__).resolve().parent
checkout = here.parents[3]
root = args.output.resolve()
root.mkdir(mode=0o700, parents=True, exist_ok=False)
project = root / "consumer"
(project / "src").mkdir(parents=True)
(root / "captures").mkdir()
shutil.copy2(here / "consumer.rs", project / "src/main.rs")
capture = root / "capture-opencode"
shutil.copy2(here / "capture.py", capture)
capture.chmod(0o755)
manifest = '[package]\nname="peritus-opencode-malformed-replay"\nversion="0.1.0"\nedition="2024"\n[workspace]\n[dependencies]\n'
for package, path in (
    ("peritus-daemon", "crates/app/peritus-daemon"),
    ("peritus-model-protocol", "crates/model/peritus-model-protocol"),
    ("peritus-provider-core", "crates/model/peritus-provider-core"),
):
    manifest += package + "={path=" + json.dumps(str(checkout / path)) + "}\n"
manifest += 'tokio={version="=1.53.1",features=["rt","time","macros"]}\nserde_json="=1.0.149"\ntoml="=1.1.4"\n'
(project / "Cargo.toml").write_text(manifest)
shutil.copy2(checkout / "Cargo.lock", project / "Cargo.lock")
subprocess.run([
    "cargo", "build", "--offline", "--jobs", "2", "--manifest-path", str(project / "Cargo.toml"),
    "--target-dir", str(checkout / "target"),
], check=True)
env = os.environ.copy()
env.update(
    PERITUS_REAL_CASE_ROOT=str(root), PERITUS_REAL_CONFIG=str(config),
    PERITUS_REAL_KEY_FILE=str(key_file), PERITUS_REAL_CAPTURE=str(capture),
    PERITUS_REAL_VENDOR_BINARY=str(vendor), PERITUS_REAL_CAPTURE_DIR=str(root / "captures"),
)
command = [str(checkout / "target/debug/peritus-opencode-malformed-replay"), args.model,
           "cancel" if args.cancel else "replay", str(args.attempts), args.service]
with (root / "consumer.log").open("w") as log:
    process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    for line in process.stdout:
        log.write(line)
        log.flush()
        print(line, end="", flush=True)
    code = process.wait()
(root / "result.json").write_text(json.dumps({
    "command": command, "returncode": code,
    "source_commit": subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip(),
}, indent=2) + "\n")
raise SystemExit(code)
