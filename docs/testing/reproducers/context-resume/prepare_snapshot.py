#!/usr/bin/env python3
"""Export a copied context checkpoint and uncovered events for public-API replay."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3

parser = argparse.ArgumentParser()
parser.add_argument("case_dir", type=Path, help="contains context/writer/journal.sqlite3 and copied artifacts")
args = parser.parse_args()
case = args.case_dir.resolve()
root = case / "context/writer"
with sqlite3.connect(f"file:{root / 'journal.sqlite3'}?mode=ro", uri=True) as connection:
    row = connection.execute("SELECT value FROM state_records WHERE namespace=3401").fetchone()
    if row is None:
        raise SystemExit("Copied writer context has no published checkpoint")
    checkpoint = json.loads(row[0])
    def artifact(descriptor):
        digest = bytes(descriptor["digest"]).hex()
        data = (root / "artifacts/objects/sha256" / digest[:2] / digest).read_bytes()
        assert len(data) == descriptor["bytes"]
        assert hashlib.sha256(data).hexdigest() == digest
        return data
    sources = json.loads(artifact(checkpoint["source_index"]))
    events = []
    for sequence, frame in connection.execute("SELECT sequence,frame FROM events WHERE sequence>? ORDER BY sequence", (checkpoint["through_event"],)):
        assert frame[:4] == b"PRTS"
        assert int.from_bytes(frame[12:16], "big") == len(frame) - 16
        record = json.loads(frame[16:])
        events.append(record)
        if "observation" in record:
            sources.append(record["observation"]["observation"])
    (case / "working_state.bin").write_bytes(artifact(checkpoint["working_state"]))
    (case / "snapshot.json").write_text(json.dumps({"checkpoint": checkpoint, "events": events, "sources": sources}))
print(json.dumps({"case_dir": str(case), "tail_events": len(events), "sources": len(sources)}))
