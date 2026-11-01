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
        directories = [root / f"bin {i} é" for i in range(len(states))]
        for directory in directories:
            directory.mkdir()
        for directory, state in zip(directories, states):
            if state == "absent":
                continue
            path = directory / name
            if state == "directory":
                path.mkdir()
                continue
            if state == "dangling":
                path.symlink_to(directory / "missing-target")
                continue
            target = directory / "actual executable" if state.startswith("symlink_") else path
            target.write_text("#!/bin/sh\nexit 0\n")
            target.chmod(0o755 if state in ("valid", "symlink_valid") else 0o644)
            if target != path:
                path.symlink_to(target.name)
        home = root / "home"
        home.mkdir()
        env = os.environ.copy()
        env.update(PATH=os.pathsep.join(map(str, directories)), HOME=str(home))
        execution = subprocess.run([str(probe), kind], env=env, text=True, capture_output=True, check=False, timeout=10)
        if execution.returncode:
            raise RuntimeError(f"probe failed ({execution.returncode}): {execution.stderr}")
        observed = json.loads(execution.stdout)
        expected = next((str((directory / name).resolve()) for directory, state in zip(directories, states) if state in ("valid", "symlink_valid")), None)
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
    classes = ("absent", "invalid", "valid", "directory", "dangling", "symlink_valid", "symlink_invalid")
    # Exhaust the small interaction space; deeper histories separately vary growth and shadowing.
    fixed += list(itertools.product(classes, repeat=2))
    generated = [tuple(rng.choice(classes) for _ in range(rng.randrange(7))) for _ in range(256)]
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
    minimized = None
    if first:
        # Preserve the actual failure family; remove entries only while disagreement remains.
        states = tuple(first["states"])
        for index in range(len(states) - 1, -1, -1):
            shorter = states[:index] + states[index + 1:]
            candidate = run_case(probe, first["kind"], shorter, -1)
            if not candidate["valid"]:
                states = shorter
        minimized = run_case(probe, first["kind"], states, -1)
    report = {
        "status": "fail" if first else "pass", "seed": 7319,
        "requested": len(cases), "completed": completed,
        "source": {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in SOURCES},
        "path_state_counts": {state: sum(states.count(state) for _, states in cases) for state in classes},
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
