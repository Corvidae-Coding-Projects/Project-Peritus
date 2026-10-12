#!/usr/bin/env python3
"""Check real reader observations, exact retrieval ranges and independent marker receipts."""
import argparse
import hashlib
import json
import struct
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("document", type=Path)
p.add_argument("oracle", type=Path)
p.add_argument("saved_run", type=Path)
p.add_argument("trace", type=Path)
a = p.parse_args()
run = json.loads(a.saved_run.read_bytes())
expected = json.loads(a.oracle.read_text())["files"][a.document.name]["markers"]
observations = []
with a.trace.open("rb") as trace:
    while header := trace.read(9):
        if len(header) != 9:
            raise RuntimeError("torn real trace header")
        tag, size = header[0], struct.unpack_from("<Q", header, 1)[0]
        payload = trace.read(size)
        if len(payload) != size:
            raise RuntimeError("torn real trace payload")
        if tag in (2, 7):
            observations.append(json.loads(payload))
reads = [o for o in observations if o["name"] == "workspace_read" and
         json.loads(o["arguments"]).get("path") == a.document.name]
if len(reads) != 1 or reads[0]["is_error"]:
    raise RuntimeError("expected exactly one successful real workspace read")
original = reads[0]["output"].encode()
output = json.loads(original)
lines = a.document.read_text().splitlines()
if output["content"] != "\n".join(f"{i}: {line}" for i, line in enumerate(lines, 1)):
    raise RuntimeError("host read does not match independently read source file")
if output["next_line"] is not None or output["end_line"] != len(lines):
    raise RuntimeError("host read did not reach actual EOF")
pages = 0
for observation in observations:
    if observation["name"] != "context_read" or observation["is_error"]:
        continue
    for source in json.loads(observation["output"]).get("sources", []):
        if source["sha256"] != hashlib.sha256(original).hexdigest():
            continue
        data = source["data"].encode() if source["encoding"] == "utf8" else bytes.fromhex(source["data"])
        start = source["offset"]
        if data != original[start:start + len(data)]:
            raise RuntimeError("retrieved bytes differ from the original durable observation")
        pages += 1
answer = json.loads(next(item[2] for item in reversed(run["interaction"]["activities"])
                         if item[2].startswith('{"FIRST_MARKER"')))
if run["phase"] != 11 or not pages or not answer.get("provider_limited_view_used"):
    raise RuntimeError("oversized-reader workflow did not complete using real archived evidence")
for name, value in expected.items():
    if answer[name.upper() + "_MARKER"] != value:
        raise RuntimeError("model marker differs from independent source oracle")
print("PASS: one complete host read,", pages, "exact retrieval pages, and independent model markers")
