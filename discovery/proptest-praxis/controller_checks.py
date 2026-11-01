#!/usr/bin/env python3
"""Exercise owner-controller classification with actual disposable child processes."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import owner


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(mode=0o700)
    rows = []
    cases = (
        ("success", "pass", "raise SystemExit(0)", 0),
        ("ordinary-rust-failure", "fail", "raise SystemExit(101)", 1),
        ("pass-then-abnormal-exit", "pass", "raise SystemExit(7)", 2),
        ("pass-then-signal", "pass", "os.kill(os.getpid(), signal.SIGTERM)", 2),
        ("pass-then-timeout", "pass", "time.sleep(60)", 2),
        ("failure-report-with-success-exit", "fail", "raise SystemExit(0)", 2),
        ("no-report-with-success-exit", None, "raise SystemExit(0)", 2),
    )
    with tempfile.TemporaryDirectory(prefix="peritus-controller-") as temporary:
        scratch = Path(temporary)
        for name, reported, terminal, expected in cases:
            executable = scratch / name
            source = (
                f"#!{sys.executable}\n"
                "import json, os, signal, time\nfrom pathlib import Path\n"
                "folder=Path(os.environ['PRAXIS_OUTPUT']);folder.mkdir()\n"
            )
            if reported is not None:
                source += f"(folder/'report.json').write_text(json.dumps({{'status':{reported!r}}}))\n"
            source += terminal + "\n"
            executable.write_text(source)
            executable.chmod(0o700)
            receipt = scratch / (name + ".json")
            receipt.write_text(json.dumps({
                "package": "peritus-product-runner", "inputs": owner.snapshot(),
                "executable": str(executable),
                "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            }))
            destination = args.output / name
            completed = subprocess.run([
                sys.executable, str(HERE / "owner.py"), "examples", "--target", "inputs",
                "--receipt", str(receipt), "--timeout", "1", "--output", str(destination),
            ], cwd=ROOT, capture_output=True, text=True, timeout=15, check=False)
            (args.output / (name + ".stdout.log")).write_text(completed.stdout)
            (args.output / (name + ".stderr.log")).write_text(completed.stderr)
            native = json.loads((destination / "inputs/process.json").read_text())
            summary = json.loads((destination / "summary.json").read_text())
            expected_status = {0: "pass", 1: "fail", 2: "incomplete"}[expected]
            if completed.returncode != expected or summary["status"] != expected_status:
                raise ValueError(f"{name}: expected {expected_status}, got {completed.returncode}/{summary['status']}")
            if not native["owned_direct_child_reaped"]:
                raise ValueError(f"{name}: native child not reaped")
            rows.append({"name": name, "expected_controller_exit": expected,
                         "actual_controller_exit": completed.returncode,
                         "native_exit": native["exit_code"], "timed_out": native["timed_out"],
                         "controller_status": summary["status"]})
    result = {"status": "pass", "cases": rows,
              "scope": "Actual owned process classification fixtures; not product or model inference coverage."}
    with (args.output / "report.json").open("x") as stream:
        json.dump(result, stream, indent=2, sort_keys=True)
        stream.write("\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
