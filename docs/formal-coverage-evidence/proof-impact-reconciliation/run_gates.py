#!/usr/bin/env python3
"""Run and retain the 132 exact package gates on one clean, frozen candidate.

One build lane, serial Rust tests, at most two Verus CPUs. Stops on the first failure.
Outputs are evidence only; neither this driver nor an exit code grants review approval.
"""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import time
import tomllib


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(root, *args):
    return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True).stdout


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")


def require_frozen(root, candidate):
    if git(root, "rev-parse", "HEAD").decode().strip() != candidate:
        raise ValueError("HEAD differs from the frozen candidate")
    allowed = (".claude/", ".codex/", ".crosslink/")
    for row in filter(None, git(root, "status", "--porcelain=v1", "-z", "--untracked-files=all").split(b"\0")):
        status, path = row[:2], row[3:].decode()
        if status != b"??" or not (path.startswith(allowed) or path in (".mcp.json", "AGENTS.md")):
            raise ValueError(f"candidate checkout is not frozen: {path}")


def expected_gates(root):
    policy = tomllib.loads((root / "architecture.toml").read_text())
    names = [item["name"] for item in policy["packages"]]
    if len(names) != len(set(names)):
        raise ValueError("architecture contains duplicate package names")
    packages = {item["name"]: item["verification_class"] for item in policy["packages"]
                if item["verification_class"] in ("V", "H", "T")}
    if len(packages) != 66:
        raise ValueError("this qualification requires exactly the reviewed 66 formal packages")
    gates = []
    for name, classification in sorted(packages.items()):
        ordinary = f"cargo test --package {name} --all-targets --all-features --locked"
        strict = "--no-cheating " if classification in ("V", "H") else ""
        verus = (f"cargo verus verify --package {name} --all-features --locked --check-toolchain "
                 f"--fwd-verus-args-to roots -- {strict}--rlimit 20")
        for kind, command in (("ordinary-test", ordinary), ("verus-verify", verus)):
            gates.append({"kind": kind, "owning_crate": name, "command": command})
    return gates


def write_state(output, state):
    temporary = output / "state.json.tmp"
    temporary.write_text(json.dumps(state, indent=2) + "\n")
    temporary.replace(output / "state.json")


def terminate(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()


def execute(root, output, state, gate, index, environment, cpus, timeout):
    require_frozen(root, state["candidate_commit"])
    log = output / f"{index:03d}-{gate['owning_crate']}-{gate['kind']}.log"
    argv = shlex.split(gate["command"])
    invocation = ["taskset", "-c", ",".join(map(str, cpus)), *argv] if gate["kind"] == "verus-verify" else argv
    entry = dict(gate, started_at=now(), output=log.name, invocation=invocation)
    state["active"] = entry
    write_state(output, state)
    print(f"[{index}/132] START {gate['owning_crate']} {gate['kind']}", flush=True)
    started = time.monotonic()
    with log.open("xb") as stream:
        header = {"candidate_commit": state["candidate_commit"], "candidate_tree": state["candidate_tree"],
                  "plan_sha256": state["plan_sha256"], "gate": entry, "environment": state["environment"]}
        stream.write((json.dumps(header, sort_keys=True) + "\n\n").encode())
        stream.flush()
        with subprocess.Popen(invocation, cwd=root, env=environment, stdout=stream,
                              stderr=subprocess.STDOUT, start_new_session=True) as process:
            try:
                code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                terminate(process)
                code = 124
                stream.write(b"\nGATE DRIVER: command exceeded the recorded deadline.\n")
            except BaseException:
                terminate(process)
                raise
        entry.update(command_exit_code=code, finished_at=now(), elapsed_seconds=round(time.monotonic() - started, 3))
        try:
            require_frozen(root, state["candidate_commit"])
        except ValueError as error:
            entry["source_integrity_error"] = str(error)
            code = 125
        entry["exit_code"] = code
        entry["result"] = "passed" if code == 0 else "failed"
        stream.write(("\nGATE DRIVER RESULT: " + json.dumps(entry, sort_keys=True) + "\n").encode())
    entry["output_sha256"] = sha(log.read_bytes())
    state["results"].append(entry)
    state["active"] = None
    state["status"] = "running" if code == 0 else "failed"
    write_state(output, state)
    print(f"[{index}/132] {entry['result'].upper()} {gate['owning_crate']} {gate['kind']} ({entry['elapsed_seconds']}s)", flush=True)
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=3600)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.candidate) or args.timeout < 1:
        parser.error("supply a full commit identity and positive command timeout")
    root = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
    require_frozen(root, args.candidate)
    plan_bytes = args.plan.read_bytes()
    plan = json.loads(plan_bytes)
    gates = expected_gates(root)
    tree = git(root, "rev-parse", "HEAD^{tree}").decode().strip()
    if plan["candidate"]["commit"] != args.candidate or plan["candidate"]["tree"] != tree or [
        {key: row[key] for key in ("kind", "owning_crate", "command")} for row in plan["required_gates"]
    ] != gates:
        raise ValueError("plan does not bind this candidate and the exact 132 canonical commands")
    if os.path.lexists(args.output.absolute()):
        raise ValueError("output must be a fresh path")
    output = args.output.resolve()
    output.relative_to((root / "target").resolve())
    output.mkdir(parents=True, exist_ok=False)
    (output / "plan.json").write_bytes(plan_bytes)
    controls = {"CARGO_BUILD_JOBS": "1", "CCACHE_DISABLE": "1", "RUST_TEST_THREADS": "1"}
    environment = dict(os.environ, **controls)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    state = {"schema": "peritus.proof-impact-package-gates", "schema_version": 1,
             "candidate_commit": args.candidate,
             "candidate_tree": tree,
             "plan_sha256": sha(plan_bytes), "driver_sha256": sha(Path(__file__).read_bytes()),
             "environment": controls, "verus_cpu_affinity": cpus,
             "command_timeout_seconds": args.timeout, "started_at": now(),
             "status": "running", "active": None, "results": [], "approval": False}
    write_state(output, state)
    for index, gate in enumerate(gates, 1):
        if execute(root, output, state, gate, index, environment, cpus, args.timeout) != 0:
            return 1
    state.update(status="passed", finished_at=now())
    write_state(output, state)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
