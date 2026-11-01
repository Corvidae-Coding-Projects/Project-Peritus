#!/usr/bin/env python3
"""Independent decimal-u64 release identity oracle for source-mounted Rust parser."""

import hashlib
import json
from pathlib import Path
import random
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
SOURCE = "crates/app/peritus-launcher/src/update/release/version.rs"
MAX = 2**64 - 1


def expected(tag: str) -> str | None:
    if not tag.startswith("v"):
        return None
    fields = tag[1:].split(".")
    if len(fields) != 3 or any(not field or not field.isascii() or not field.isdigit() for field in fields):
        return None
    numbers = [int(field) for field in fields]
    return ".".join(map(str, numbers)) if all(number <= MAX for number in numbers) else None


def generated(seed: int, count: int) -> list[str]:
    rng = random.Random(seed)
    components = ["0", "1", "9", "01", "00012", str(MAX), str(MAX + 1), "", "-1", "+1", "１", "1x", " 1", "1 "]
    prefixes = ["v", "v", "v", "V", "", "vv"]
    suffixes = ["", "", "", ".0", "-rc1", "\n"]
    return [rng.choice(prefixes) + ".".join(rng.choice(components) for _ in range(3)) + rng.choice(suffixes) for _ in range(count)]


def main() -> int:
    if len(sys.argv) < 2:
        raise SystemExit("usage: release_version.py PROBE_BINARY [OUTPUT_DIR]")
    probe = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
    fixed = ["v0.0.0", "v1.2.3", "v01.002.0003", f"v{MAX}.0.{MAX}", f"v{MAX + 1}.0.0",
             "1.2.3", "V1.2.3", "v1.2", "v1.2.3.4", "v1..3", "v-1.2.3", "v１.2.3", "v1.2.3-rc1"]
    cases = fixed + generated(7319, 1024)
    first = None
    for index, tag in enumerate(cases):
        result = subprocess.run([str(probe), tag], capture_output=True, text=True, check=False)
        if result.returncode:
            raise RuntimeError(f"probe failed ({result.returncode}): {result.stderr}")
        actual = json.loads(result.stdout)["version"]
        oracle = expected(tag)
        if actual != oracle:
            first = {"index":index,"tag":tag,"expected":oracle,"actual":actual}
            break
    report = {"status":"fail" if first else "pass", "seed":7319,"requested":len(cases),
              "completed":first["index"]+1 if first else len(cases),"failure":first,
              "source":{SOURCE:hashlib.sha256((ROOT / SOURCE).read_bytes()).hexdigest()}}
    if output:
        output.mkdir(parents=True, exist_ok=False)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return int(first is not None)


if __name__ == "__main__":
    raise SystemExit(main())
