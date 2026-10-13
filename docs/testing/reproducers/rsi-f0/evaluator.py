#!/usr/bin/env python3
"""Independent CLI requirements oracle, run only in evaluator stage.

No implementation-output snapshots serve as the oracle. The contract is the
Cargo package version and documented command-line interface.
"""
import json
import sys
from pathlib import Path

contract = json.loads(Path(sys.argv[1]).read_text())
actual = json.loads(Path(sys.argv[2]).read_text())
kind = contract["kind"]
if kind == "version":
    passed = actual["exit"] == 0 and actual["stdout"] == "peritus 0.0.7\n"
elif kind == "help":
    passed = actual["exit"] == 0 and "USAGE:" in actual["stdout"] and "peritus" in actual["stdout"]
elif kind == "bad-argument":
    passed = actual["exit"] != 0 and bool(actual["stderr"].strip())
else:
    raise ValueError("unknown independently defined contract")
print(json.dumps({"kind": kind, "passed": passed, "exit": actual["exit"]}, sort_keys=True))
raise SystemExit(0 if passed else 1)
