#!/usr/bin/env python3
"""Replay the real E3-to-daemon candidate scheduling failure.

This starts actual CLI processes, actual evaluators, an actual daemon, a real
SQLite journal, and the content-addressed artifact store. It supplies no fake
schedule acknowledgements, published evaluation evidence, or promotions.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument("--consumer", type=Path, required=True)
p.add_argument("--baseline", type=Path, required=True)
p.add_argument("--candidate", type=Path, required=True)
p.add_argument("--daemon", type=Path, required=True)
p.add_argument("--cli", type=Path, required=True)
p.add_argument("--approval-registry", type=Path, required=True)
p.add_argument("--root", type=Path, required=True)
a = p.parse_args()
root = a.root.resolve()
if root.exists():
    raise SystemExit("Use a fresh replay root; existing evidence is preserved.")
root.mkdir(parents=True, mode=0o700)
registry = root / "approval-registry.bin"
shutil.copyfile(a.approval_registry, registry)
registry.chmod(0o600)
paths = {"state_root": root}
for field, name in [("artifact_root", "artifacts"), ("evidence_root", "evidence"),
                    ("workspace_root", "workspaces"), ("process_root", "processes"),
                    ("transaction_root", "transactions"), ("backup_root", "backups")]:
    paths[field] = root / name
config = 'version = 1\nstore_id = "' + 'dd' * 16 + '"\n\n[paths]\n'
config += ''.join(f'{field} = "{path}"\n' for field, path in paths.items())
config += (f'\n[approval_registry]\npayload_file = "{registry}"\ngeneration = 1\n'
           '\n[human]\nactor_id = "4fb7c7b74d30ab95665b8d7d9dcd7c25"\n'
           '\n[product]\nautomatic_provider_failover = false\n'
           '\n[tools]\nallow = []\n\n[telemetry]\nmode = "disabled"\n')
(root / "daemon.toml").write_text(config)
env = dict(os.environ, EXPECT_VERSION_FIX="1")
with (root / "consumer.stdout").open("wb") as out, (root / "consumer.stderr").open("wb") as err:
    subprocess.run([str(a.consumer), str(a.baseline), str(a.candidate), str(root),
                    str(Path(__file__).with_name("evaluator.py"))], env=env,
                   stdout=out, stderr=err, check=True, timeout=30)
hashes = {str(f): hashlib.sha256(f.read_bytes()).hexdigest()
          for f in (a.consumer, a.baseline, a.candidate, a.daemon, a.cli)}
(root / "executables.json").write_text(json.dumps(hashes, indent=2))
out = (root / "daemon.stdout").open("wb")
err = (root / "daemon.stderr").open("wb")
daemon = subprocess.Popen([str(a.daemon), "serve", "--config", str(root / "daemon.toml")],
                          stdout=out, stderr=err)
endpoint = None
status = None
shutdown = None
try:
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if daemon.poll() is not None:
            raise RuntimeError(f"owned daemon exited before readiness: {daemon.returncode}")
        instance = root / "daemon.instance"
        if instance.exists():
            values = dict(line.split("=", 1) for line in instance.read_text().splitlines())
            full = str(root / (values["endpoint"] + ".sock"))
            endpoint = full if len(full.encode()) < 108 else (
                "/tmp/peritus-" + hashlib.sha256(
                    b"peritus/unix-runtime-endpoint/v1\0" + full.encode()).hexdigest()[:32] + "/daemon.sock")
            s = subprocess.run([str(a.cli), "--endpoint", endpoint, "--timeout-seconds", "2", "--json", "status"],
                               capture_output=True, text=True, timeout=4)
            if s.returncode == 0:
                status = json.loads(s.stdout)
                if not status["result"]["mutation_ready"]:
                    break
        time.sleep(.1)
    if status is None:
        raise RuntimeError("No real daemon status response was obtained")
    (root / "status.json").write_text(json.dumps(status, indent=2))
    with sqlite3.connect(f"file:{root / 'peritus.sqlite3'}?mode=ro", uri=True) as db:
        rows = db.execute("select destination,state,attempts,fence from outbox").fetchall()
    (root / "outbox.json").write_text(json.dumps(rows, indent=2))
    ready = status["result"]["mutation_ready"]
    settled = rows and all(row[1] == 3 for row in rows)
    failure = not ready or not settled
    print(json.dumps({"result": "FAIL" if failure else "PASS", "pid": daemon.pid,
                      "readiness": status["result"]["readiness"], "diagnostic": status["result"]["diagnostic"],
                      "outbox": rows, "evidence": str(root)}, indent=2))
finally:
    if endpoint and daemon.poll() is None:
        shutdown = subprocess.run([str(a.cli), "--endpoint", endpoint, "--timeout-seconds", "5", "--json", "shutdown", "--wait"],
                                  capture_output=True, text=True, timeout=10)
        (root / "shutdown.json").write_text(json.dumps({"exit": shutdown.returncode,
                                                        "stdout": shutdown.stdout, "stderr": shutdown.stderr}, indent=2))
    try:
        daemon.wait(timeout=10)
    except subprocess.TimeoutExpired:
        daemon.terminate()
        daemon.wait(timeout=5)
    out.close()
    err.close()
    (root / "daemon-exit.json").write_text(json.dumps({"pid": daemon.pid, "exit": daemon.returncode}))
raise SystemExit(1 if failure else 0)
