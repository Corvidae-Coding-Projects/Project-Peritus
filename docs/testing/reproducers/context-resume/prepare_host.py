#!/usr/bin/env python3
"""Prepare an isolated host replay from the already copied context and non-secret profile."""
import argparse
import json
import os
from pathlib import Path
import shutil
import sqlite3
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument("case_dir", type=Path)
parser.add_argument("host_dir", type=Path)
parser.add_argument("--config", type=Path, required=True, help="actual daemon TOML; only provider profile/executable are retained")
args = parser.parse_args()
source = args.case_dir.resolve() / "context/writer"
host = args.host_dir.resolve()
root = host / "replay.context/writer"
if root.exists():
    raise SystemExit("Use a fresh host_dir; this replay deliberately advances copied durable state")
root.mkdir(parents=True)
with sqlite3.connect(f"file:{source / 'journal.sqlite3'}?mode=ro", uri=True) as original, sqlite3.connect(root / "journal.sqlite3") as copy:
    original.backup(copy)
try:
    shutil.copytree(source / "artifacts", root / "artifacts", copy_function=os.link)
except OSError:
    shutil.rmtree(root / "artifacts", ignore_errors=True)
    shutil.copytree(source / "artifacts", root / "artifacts")
(host / "workspace").mkdir()
config = tomllib.loads(args.config.read_text())
settings = next(provider for provider in config["providers"] if provider.get("profile", {}).get("profile_id") == "a1000000000000000000000000000001")
if settings["kind"] != "codex-runtime":
    raise SystemExit("The retained failure uses a real codex-runtime profile")
(host / "provider.json").write_text(json.dumps({"kind": settings["kind"], "executable": settings["executable"], "profile": settings["profile"]}))
print(json.dumps({"host_dir": str(host), "context_root": str(root), "provider_requests": 0}))
