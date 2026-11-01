#!/usr/bin/env python3
"""Deterministic public-interface executable discovery campaign."""

import hashlib
import itertools
import json
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCES = [
    "crates/model/peritus-provider-openai/src/runtime/executable.rs",
    "crates/model/peritus-provider-anthropic/src/runtime/executable.rs",
    "crates/model/peritus-provider-core/src/process.rs",
    "crates/app/peritus-provider-onboarding/src/account.rs",
]


def run_case(probe: Path, kind: str, states: tuple[str, ...], index: int) -> dict:
    name = {"codex": "codex", "claude": "claude"}[kind]
    with tempfile.TemporaryDirectory(prefix="peritus-stage7-provider-") as raw:
        root = Path(raw)
        directories = [root / f"bin{i}" for i in range(len(states))]
        for directory in directories:
            directory.mkdir()
        for directory, state in zip(directories, states):
            if state == "absent":
                continue
            path = directory / name
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o755 if state == "valid" else 0o644)
        home = root / "home"
        home.mkdir()
        env = os.environ.copy()
        env.update(PATH=os.pathsep.join(map(str, directories)), HOME=str(home))
        execution = subprocess.run([str(probe), kind], env=env, text=True, capture_output=True, check=False)
        if execution.returncode:
            raise RuntimeError(f"probe failed ({execution.returncode}): {execution.stderr}")
        observed = json.loads(execution.stdout)
        expected = next((str((directory / name).resolve()) for directory, state in zip(directories, states) if state == "valid"), None)
        actual_raw = observed["raw"]
        actual_onboarding = observed["onboarding"]
        # Normalize temp roots so the retained case is path independent.
        normalized = lambda value: value.replace(str(root), "$CASE") if value else None
        return {
            "kind": kind, "states": states, "index": index,
            "expected": normalized(expected),
            "raw": normalized(actual_raw),
            "onboarding": normalized(actual_onboarding),
            "valid": expected == actual_raw == actual_onboarding,
        }


def main() -> int:
    if len(sys.argv) < 2:
        raise SystemExit("usage: provider_discovery.py PROBE_BINARY [OUTPUT_DIR]")
    probe = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    rng = random.Random(7319)
    fixed = [("valid",), ("invalid", "valid"), ("absent", "valid"),
             ("invalid", "absent"), ("valid", "valid")]
    generated = [tuple(rng.choice(("absent", "invalid", "valid")) for _ in range(3)) for _ in range(256)]
    cases = [(kind, states) for kind in ("codex", "claude") for states in fixed + generated]
    first = None
    failures = 0
    by_kind = {}
    completed = 0
    for index, (kind, states) in enumerate(cases):
        observed = run_case(probe, kind, states, index)
        completed += 1
        if not observed["valid"]:
            failures += 1
            first = first or observed
            by_kind.setdefault(kind, observed)
    minimized = run_case(probe, first["kind"], ("invalid", "valid"), -1) if first else None
    report = {
        "status": "fail" if first else "pass", "seed": 7319,
        "requested": len(cases), "completed": completed,
        "source": {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in SOURCES},
        "failures": failures, "first_by_kind": by_kind,
        "first": first, "minimized": minimized,
    }
    if output:
        output.mkdir(parents=True, exist_ok=False)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return int(first is not None)


if __name__ == "__main__":
    raise SystemExit(main())
