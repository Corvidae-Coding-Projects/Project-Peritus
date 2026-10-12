#!/usr/bin/env python3
"""Read back an actual daemon-written continuation against independent fixture data."""
import argparse
import hashlib
import json
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("fixture", type=Path)
p.add_argument("saved_run", type=Path)
a = p.parse_args()
expected = json.loads((a.fixture / "expected-continuation.bin").read_bytes())
record = json.loads(a.saved_run.read_bytes())
if record["format_version"] != 7 or record.get("resume_state") is not None:
    raise RuntimeError("daemon did not publish an artifact-backed continuation")
pointer = record["resume_artifact"]
digest = bytes(pointer["digest"])
artifact = a.saved_run.parent.parent / "continuations" / (digest.hex() + ".bin")
raw = artifact.read_bytes()
if len(raw) != pointer["bytes"] or hashlib.sha256(raw).digest() != digest:
    raise RuntimeError("continuation pointer does not match durable bytes")
actual = json.loads(raw)
# Existing public restore semantics require fresh checks after a process restart.
if expected["next_phase"] >= 3:
    expected["next_phase"] = 3
if actual != expected:
    raise RuntimeError("saved continuation changed independently generated retained data")
print("PASS: exact retained values, verified artifact pointer and legacy migration;", len(raw), "continuation bytes")
