#!/usr/bin/env python3
"""Generate a large legal history from an actual disposable run and its control store."""
import argparse
import hashlib
import json
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("saved_run", type=Path)
p.add_argument("output", type=Path)
p.add_argument("--filler-bytes", type=int, default=3 * 1024 * 1024)
a = p.parse_args()
if not 0 < a.filler_bytes <= 4 * 1024 * 1024:
    p.error("filler-bytes must be within 1..4194304")
record = json.loads(a.saved_run.read_bytes())
if record["format_version"] != 6 or not record.get("resume_state"):
    p.error("use an actual version-six disposable run with a retained continuation")
resume = json.loads(bytes(record["resume_state"]))
line = "Generated retained historical observation for persistence replay. No new action is authorized by this stored text.\n"
filler = (line * (a.filler_bytes // len(line) + 1))[:a.filler_bytes]
resume["developer_evidence"] += "\nHISTORY_FIRST_58d97ac0\n" + filler + "\nHISTORY_LAST_a06fb325\n"
payload = json.dumps(resume, ensure_ascii=False, separators=(",", ":")).encode()
record.update(resume_state=list(payload), phase=11, status="Fixture waiting for explicit input", user_cancelled=False)
compact = json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode()
pretty = json.dumps(record, ensure_ascii=False, indent=2).encode()
if not len(compact) < 16 * 1024 * 1024 < len(pretty):
    p.error("this fixture does not straddle the old save boundary; adjust filler-bytes")
a.output.mkdir(parents=True, exist_ok=True)
(a.output / "large-legacy-run.json").write_bytes(compact)
(a.output / "expected-continuation.bin").write_bytes(payload)
(a.output / "oracle.json").write_text(json.dumps({
    "run": record["run_id"], "legacy_compact_bytes": len(compact),
    "legacy_pretty_bytes": len(pretty), "continuation_bytes": len(payload),
    "developer_evidence_sha256": hashlib.sha256(resume["developer_evidence"].encode()).hexdigest(),
}, indent=2) + "\n")
print("Prepared disposable run", record["run_id"], "with compact/pretty bytes", len(compact), len(pretty))
print("Use only the matching disposable control store; no application component is substituted.")
