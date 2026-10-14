#!/usr/bin/env python3
"""Run real Cargo build/Clippy checks; record evidence without claiming behavior."""

import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time


EXCLUDED_NAMES = {".git", "target", ".venv", "venv", "node_modules", "__pycache__"}
PATTERNS = {
    "cancellation-boundary": r"\b(?:select|try_join)\s*!|\b(?:timeout|timeout_at|try_join_all)\s*\(",
    "task-lifecycle": r"\b(?:spawn|spawn_local|spawn_blocking|abort|abort_all|shutdown)\s*\(|\bJoinSet\b",
    "shared-state": r"\b(?:Mutex|RwLock|RefCell|UnsafeCell|Arc)\b|\.lock\s*\(",
    "queue-or-partial-io": r"\b(?:unbounded_channel|read_exact|read_to_end|read_to_string|write_all)\s*\(|\.send\s*\(",
    "unsafe-boundary": r"\bunsafe\b|\b(?:transmute|from_raw_parts|set_len|get_unchecked)\b",
    "panic-or-ignored-outcome": r"\b(?:panic|unreachable|todo)\s*!|\.\s*(?:unwrap|expect)\s*\(|\bcatch_unwind\b|\blet\s+_\s*=",
    "policy-or-ownership-workaround": r"#\s*!?\s*\[\s*(?:allow|expect)\b|\b(?:forget|leak)\s*\(|\.clone\s*\(",
}


def inside(path, parent):
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def utc_now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def file_hash(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_files(root, excluded):
    paths = []
    def walk_error(error):
        raise error
    for base, dirs, files in os.walk(root, followlinks=False, onerror=walk_error):
        current = Path(base)
        dirs[:] = sorted(name for name in dirs
                         if name not in EXCLUDED_NAMES
                         and not (current / name).is_symlink()
                         and not any(inside((current / name).resolve(), item) for item in excluded))
        for name in sorted(files):
            path = current / name
            if not path.is_symlink() and path.is_file() and not any(inside(path.resolve(), item) for item in excluded):
                paths.append(path)
    return sorted(paths)


def fingerprint(root, excluded):
    paths = source_files(root, excluded)
    digest = hashlib.sha256()
    inventory = []
    patterns = {key: re.compile(value) for key, value in PATTERNS.items()}
    for path in paths:
        relative = path.relative_to(root).as_posix()
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(bytes.fromhex(file_hash(path)))
        if path.suffix == ".rs":
            for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
                for category, pattern in patterns.items():
                    if pattern.search(line):
                        inventory.append({"path": relative, "line": number, "category": category})
    return {"sha256": digest.hexdigest(), "file_count": len(paths)}, inventory


def run_command(name, argv, cwd, output, timeout):
    log = output / (name + ".log")
    item = {"name": name, "argv": argv, "cwd": str(cwd), "log": log.name,
            "started_at": utc_now(), "status": "BLOCKED", "exit_code": None}
    start = time.monotonic()
    with log.open("wb") as stream:
        try:
            process = subprocess.Popen(argv, cwd=str(cwd), stdout=stream, stderr=subprocess.STDOUT,
                                       start_new_session=(os.name == "posix"))
            try:
                item["exit_code"] = process.wait(timeout=timeout)
                item["status"] = "PASS" if item["exit_code"] == 0 else "FAIL"
            except subprocess.TimeoutExpired:
                if os.name == "posix":
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                else:
                    process.kill()
                process.wait()
                item["reason"] = "Command exceeded its execution budget; process was stopped."
        except OSError as error:
            item["reason"] = str(error)
    item["duration_seconds"] = round(time.monotonic() - start, 3)
    item["log_sha256"] = file_hash(log)
    return item


def compiler_artifacts(log, package_ids):
    artifacts = []
    for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(event, dict) or event.get("reason") != "compiler-artifact" or event.get("package_id") not in package_ids:
            continue
        files = []
        for filename in event.get("filenames", []):
            path = Path(filename)
            if path.is_file():
                files.append({"path": str(path), "sha256": file_hash(path)})
        artifacts.append({"package_id": event["package_id"], "target": event.get("target"),
                          "executable": event.get("executable"), "fresh": event.get("fresh"), "files": files})
    return artifacts


def classify_failure(item, output):
    if item["status"] != "FAIL":
        return
    text = (output / item["log"]).read_text(encoding="utf-8", errors="replace").lower()
    blockers = ["unknown lint", "no such command: `clippy`", "component 'clippy'", "component `clippy`",
                "failed to download", "failed to get", "could not resolve host", "failed to update",
                "unable to update", "attempting to make an http request", "lock file needs to be updated",
                "lockfile needs to be updated", "toolchain", "can't find crate for `std`"]
    if any(marker in text for marker in blockers):
        item["status"] = "BLOCKED"
        item["reason"] = "Required tool, dependency, lint, lockfile, or target setup unavailable; inspect the log."


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="New evidence directory; existing paths are refused.")
    parser.add_argument("--package", action="append", default=[])
    parser.add_argument("--features")
    parser.add_argument("--no-default-features", action="store_true")
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--target")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--forbid-unsafe", action="store_true")
    parser.add_argument("--timeout", type=float, default=180)
    args = parser.parse_args()
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout must be finite and positive")
    manifest, output = args.manifest.resolve(), args.output.resolve()
    if not manifest.is_file() or manifest.name != "Cargo.toml":
        parser.error("--manifest must identify an existing Cargo.toml")
    if inside(manifest.parent, output):
        parser.error("--output must not contain the project")
    try:
        output.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error("Cannot create a new evidence directory: " + str(error))

    report = {"schema_version": 1, "started_at": utc_now(), "manifest": str(manifest),
              "static_status": "BLOCKED", "behavioral_status": "NOT RUN", "commands": [],
              "configuration": {"packages": args.package, "features": args.features,
                                "no_default_features": args.no_default_features, "release": args.release,
                                "target": args.target, "offline": args.offline, "forbid_unsafe": args.forbid_unsafe},
              "limitations": ["Static checks do not exercise application behavior.",
                              "Inventory is lexical and advisory, not semantic analysis.",
                              "Fingerprint excludes symlinks, generated/cache directories, and external dependencies.",
                              "This report is not an execution attestation or anti-tampering boundary."]}
    try:
        cargo, rustc = shutil.which("cargo"), shutil.which("rustc")
        if not cargo or not rustc:
            report["reason"] = "Cargo and rustc must both be available on PATH."
            report["source_before"], report["risk_inventory"] = fingerprint(manifest.parent, [output])
            return finish(output, report)
        for name, command in [("cargo-version", [cargo, "--version"]), ("rustc-version", [rustc, "--version"]),
                              ("clippy-version", [cargo, "clippy", "--version"])]:
            item = run_command(name, command, manifest.parent, output, args.timeout)
            report["commands"].append(item)
            if item["status"] != "PASS":
                item["status"] = "BLOCKED"
                report["reason"] = "Toolchain preflight failed; inspect " + item["log"]
                return finish(output, report)

        metadata_cmd = [cargo, "metadata", "--manifest-path", str(manifest), "--format-version", "1", "--no-deps", "--locked"]
        if args.offline:
            metadata_cmd.append("--offline")
        item = run_command("metadata", metadata_cmd, manifest.parent, output, args.timeout)
        classify_failure(item, output)
        report["commands"].append(item)
        if item["status"] != "PASS":
            report["static_status"] = item["status"]
            return finish(output, report)
        metadata = None
        for line in (output / "metadata.log").read_text(encoding="utf-8", errors="replace").splitlines():
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict) and "workspace_root" in value:
                metadata = value
        if metadata is None:
            raise ValueError("Cargo metadata did not return a workspace object")
        root = Path(metadata["workspace_root"]).resolve()
        if inside(root, output):
            raise ValueError("Evidence directory must not contain the Cargo workspace")
        excluded = [output, Path(metadata["target_directory"]).resolve()]
        report["workspace_root"] = str(root)
        report["source_before"], report["risk_inventory"] = fingerprint(root, excluded)

        common = ["--manifest-path", str(manifest), "--locked", "--message-format=json"]
        for package in args.package:
            common.extend(["--package", package])
        for flag, enabled in [("--no-default-features", args.no_default_features), ("--release", args.release), ("--offline", args.offline)]:
            if enabled:
                common.append(flag)
        if args.features:
            common.extend(["--features", args.features])
        if args.target:
            common.extend(["--target", args.target])
        build = run_command("build", [cargo, "build"] + common, manifest.parent, output, args.timeout)
        classify_failure(build, output)
        report["commands"].append(build)
        report["artifacts"] = compiler_artifacts(output / "build.log", set(metadata["workspace_members"]))
        lint_flags = ["-F", "unknown_lints", "-F", "unused_must_use", "-F", "let_underscore_lock", "-F", "clippy::let_underscore_future",
                      "-F", "clippy::let_underscore_lock"]
        if args.forbid_unsafe:
            lint_flags.extend(["-F", "unsafe_code"])
        report["lint_flags"] = lint_flags
        if build["status"] == "PASS":
            lint = run_command("clippy", [cargo, "clippy"] + common + ["--no-deps", "--"] + lint_flags,
                               manifest.parent, output, args.timeout)
            classify_failure(lint, output)
            report["commands"].append(lint)
        report["source_after"], _ = fingerprint(root, excluded)
        report["source_unchanged"] = report["source_before"] == report["source_after"]
        statuses = [item["status"] for item in report["commands"]]
        report["static_status"] = "FAIL" if "FAIL" in statuses else "BLOCKED" if "BLOCKED" in statuses else "PASS"
        if not report["source_unchanged"]:
            report["static_status"] = "BLOCKED"
            report["reason"] = "Source changed during checks; rerun on stable inputs."
    except (OSError, ValueError, KeyError) as error:
        report["static_status"] = "BLOCKED"
        report["reason"] = str(error)
    return finish(output, report)


def finish(output, report):
    report["finished_at"] = utc_now()
    path = output / "report.json"
    path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({"static_status": report["static_status"], "behavioral_status": report["behavioral_status"],
                      "report": str(path)}, ensure_ascii=False))
    return {"PASS": 0, "FAIL": 1, "BLOCKED": 2}[report["static_status"]]


if __name__ == "__main__":
    sys.exit(main())
