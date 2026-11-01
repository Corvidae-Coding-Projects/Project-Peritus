#!/usr/bin/env python3
"""Exercise malformed web subcommands through the real CLI without network access."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


INVALID = (
    ("search_missing_query", ("search",)),
    ("open_missing_url", ("open",)),
    ("read_missing_source", ("read",)),
    ("find_missing_source", ("find",)),
    ("find_missing_query", ("find", "source")),
    ("open_invalid_flag", ("open", "https://example.invalid/", "--wrong")),
    ("read_invalid_offset", ("read", "source", "not-a-number")),
    ("find_invalid_offset", ("find", "source", "query", "not-a-number")),
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    if output.exists():
        parser.error(f"output already exists: {output}")
    output.mkdir(parents=True)
    source = Path(__file__).resolve().parents[2] / "crates/app/peritus-cli/src/web.rs"
    runs = []
    for name, operands in INVALID:
        for mode in ("disposable_home", "missing_home"):
            with tempfile.TemporaryDirectory(prefix="peritus-cli-stage7-") as directory:
                root = Path(directory)
                env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}
                if mode == "disposable_home":
                    home = root / "home"
                    home.mkdir()
                    data = root / "data"
                    env.update({"HOME": str(home), "XDG_DATA_HOME": str(data)})
                else:
                    data = root / "data"
                command = [str(binary), "--json", "web", *operands]
                completed = subprocess.run(command, env=env, capture_output=True, text=True, timeout=10)
                try:
                    payload = json.loads(completed.stderr)
                    category = payload["error"]["category"]
                except (ValueError, KeyError, TypeError):
                    category = None
                created_data = data.exists()
                runs.append(
                    {
                        "name": name,
                        "mode": mode,
                        "arguments": list(operands),
                        "exit_code": completed.returncode,
                        "category": category,
                        "stdout": completed.stdout,
                        "stderr": completed.stderr,
                        "created_data": created_data,
                        "pass": completed.returncode == 2 and category == "usage" and not created_data,
                    }
                )
    report = {
        "binary": str(binary),
        "binary_sha256": sha256(binary),
        "source": str(source),
        "source_sha256": sha256(source),
        "cases": runs,
        "passes": sum(row["pass"] for row in runs),
        "failures": sum(not row["pass"] for row in runs),
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passes": report["passes"], "failures": report["failures"], "report": str(output / "report.json")}))
    return 0 if report["failures"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
