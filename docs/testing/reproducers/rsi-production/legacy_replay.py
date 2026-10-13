#!/usr/bin/env python3
"""Reopen a copied historical failed E3 schedule with the actual daemon and CLI.

The source fixture must be closed. This never edits it, guesses Debug text into a
profile, or manufactures a scheduler acknowledgement. Only its own daemon is stopped.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--root", type=Path, required=True)
parser.add_argument("--daemon", type=Path, required=True)
parser.add_argument("--cli", type=Path, required=True)
args = parser.parse_args()
source, root = args.source.resolve(), args.root.resolve()
if root.exists():
    raise SystemExit("Use a fresh root; retained evidence is preserved.")
root.mkdir(mode=0o700, parents=True)
for name in ("artifacts", "evidence", "workspaces", "processes", "transactions", "backups"):
    if (source / name).exists():
        shutil.copytree(source / name, root / name)
shutil.copyfile(source / "approval-registry.bin", root / "approval-registry.bin")
(root / "approval-registry.bin").chmod(0o600)
with sqlite3.connect(f"file:{source}/peritus.sqlite3?mode=ro", uri=True) as old:
    with sqlite3.connect(root / "peritus.sqlite3") as copied:
        old.backup(copied)
config = (source / "daemon.toml").read_text().replace(str(source), str(root))
(root / "daemon.toml").write_text(config)
daemon = None
endpoint = None
report = {}
with (root / "daemon.stdout").open("wb") as out, (root / "daemon.stderr").open("wb") as err:
    daemon = subprocess.Popen([str(args.daemon.resolve()), "serve", "--config",
                               str(root / "daemon.toml")], stdout=out, stderr=err)
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if daemon.poll() is not None:
                raise RuntimeError(f"owned daemon exited before readiness: {daemon.returncode}")
            instance = root / "daemon.instance"
            if instance.exists():
                values = dict(line.split("=", 1) for line in instance.read_text().splitlines())
                full = str(root / (values["endpoint"] + ".sock"))
                endpoint = full if len(full.encode()) < 108 else (
                    "/tmp/peritus-" + hashlib.sha256(
                        b"peritus/unix-runtime-endpoint/v1\0" + full.encode()).hexdigest()[:32]
                    + "/daemon.sock")
                result = subprocess.run([str(args.cli.resolve()), "--endpoint", endpoint,
                    "--timeout-seconds", "2", "--json", "status"], capture_output=True,
                    text=True, timeout=5)
                if result.returncode == 0:
                    status = json.loads(result.stdout)
                    with sqlite3.connect(f"file:{root}/peritus.sqlite3?mode=ro", uri=True) as db:
                        rows = db.execute("SELECT destination,state,attempts,fence FROM outbox").fetchall()
                    report.update(status=status, outbox=rows)
                    if rows and all(row[1] == 3 for row in rows):
                        break
            time.sleep(0.1)
        if not report or not report["status"]["result"]["mutation_ready"]:
            raise RuntimeError("actual daemon is not writable")
        if not report["outbox"] or any(row[1] != 3 for row in report["outbox"]):
            raise RuntimeError("historical schedule has not actually settled")
    finally:
        if endpoint and daemon.poll() is None:
            try:
                result = subprocess.run([str(args.cli.resolve()), "--endpoint", endpoint,
                    "--timeout-seconds", "5", "--json", "shutdown", "--wait"],
                    capture_output=True, text=True, timeout=12)
                report.update(shutdown_exit=result.returncode,
                    shutdown_messages=[json.loads(line) for line in result.stdout.splitlines()
                                       if line.strip()], shutdown_error=result.stderr)
            except (subprocess.TimeoutExpired, json.JSONDecodeError) as error:
                report["shutdown_error"] = str(error)
        try:
            daemon.wait(timeout=10)
        except subprocess.TimeoutExpired:
            daemon.terminate()
            daemon.wait(timeout=5)
        report["daemon_exit"] = daemon.returncode
        (root / "actual-legacy-replay.json").write_text(json.dumps(report, indent=2))
if report.get("shutdown_exit") != 0 or report["daemon_exit"] != 0:
    raise SystemExit("actual graceful shutdown failed; evidence retained")
print(json.dumps(report, indent=2))
