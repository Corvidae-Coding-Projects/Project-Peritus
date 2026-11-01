#!/usr/bin/env python3
"""Check both campaign controllers against real report/exit/signal/timeout processes."""

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
    parser.add_argument("--controller", choices=("all", "public", "owner"), default="all")
    parser.add_argument("--public-controller", type=Path, default=HERE / "campaign.py")
    args = parser.parse_args()
    args.output.mkdir(mode=0o700)
    rows = []
    cases = (
        ("success", '{"status":"pass"}', "raise SystemExit(0)", 0),
        ("ordinary-failure", '{"status":"fail"}', "raise SystemExit(FAILURE_EXIT)", 1),
        ("pass-then-abnormal-exit", '{"status":"pass"}', "raise SystemExit(7)", 2),
        ("pass-then-signal", '{"status":"pass"}', "os.kill(os.getpid(), signal.SIGTERM)", 2),
        ("pass-then-timeout", '{"status":"pass"}', "time.sleep(60)", 2),
        ("failure-report-with-success-exit", '{"status":"fail"}', "raise SystemExit(0)", 2),
        ("pass-report-with-failure-exit", '{"status":"pass"}', "raise SystemExit(FAILURE_EXIT)", 2),
        ("failure-report-with-abnormal-exit", '{"status":"fail"}', "raise SystemExit(7)", 2),
        ("no-report-with-success-exit", None, "raise SystemExit(0)", 2),
        ("no-report-with-failure-exit", None, "raise SystemExit(FAILURE_EXIT)", 2),
        ("incomplete-report", '{"status":"incomplete"}', "raise SystemExit(0)", 2),
        ("unknown-status", '{"status":"banana"}', "raise SystemExit(0)", 2),
        ("malformed-report", '{"status":', "raise SystemExit(0)", 2),
        ("array-report", '[]', "raise SystemExit(0)", 2),
        ("null-report", 'null', "raise SystemExit(0)", 2),
    )
    controllers = ("public", "owner") if args.controller == "all" else (args.controller,)
    with tempfile.TemporaryDirectory(prefix="peritus-controller-") as temporary:
        scratch = Path(temporary)
        snapshot = owner.snapshot()
        for controller in controllers:
            for name, reported, terminal, expected in cases:
                case_id = f"{controller}-{name}"
                target = scratch / case_id
                (target / "debug").mkdir(parents=True)
                executable = target / "debug/peritus-praxis"
                failure_exit = 101 if controller == "owner" else 1
                source = (
                    f"#!{sys.executable}\n"
                    "import json, os, signal, sys, time\nfrom pathlib import Path\n"
                    + ("folder=Path(os.environ['PRAXIS_OUTPUT'])\n" if controller == "owner" else
                       "folder=Path(sys.argv[sys.argv.index('--output')+1])\n")
                    + "folder.mkdir()\n"
                )
                if reported is not None:
                    source += f"(folder/'report.json').write_text({reported!r})\n"
                source += terminal.replace("FAILURE_EXIT", str(failure_exit)) + "\n"
                executable.write_text(source)
                executable.chmod(0o700)
                destination = args.output / case_id
                if controller == "owner":
                    receipt = target / "receipt.json"
                    receipt.write_text(json.dumps({
                        "package": "peritus-product-runner", "inputs": snapshot,
                        "executable": str(executable),
                        "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
                    }))
                    invocation = [sys.executable, str(HERE / "owner.py"), "examples", "--target", "inputs",
                                  "--receipt", str(receipt)]
                    subject = "inputs"
                else:
                    invocation = [sys.executable, str(args.public_controller), "examples", "--target", "framing",
                                  "--target-dir", str(target)]
                    subject = "framing"
                completed = subprocess.run(invocation + ["--timeout", "1", "--output", str(destination)],
                    cwd=ROOT, capture_output=True, text=True, timeout=15, check=False)
                (args.output / (case_id + ".stdout.log")).write_text(completed.stdout)
                (args.output / (case_id + ".stderr.log")).write_text(completed.stderr)
                native = json.loads((destination / subject / "process.json").read_text())
                summary_path = destination / "summary.json"
                summary = json.loads(summary_path.read_text()) if summary_path.exists() else {}
                expected_status = {0: "pass", 1: "fail", 2: "incomplete"}[expected]
                passed = (completed.returncode == expected and summary.get("status") == expected_status
                          and native["owned_direct_child_reaped"])
                rows.append({"name": case_id, "expected_controller_exit": expected,
                             "actual_controller_exit": completed.returncode,
                             "native_exit": native["exit_code"], "timed_out": native["timed_out"],
                             "controller_status": summary.get("status"), "passed": passed})
    result = {"status": "pass" if all(row["passed"] for row in rows) else "fail", "cases": rows,
              "scope": "Actual owned process classification fixtures; not product or model inference coverage."}
    with (args.output / "report.json").open("x") as stream:
        json.dump(result, stream, indent=2, sort_keys=True)
        stream.write("\n")
    print(json.dumps(result))
    return 0 if result["status"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
