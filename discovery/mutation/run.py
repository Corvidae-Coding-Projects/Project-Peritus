#!/usr/bin/env python3
"""Run cargo-mutants against real public models without release dependencies.

The source tree is never mutated. A disposable copy gains the discovery test
packages as root workspace members; cargo-mutants makes its own per-worker copies.
"""
from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]
ACTIVE_OUTPUT = None
TEST_MEMBERS = ("discovery/proptest-praxis", "discovery/proptest-praxis/stage8-policy", "discovery/mutation")
DEFAULT_FILES = ("crates/**/*.rs", "xtask/src/**/*.rs")
DISCOVERY_TEST_PACKAGES = (
    "peritus-mutation-tests",
    "peritus-stage8-policy-properties",
    "peritus-debugger",
    "peritus-eval",
    "peritus-evolution",
    "peritus-telemetry",
)


def selected_files(requested):
    """Use explicit file filters instead of silently unioning them with defaults."""
    return tuple(requested) if requested else DEFAULT_FILES


def matches_file(path, patterns):
    """Match cargo-mutants file globs against a workspace-relative source path."""
    return any(matches_path_glob(path, pattern) for pattern in patterns)


def matches_path_glob(path, pattern):
    """Match slash-delimited globs with `**` spanning zero or more path segments."""
    if "/" not in pattern:
        return fnmatch.fnmatchcase(Path(path).name, pattern)
    path_parts = tuple(path.split("/"))
    pattern_parts = tuple(pattern.split("/"))

    def match(path_index, pattern_index):
        if pattern_index == len(pattern_parts):
            return path_index == len(path_parts)
        part = pattern_parts[pattern_index]
        if part == "**":
            return match(path_index, pattern_index + 1) or (
                path_index < len(path_parts) and match(path_index + 1, pattern_index)
            )
        return (
            path_index < len(path_parts)
            and fnmatch.fnmatchcase(path_parts[path_index], part)
            and match(path_index + 1, pattern_index + 1)
        )

    return match(0, 0)


def selected_test_packages(mutation_packages):
    """Run each mutated package's own tests before the cross-package discovery gates."""
    return tuple(dict.fromkeys((*mutation_packages, *DISCOVERY_TEST_PACKAGES)))


def selected_mutation_packages(requested, mutants_by_package):
    """Resolve implicit workspace selection to the packages present in this inventory."""
    return tuple(dict.fromkeys(requested)) if requested else tuple(sorted(mutants_by_package))


def configure_test_packages(text, packages):
    """Replace the checked-in discovery defaults for one disposable campaign."""
    replacement = "test_package = [" + ", ".join(json.dumps(package) for package in packages) + "]"
    configured, replacements = re.subn(r"(?m)^test_package = \[[^\n]*\]$", replacement, text)
    if replacements != 1:
        raise RuntimeError("mutation config must contain exactly one single-line test_package setting")
    return configured


def compiler_env():
    env = os.environ.copy()
    for key in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "CARGO_TARGET_DIR", "CARGO_MUTANTS_JOBS"):
        env.pop(key, None)
    # Native dependencies must not inherit user compiler-cache state or require
    # access to cache directories outside the disposable mutation workspace.
    env.update(CCACHE_DISABLE="1", SCCACHE_DISABLE="1")
    return env


def execute(argv, *, cwd=ROOT, env=None, log=None, timeout=1800):
    env = compiler_env() if env is None else env
    output = log.open("w") if log else None
    try:
        process = subprocess.Popen(argv, cwd=cwd, env=env, start_new_session=True, text=True,
                                   stdout=output if output else subprocess.PIPE,
                                   stderr=subprocess.STDOUT if output else subprocess.PIPE)
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
            if output:
                output.write(f"\nCONTROLLER_TIMEOUT seconds={timeout}\n")
                return 124
            raise RuntimeError(f"command interrupted or timed out: {argv}")
        if output:
            return process.returncode
        if process.returncode:
            raise RuntimeError(f"command failed ({process.returncode}): {argv}: {stderr}")
        return stdout
    finally:
        if output:
            output.close()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def metadata(root):
    return json.loads(execute(["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version=1"], cwd=root))


def inventory(output, args):
    meta = metadata(ROOT)
    members = set(meta["workspace_members"])
    packages = [{"name": p["name"], "manifest": str(Path(p["manifest_path"]).relative_to(ROOT)),
                 "targets": [{"name": t["name"], "kind": t["kind"], "test": t["test"]} for t in p["targets"]]}
                for p in meta["packages"] if p["id"] in members]
    command = ["cargo", "mutants", "--workspace", "--list", "--json"]
    for package in args.package:
        command += ["--package", package]
    if args.regex:
        command += ["--re", args.regex]
    files = selected_files(args.file)
    for pattern in files:
        command += ["--file", pattern]
    if args.shard:
        command += ["--shard", args.shard]
    with (output / "mutants.json").open("w") as stdout, (output / "inventory.stderr.log").open("w") as stderr:
        result = subprocess.run(command, cwd=ROOT, env=compiler_env(), text=True, stdout=stdout, stderr=stderr, timeout=1800)
    if result.returncode:
        raise RuntimeError(f"mutation inventory failed ({result.returncode}); inspect inventory.stderr.log")
    mutants = json.loads((output / "mutants.json").read_text())
    assert isinstance(mutants, list)
    counts = {}
    for mutant in mutants:
        name = mutant.get("package", "unknown")
        if isinstance(name, dict):
            name = name.get("name", "unknown")
        counts[name] = counts.get(name, 0) + 1
    unexpected_packages = sorted({item.get("package", {}).get("name", "unknown")
                                  if isinstance(item.get("package"), dict) else item.get("package", "unknown")
                                  for item in mutants} - set(args.package)) if args.package else []
    unexpected_files = sorted({item.get("file", "") for item in mutants
                               if not matches_file(item.get("file", ""), files)})
    report = {"status": "inventory_only", "source_head": execute(["git", "rev-parse", "HEAD"]).strip(),
              "active_packages": packages, "active_package_count": len(packages), "mutation_candidates": len(mutants),
              "mutants_by_package": counts, "command": command, "selected_files": list(files),
              "unexpected_packages": unexpected_packages, "unexpected_files": unexpected_files,
              "limits": ["Inventory is not execution coverage.", "Only current-host cfg and default features are selected.",
                         "Public models do not exercise every private owner, CLI, daemon, platform or external-provider behavior."]}
    problems = []
    if unexpected_packages:
        problems.append(f"{len(unexpected_packages)} packages outside --package")
    if unexpected_files:
        problems.append(f"{len(unexpected_files)} files outside --file")
    if args.regex:
        try:
            requested = re.compile(args.regex)
        except re.error as error:
            raise RuntimeError(f"--regex must use the common Python/Rust regex subset: {error}")
        unexpected = [item["name"] for item in mutants if not requested.search(item["name"])]
        report["unexpected_regex_candidates"] = unexpected
        if unexpected:
            problems.append(f"{len(unexpected)} candidates outside --regex")
    write(output / "inventory.json", report)
    if problems:
        raise RuntimeError("cargo-mutants emitted candidates outside the requested selection (" +
                           ", ".join(problems) + "); inspect inventory.json")
    return report


def stage(output, mutation_packages):
    destination = output / "source"
    execute(["git", "clone", "--shared", "--no-checkout", "--quiet", str(ROOT), str(destination)])
    # Own the clone's index and references; only immutable Git objects are shared.
    execute(["git", "reset", "--mixed", "--quiet", "HEAD"], cwd=destination)
    names = execute(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"]).split("\0")
    records = []
    for relative in sorted(set(filter(None, names))):
        source = ROOT / relative
        if not source.exists() and not source.is_symlink():
            continue  # A tracked deletion is part of the requested source snapshot.
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_symlink():
            link = os.readlink(source)
            resolved = (target.parent / link).resolve()
            if not resolved.is_relative_to(destination):
                raise RuntimeError(f"source symlink escapes disposable tree: {relative}")
            target.symlink_to(link)
            content = link.encode()
        elif source.is_file():
            shutil.copy2(source, target)
            content = source.read_bytes()
        else:
            raise RuntimeError(f"unsupported source entry {relative}")
        records.append({"path": relative, "sha256": hashlib.sha256(content).hexdigest()})
    root = destination / "Cargo.toml"
    text = root.read_text()
    start = text.index("members = [")
    end = text.index("]", start)
    text = text[:end] + "".join(f'    "{member}",\n' for member in TEST_MEMBERS) + text[end:]
    # Only discovery membership/profiles change in the disposable tree.
    # Production dependency versions and sources are copied byte-for-byte.
    text += '\n[profile.test]\ndebug = 0\nincremental = true\noverflow-checks = true\n'
    root.write_text(text)
    for member in TEST_MEMBERS:
        manifest = destination / member / "Cargo.toml"
        text = manifest.read_text().replace("[workspace]\n", "")
        manifest.write_text(text)
    config = destination / ".cargo" / "mutants.toml"
    config.write_text(configure_test_packages(config.read_text(), selected_test_packages(mutation_packages)))
    write(output / "source-manifest.json", records)
    return destination


def inspect_outcomes(output, expected_targets, expected_test_packages):
    outcomes_path = output / "mutants.out/outcomes.json"
    outcomes = None
    complete, counts, problems = False, {}, []
    try:
        outcomes = json.loads(outcomes_path.read_text())
        expected_names = {item["name"] for item in json.loads((output / "mutants.json").read_text())}
        observed_names = []
        for outcome in outcomes["outcomes"]:
            scenario = outcome["scenario"]
            if not isinstance(scenario, dict) or "Mutant" not in scenario:
                problems.append("unexpected baseline or nonmutant scenario")
                continue
            mutant = scenario["Mutant"]
            observed_names.append(mutant["name"])
            summary = outcome["summary"]
            counts[summary] = counts.get(summary, 0) + 1
            if summary not in {"CaughtMutant", "MissedMutant", "Unviable", "Timeout"}:
                problems.append(f"unclassified outcome: {summary}")
            tests = [phase for phase in outcome["phase_results"] if phase["phase"]=="Test"]
            if summary in {"CaughtMutant", "MissedMutant"}:
                if len(tests)!=1 or not all(any(arg.startswith(f"--package={package}@") for arg in tests[0]["argv"]) for package in expected_test_packages):
                    problems.append("mutant did not execute all configured discovery test packages")
            if summary=="MissedMutant":
                log_path=output / "mutants.out" / outcome["log_path"]
                markers=set(re.findall(r"MUTATION_CAMPAIGN target=(\w+) fixed=\d+ generated=\d+",log_path.read_text()))
                if markers!=expected_targets:
                    problems.append("surviving mutant skipped a selected model")
                if "MUTATION_DIAGNOSTICS meaningful_reachable_error_families=" not in log_path.read_text():
                    problems.append("surviving mutant skipped codec diagnostic invariants")
        if set(observed_names)!=expected_names or len(observed_names)!=len(expected_names):
            problems.append("scenario inventory is missing, duplicated or unexpected")
        if outcomes["total_mutants"] != len(expected_names):
            problems.append("outcome total does not match selected inventory")
        complete=not problems and outcomes.get("end_time") is not None
    except (OSError, ValueError, KeyError, TypeError) as error:
        problems.append(f"missing or invalid outcomes: {error}")
    return outcomes, counts, problems, complete


def main():
    global ACTIVE_OUTPUT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("inventory", "run"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--package", action="append", default=[])
    parser.add_argument("--file", action="append", default=[])
    parser.add_argument("--regex")
    parser.add_argument("--shard")
    parser.add_argument("--jobs", type=int, default=1, help="mutation workers (each runs a Cargo build)")
    parser.add_argument("--build-jobs", type=int, default=1)
    parser.add_argument("--cases", type=int, default=64)
    parser.add_argument("--steps", type=int, default=64)
    parser.add_argument("--seed", type=int, default=7319)
    parser.add_argument("--target", action="append", default=[], help="public model campaign; default all 42")
    parser.add_argument("--timeout", type=float, help="seconds per mutant test; default 3x observed baseline test duration, minimum 60")
    parser.add_argument("--build-timeout", type=float, default=1800)
    parser.add_argument("--campaign-seconds", type=float, default=3600, help="total mutation execution budget; expiration remains incomplete")
    args = parser.parse_args()
    if min(args.jobs, args.build_jobs, args.cases, args.steps) < 1 or (args.timeout is not None and args.timeout <= 0) or args.build_timeout <= 0 or args.campaign_seconds <= 0:
        parser.error("budgets and concurrency must be positive")
    targets = re.findall(r"^campaign!\((\w+),", (ROOT / "discovery/mutation/tests/public_models.rs").read_text(), re.M)
    if set(args.target) - set(targets):
        parser.error("unknown model target: " + ", ".join(sorted(set(args.target) - set(targets))))
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("output must be outside the source checkout")
    output.mkdir(parents=True, exist_ok=False)
    ACTIVE_OUTPUT = output
    started = time.time()
    write(output / "request.json", {**vars(args), "output": str(output), "targets": args.target or targets})
    report = inventory(output, args)
    if args.command == "inventory":
        print(json.dumps({"status": report["status"], "packages": report["active_package_count"], "mutants": report["mutation_candidates"], "output": str(output)}))
        return 0
    if report["mutation_candidates"] == 0:
        raise RuntimeError("selection contains zero mutations; no campaign executed")
    mutation_packages = selected_mutation_packages(args.package, report["mutants_by_package"])
    test_packages = selected_test_packages(mutation_packages)
    source = stage(output, mutation_packages)
    env = compiler_env()
    env.pop("PERITUS_MUTATION_TARGETS", None)
    env.update({"CARGO_BUILD_JOBS": str(args.build_jobs), "CARGO_PROFILE_TEST_DEBUG": "0",
                "PERITUS_MUTATION_CASES": str(args.cases), "PERITUS_MUTATION_STEPS": str(args.steps),
                "PERITUS_MUTATION_SEED": str(args.seed), "PERITUS_MUTATION_FAILURES": str(output / "failures"),
                "PROPTEST_CASES": str(args.cases), "PROPTEST_RNG_SEED": str(args.seed)})
    if args.target:
        env["PERITUS_MUTATION_TARGETS"] = ",".join(args.target)
    # No third-party dependency updates: the source lock is reconciled only for
    # adding the already-pinned optional discovery dependency graph.
    lock = ["cargo", "metadata", "--offline", "--format-version=1"]
    code = execute(lock, cwd=source, env=env, log=output / "lock.log")
    if code:
        raise RuntimeError("cannot resolve staged workspace; inspect lock.log")
    original = tomllib.loads((ROOT / "Cargo.lock").read_text())
    staged = tomllib.loads((source / "Cargo.lock").read_text())
    def external(lock):
        return {(p["name"], p["version"], p["source"]): p for p in lock["package"] if "source" in p}
    originals, resolved = external(original), external(staged)
    changed = [key for key, value in originals.items() if resolved.get(key) != value]
    if changed:
        raise RuntimeError(f"staging changed existing external dependencies: {changed}")
    write(output / "lock-verification.json", {"status":"pass","existing_external_entries_unchanged":len(originals),"additional_external_entries":len(resolved)-len(originals)})
    # cargo-mutants 27.1.0 baselines the mutated production packages rather
    # than configured test packages. Run the real discovery baseline ourselves
    # and verify its execution markers before allowing any mutants.
    baseline = ["cargo", "test", "--locked"]
    for package in test_packages:
        baseline += ["--package", package]
    baseline += ["--", "--test-threads=1", "--nocapture"]
    baseline_env = env.copy()
    flags = env.get("CARGO_ENCODED_RUSTFLAGS")
    if flags is None:
        flags = "\x1f".join(env.get("RUSTFLAGS", "").split())
    baseline_env["CARGO_ENCODED_RUSTFLAGS"] = (flags + "\x1f" if flags else "") + "--cap-lints=warn"
    baseline_code = execute(baseline, cwd=source, env=baseline_env, log=output / "baseline.log", timeout=args.build_timeout + (args.timeout or 900))
    baseline_text = (output / "baseline.log").read_text()
    executed = re.findall(r"MUTATION_CAMPAIGN target=(\w+) fixed=(\d+) generated=(\d+)", baseline_text)
    actual_targets = {entry[0] for entry in executed}
    expected_targets = set(args.target or targets)
    diagnostics = re.search(r"MUTATION_DIAGNOSTICS meaningful_reachable_error_families=(\d+)", baseline_text)
    baseline_report = {"status":"pass" if baseline_code==0 and actual_targets==expected_targets and diagnostics is not None else "fail",
                       "codec_diagnostic_families":int(diagnostics.group(1)) if diagnostics else 0,
                       "exit_code":baseline_code, "command":baseline,
                       "model_campaigns":[{"target":name,"fixed":int(fixed),"generated":int(generated)} for name,fixed,generated in executed]}
    write(output / "baseline.json",baseline_report)
    if baseline_report["status"] != "pass":
        raise RuntimeError("real discovery baseline failed or skipped selected models; inspect baseline.log")
    observed_test_seconds = sum(float(value) for value in re.findall(r"test result: ok\..*?finished in ([0-9.]+)s", baseline_text))
    test_timeout = args.timeout if args.timeout is not None else max(60.0, observed_test_seconds * 3.0)
    write(output / "test-budget.json", {"observed_baseline_test_seconds": observed_test_seconds,
          "timeout_seconds": test_timeout, "explicit_timeout": args.timeout is not None})
    command = ["cargo", "mutants", "--workspace", "--baseline", "skip", "--jobserver-tasks", str(args.build_jobs),
               "--timeout", str(test_timeout), "--build-timeout", str(args.build_timeout), "--output", str(output), "--caught", "--unviable"]
    if args.jobs == 1:
        command += ["--in-place"]
    else:
        command += ["--jobs", str(args.jobs), "--copy-target", "true"]
    for package in args.package:
        command += ["--package", package]
    if args.regex:
        command += ["--re", args.regex]
    for pattern in selected_files(args.file):
        command += ["--file", pattern]
    if args.shard:
        command += ["--shard", args.shard]
    write(output / "execution.json", {"command":command,"cases_per_model":args.cases,"steps":args.steps,"seed":args.seed,"targets":args.target or targets})
    code = execute(command, cwd=source, env=env, log=output / "campaign.log", timeout=args.campaign_seconds)
    outcomes, counts, problems, complete = inspect_outcomes(output, expected_targets, test_packages)
    result = {"status":"incomplete" if not complete else "pass" if code==0 and not counts.get("MissedMutant") and not counts.get("Timeout") else "fail", "exit_code":code, "seconds":time.time()-started,
              "mutation_candidates":report["mutation_candidates"],"source_head":report["source_head"],
              "outcomes":outcomes, "baseline":baseline_report, "counts":counts,"completeness_problems":problems,
              "unexecuted_candidates":report["mutation_candidates"]-sum(counts.values()),
              "limits":report["limits"],"output":str(output)}
    write(output / "report.json",result)
    print(json.dumps({key:value for key,value in result.items() if key!='outcomes'}))
    return code if code else 0 if result["status"]=="pass" else 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        if ACTIVE_OUTPUT is not None:
            try:
                if not (ACTIVE_OUTPUT / "report.json").exists():
                    write(ACTIVE_OUTPUT / "report.json", {"status":"incomplete","error":str(error),"exit_code":2})
            except OSError:
                pass
        print(f"mutation controller failed: {error}", file=sys.stderr)
        sys.exit(2)
