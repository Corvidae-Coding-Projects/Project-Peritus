#!/usr/bin/env python3
"""Inspect a real completed no-patch evaluation and replay its refused controls."""
import argparse
import json
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
p.add_argument("--cli", type=Path, required=True)
p.add_argument("--endpoint", required=True)
p.add_argument("--run", required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
if a.output.exists():
    raise SystemExit("Use a fresh output file; retained observations are preserved.")
observations = []

def call(action):
    command = [str(a.cli.resolve()), "--endpoint", a.endpoint, "--json", "runs", action, "--run", a.run]
    result = subprocess.run(command, capture_output=True, text=True, timeout=30)
    value = json.loads(result.stdout if result.stdout.strip() else result.stderr)
    observations.append({"command": command, "exit": result.returncode, "response": value, "stderr": result.stderr})
    a.output.write_text(json.dumps(observations, indent=2))
    return result.returncode, value

status, value = call("show")
assert status == 0, value
run = value["result"]
assert run["phase"] == "Complete", run["status"]
assert run["settlement"]["disposition"] == "CompletedWithoutCandidate", run["settlement"]
assert run["settlement"]["cause"] == "NoChangeComplete", run["settlement"]
assert run["state"] == "Completed without candidate", run["state"]
assert run["deliverable"] is None and run["diff"] == "" and run["review"], run["status"]
assert run["settlement"]["evidence"] is None, run["settlement"]
for control in ("accept", "commit", "export"):
    assert not run["operation"]["legal_controls"][control], run["operation"]
    status, result = call(control)
    assert status == 12 and result["ok"] is False and result["error"]["category"] == "rejected", (control, result)
print("PASS: actual no-patch completion, retained review, and three refused candidate controls")
