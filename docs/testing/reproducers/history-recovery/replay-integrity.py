#!/usr/bin/env python3
"""Restart an actual disposable daemon against valid, corrupt and missing continuations."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("fixture", type=Path)
p.add_argument("runtime", type=Path, help="disposable XDG config/state/cache parent")
p.add_argument("config", type=Path)
p.add_argument("cli", type=Path)
p.add_argument("daemon", type=Path)
p.add_argument("endpoint")
a = p.parse_args()
saved = (a.fixture / "fixed-saved-run.json").read_bytes()
record = json.loads(saved)
pointer = record["resume_artifact"]
raw = (a.fixture / "fixed-continuation.bin").read_bytes()
if hashlib.sha256(raw).digest() != bytes(pointer["digest"]):
    p.error("fixture continuation does not match saved record")
state = a.runtime / "state/peritus/daemon"
record_path = state / "workbench-v1/runs" / (record["run_id"] + ".json")
artifact = state / "workbench-v1/continuations" / (bytes(pointer["digest"]).hex() + ".bin")
trace = state / "product-runs" / (record["run_id"] + ".trace")
trace_digest = hashlib.sha256(trace.read_bytes()).digest()
env = os.environ.copy()
env.update({"XDG_" + name.upper() + "_HOME": str(a.runtime / name)
            for name in ("config", "state", "cache")})
client = [str(a.cli), "--endpoint", a.endpoint, "--timeout-seconds", "2", "--json"]
results = []
for case in ("valid", "corrupt", "missing"):
    record_path.write_bytes(saved)
    artifact.write_bytes(raw)
    if case == "corrupt":
        damaged = bytearray(raw)
        damaged[100] ^= 1
        artifact.write_bytes(damaged)
    elif case == "missing":
        artifact.unlink()
    unit = "peritus-history-integrity-" + case
    command = ["systemd-run", "--user", "--scope", "--unit=" + unit,
               "--property=CPUQuota=400%", "--property=MemoryHigh=3G",
               "--property=MemoryMax=4G", "--property=TasksMax=192",
               "nice", "-n10", str(a.daemon), "serve", "--config", str(a.config)]
    with (a.fixture / (case + "-daemon.log")).open("w") as log:
        process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                ready = subprocess.run(client + ["status"], env=env, capture_output=True, text=True)
                if ready.returncode == 0:
                    break
                if process.poll() is not None:
                    raise RuntimeError(case + " daemon failed to start")
                time.sleep(0.1)
            else:
                raise RuntimeError(case + " daemon never became ready")
            response = subprocess.run(client + ["runs", "show", "--run", record["run_id"]],
                                      env=env, capture_output=True, text=True)
            (a.fixture / (case + "-show.stdout")).write_text(response.stdout)
            (a.fixture / (case + "-show.stderr")).write_text(response.stderr)
            if case == "valid":
                if response.returncode != 0 or json.loads(response.stdout)["result"]["phase"] != "Cancelled":
                    raise RuntimeError("valid continuation was not reopened")
            else:
                if response.returncode == 0 or record_path.exists():
                    raise RuntimeError(case + " continuation was admitted instead of quarantined")
            if hashlib.sha256(trace.read_bytes()).digest() != trace_digest:
                raise RuntimeError("restart replayed an execution effect")
            group = subprocess.check_output(["systemctl", "--user", "show", unit + ".scope",
                                             "-p", "ControlGroup"], text=True).strip().split("=", 1)[1]
            resource = Path("/sys/fs/cgroup") / group.lstrip("/")
            results.append({"case": case, "status": "PASS", "exit": response.returncode,
                            "memory_peak": int((resource / "memory.peak").read_text()),
                            "memory_events": (resource / "memory.events").read_text()})
        finally:
            shutdown = subprocess.run(client + ["shutdown", "--wait"], env=env, capture_output=True, text=True)
            (a.fixture / (case + "-shutdown.txt")).write_text(shutdown.stdout + shutdown.stderr)
            if shutdown.returncode != 0:
                subprocess.run(["systemctl", "--user", "stop", unit + ".scope"], check=False)
            process.wait(timeout=20)
    print("PASS:", case, "continuation; no automatic effect replay", flush=True)
# Preserve the good pointer and bytes for later manual replay.
artifact.write_bytes(raw)
record_path.write_bytes(saved)
(a.fixture / "integrity-result.json").write_text(json.dumps(results, indent=2) + "\n")
