#!/usr/bin/env python3
"""Bounded subprocess controller for the discovery consumer; never mutates production."""

import argparse
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import time


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
TARGETS = ("framing", "context", "telemetry", "invalidation", "knowledge", "memory", "review", "settlement", "agent_driver", "modules", "approval", "artifacts", "collaboration", "harness", "journal", "leases", "migrations", "projection", "scheduler", "evidence", "artifact_stream", "codec", "models", "tools", "wire", "plugins", "mcp", "providers", "spec", "authority", "router", "filesystem", "scoped_read", "fs_snapshot", "host", "sandbox", "kernel", "fs_dispatch", "process_output", "security", "tui_paste", "web_source")


def save(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, sort_keys=True, indent=2)
        stream.write("\n")


def command(command, output, timeout, overrides=None):
    started = time.monotonic()
    timed_out = False
    with (output / "stdout.log").open("xb") as stdout, (output / "stderr.log").open("xb") as stderr:
        environment=os.environ.copy()
        environment["CARGO_PROFILE_DEV_DEBUG"]="0"
        environment.update(overrides or {})
        process = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                code = process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait(timeout=5)
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    result = {"command": command, "cwd": str(ROOT), "exit_code": code,
              "pid":process.pid,"process_group":process.pid,
              "status": "incomplete" if timed_out or code == 2 or (command[0] == "cargo" and code != 0) else "pass" if code == 0 else "fail",
              "timed_out": timed_out, "timeout_seconds": timeout,
              "environment_overrides":{"CARGO_PROFILE_DEV_DEBUG":"0", **(overrides or {})},
              "elapsed_seconds": time.monotonic() - started,
              "waited_children_max_rss_kib": usage.ru_maxrss,
              "owned_direct_child_reaped": process.poll() is not None,
              "rss_semantics": "Maximum waited child process RSS on Linux, not sum of concurrent RSS. Earlier commands in this controller can contribute to the high-water mark."}
    save(output / "process.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "workspace-check", "lint", "search", "examples", "calibration", "replay"))
    parser.add_argument("--output", type=Path, required=True, help="A directory that does not yet exist")
    parser.add_argument("--target", choices=TARGETS + ("all",), default="all")
    parser.add_argument("--profile", choices=("smoke", "standard", "extended"), default="smoke")
    parser.add_argument("--seed", type=int, default=7319)
    parser.add_argument("--cases", type=int)
    parser.add_argument("--max-steps", type=int)
    parser.add_argument("--case", type=Path)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--target-dir", type=Path)
    args = parser.parse_args()
    if args.timeout <= 0 or args.seed < 0 or args.seed >= 2**64:
        parser.error("positive timeout and a u64 seed are required")
    args.output = args.output.resolve()
    args.output.mkdir(mode=0o700)
    target_dir = (args.target_dir or ROOT / "target/praxis").resolve()
    manifest = HERE / "Cargo.toml"
    binary = target_dir / "debug/peritus-praxis"
    if args.command in ("build","workspace-check","lint"):
        if args.command=="workspace-check":
            invocation=["cargo","check","--locked","--offline","--workspace","--manifest-path",str(ROOT/"Cargo.toml"),"--target-dir",str(target_dir),"-j","1"]
        else:
            invocation=["cargo","build" if args.command=="build" else "clippy","--locked","--offline","--manifest-path",str(manifest),"--target-dir",str(target_dir),"-j","1"]
            if args.command=="lint":
                invocation.extend(["--all-targets","--all-features","--","-D","warnings"])
        result = command(invocation, args.output, args.timeout)
        print(json.dumps(result))
        return 0 if result["status"] == "pass" else 2
    if not binary.is_file():
        parser.error(f"build the actual consumer first: missing {binary}")
    targets = TARGETS if args.target == "all" else (args.target,)
    if args.command == "replay":
        if args.case is None:
            parser.error("replay requires --case")
        targets = ("replay",)
    results = []
    for target in targets:
        folder = args.output / target
        folder.mkdir(mode=0o700)
        subject = str(args.case.resolve()) if args.command == "replay" else target
        invocation = [str(binary), args.command, subject, "--output", str(folder / "case-output"), "--profile", args.profile, "--seed", str(args.seed)]
        if args.cases is not None:
            invocation.extend(["--cases", str(args.cases)])
        if args.max_steps is not None:
            invocation.extend(["--max-steps", str(args.max_steps)])
        result = command(invocation, folder, args.timeout)
        result["target"] = target
        report = folder / "case-output/report.json"
        if report.exists():
            result["report"] = json.loads(report.read_text())
        elif result["status"] == "pass":
            result["status"] = "incomplete"
            result["reason"] = "child returned success without a report"
        results.append(result)
    status = "incomplete" if any(r["status"] == "incomplete" for r in results) else "fail" if any(r["status"] == "fail" for r in results) else "pass"
    summary = {"schema_version": 1, "status": status, "results": results}
    save(args.output / "summary.json", summary)
    print(json.dumps({"status": status, "targets": [{"target":r["target"],"status":r["status"],"seconds":r["elapsed_seconds"]} for r in results], "output":str(args.output)}))
    return {"pass":0,"fail":1,"incomplete":2}[status]


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError,ValueError,subprocess.SubprocessError) as error:
        print(json.dumps({"status":"incomplete","error":str(error)}),file=sys.stderr)
        sys.exit(2)
