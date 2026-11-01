#!/usr/bin/env python3
"""Calibrate the Unix bootstrap oracle against a temporary checksum-bypass mutant."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CASE = HERE.parent / "cases/stage7-bootstrap-checksum-mismatch.json"
TARGET = "[ \"$actual\" = \"$expected\" ] || fail \"release archive checksum did not match\""


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(source: Path, output: Path) -> tuple[int, dict[str, object]]:
    process = subprocess.run(
        [sys.executable, str(HERE / "stage7_bootstrap.py"), "--replay", str(CASE),
         "--source-root", str(source), "--output", str(output)],
        capture_output=True, text=True, timeout=90, check=False,
    )
    report = json.loads((output / "report.json").read_text())
    return process.returncode, report


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: calibrate_stage7_bootstrap.py <new-output-directory>")
    destination = Path(sys.argv[1]).resolve()
    destination.mkdir(parents=True, exist_ok=False)
    original = ROOT / "install.sh"
    original_hash = sha(original)
    baseline_code, baseline = run(ROOT, destination / "baseline")
    with tempfile.TemporaryDirectory(prefix="peritus-stage7-bootstrap-mutant-") as temporary:
        mutant = Path(temporary)
        shutil.copytree(ROOT / "packaging/linux", mutant / "packaging/linux")
        text = original.read_text()
        if text.count(TARGET) != 1:
            raise RuntimeError("checksum guard changed; calibration cannot target its intended branch")
        (mutant / "install.sh").write_text(text.replace(TARGET, ": # deliberate checksum-bypass mutant"))
        mutant_code, mutated = run(mutant, destination / "mutant")
        mutant_hash = sha(mutant / "install.sh")
    restored = sha(original) == original_hash
    success = (baseline_code == 0 and baseline["status"] == "pass"
               and mutant_code == 1 and mutated["status"] == "fail"
               and mutated["completed_cases"] == 1 and restored)
    result = {"status": "pass" if success else "fail", "source_sha256": original_hash,
              "mutant_sha256": mutant_hash, "baseline_exit": baseline_code,
              "mutant_exit": mutant_code, "source_restored": restored,
              "witness": str(CASE.relative_to(ROOT)),
              "mutation": "skip release archive digest comparison in a temporary copy"}
    (destination / "report.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, sort_keys=True))
    return int(not success)


if __name__ == "__main__":
    raise SystemExit(main())
