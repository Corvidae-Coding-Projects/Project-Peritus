#!/usr/bin/env python3
"""Run source-bound mutation and sanitizer campaigns, retaining failures and coverage gaps."""

import argparse
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


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
FUZZ = HERE / "fuzz"
TOOLS = tomllib.loads((HERE / "tools.toml").read_text())


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True, default=str) + "\n")


def fingerprint():
    """Bind evidence to current bytes, including uncommitted harnesses and source."""
    paths = {ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "rust-toolchain.toml", ROOT / ".cargo/config.toml"}
    for folder in (ROOT / "crates", ROOT / "xtask", HERE):
        for directory, children, files in os.walk(folder):
            children[:] = [name for name in children if name not in {"target", "corpus", "artifacts", "coverage", "__pycache__"}]
            for name in files:
                path = Path(directory) / name
                if path.suffix in (".rs", ".toml", ".lock", ".py", ".json", ".dict") or "seeds" in path.parts:
                    paths.add(path)
    digest = hashlib.sha256()
    for path in sorted(paths):
        relative = path.relative_to(ROOT).as_posix().encode()
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big") + relative)
        digest.update(len(content).to_bytes(8, "big") + content)
    return {"head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "source_sha256": digest.hexdigest(), "files": len(paths)}


def environment(jobs):
    env = os.environ.copy()
    env.update(
        CARGO_BUILD_JOBS=str(jobs),
        CARGO_INCREMENTAL="0",
        RUST_BACKTRACE="1",
        CCACHE_DISABLE="1",
        SCCACHE_DISABLE="1",
    )
    # A user's unrelated compiler cache must not invalidate the sanitizer toolchain.
    env.pop("RUSTC_WRAPPER", None)
    env.pop("RUSTC_WORKSPACE_WRAPPER", None)
    return env


def run(command, folder, timeout, jobs=1, sanitized=False, detect_leaks=False):
    folder.mkdir(parents=True)
    started = time.monotonic()
    print("Running: " + " ".join(map(str, command)), flush=True)
    env = environment(jobs)
    if sanitized or (len(command) > 2 and command[0] == "cargo" and "fuzz" in command[:3]):
        if not shutil.which("clang") or not shutil.which("clang++"):
            raise ValueError("sanitizer builds require clang and clang++")
        env.update(CC="clang", CXX="clang++")
    if sanitized:
        options = [value for value in [env.get("ASAN_OPTIONS", ""), "detect_odr_violation=2",
                   f"detect_leaks={int(detect_leaks)}", "abort_on_error=1"] if value]
        env["ASAN_OPTIONS"] = ":".join(options)
    with (folder / "stdout.log").open("wb") as stdout, (folder / "stderr.log").open("wb") as stderr:
        process = subprocess.Popen(list(map(str, command)), cwd=ROOT, env=env,
                                   stdout=stdout, stderr=stderr, start_new_session=True)
        timed_out = False
        try:
            code = process.wait(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                code = process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait(timeout=10)
    result = {"command": list(map(str, command)), "cwd": str(ROOT), "exit_code": code,
              "timed_out": timed_out, "elapsed_seconds": time.monotonic() - started,
              "native_compiler": {key: env[key] for key in ("CC", "CXX") if key in env},
              "asan_options": env.get("ASAN_OPTIONS") if sanitized else None,
              "status": "incomplete" if timed_out else "pass" if code == 0 else "fail"}
    save(folder / "process.json", result)
    return result


def targets():
    data = tomllib.loads((FUZZ / "Cargo.toml").read_text())
    names = [entry["name"] for entry in data["bin"]]
    if not names or len(set(names)) != len(names):
        raise ValueError("expected nonempty unique fuzz targets")
    return names


def fuzz_command(action, args, target=None):
    command = ["cargo", "+" + TOOLS["fuzz_toolchain"], "fuzz", action]
    if target:
        command.append(target)
    directory = HERE / "fuzz-libafl" if args.engine == "libafl" else FUZZ
    command += ["--fuzz-dir", str(directory), "--target-dir", str(args.target_dir),
                "--sanitizer", args.sanitizer, "--codegen-units", "16"]
    if args.careful:
        command.append("--careful")
    if args.extra_traces:
        command += ["--trace-div", "--trace-gep"]
    return command


def fuzz_binary(args, target):
    version = subprocess.check_output(["rustc", "+" + TOOLS["fuzz_toolchain"], "-vV"], text=True)
    host = next(line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: "))
    binary = args.target_dir / host / "release" / target
    if not binary.is_file():
        raise ValueError(f"missing compiled fuzz executable: {binary}")
    return binary


def setup(args):
    if not shutil.which("clang") or not shutil.which("clang++"):
        raise ValueError("install the host clang C/C++ compiler before sanitizer setup")
    commands = [["rustup", "toolchain", "install", TOOLS["fuzz_toolchain"], "--profile", "minimal",
                 "--component", "rust-src,llvm-tools-preview"]]
    for tool, version in (("cargo-fuzz", TOOLS["cargo_fuzz"]), ("cargo-mutants", TOOLS["cargo_mutants"])):
        executable = shutil.which(tool)
        observed = subprocess.check_output(["cargo", tool.removeprefix("cargo-"), "--version"], text=True).strip() if executable else ""
        if observed.split()[-1:] != [version]:
            commands.append(["cargo", "install", "--locked", tool, "--version", version])
    results = [run(command, args.output / str(index), args.build_timeout, args.jobs)
               for index, command in enumerate(commands)]
    return {"results": results, "status": combine(results)}


def combine(results):
    if not results:
        return "incomplete"
    statuses = {result["status"] for result in results}
    if "fail" in statuses or not statuses <= {"pass", "incomplete"}:
        return "fail"
    return "incomplete" if "incomplete" in statuses else "pass"


def libafl_execution_counts(output):
    """Return conservative integer lower bounds from LibAFL monitor lines."""
    counts = []
    for number, suffix in re.findall(
        r"(?:execs|executions):\s*([0-9][0-9,]*(?:\.[0-9]+)?)([kKmM]?)", output
    ):
        multiplier = {"": 1, "k": 1_000, "m": 1_000_000}[suffix.lower()]
        counts.append(int(float(number.replace(",", "")) * multiplier))
    return counts


def fuzz(args):
    selected = targets() if args.target == "all" else [args.target]
    unknown = set(selected) - set(targets())
    if unknown:
        raise ValueError(f"unknown fuzz targets: {unknown}")
    results = []
    for target in selected:
        target_output = args.output / target
        build = run(fuzz_command("build", args, target), target_output / "build", args.build_timeout, args.jobs)
        if build["status"] != "pass":
            results.append({"target": target, **build})
            continue
        corpus = target_output / "corpus"
        corpus.mkdir()
        seeds = FUZZ / "seeds" / target
        if seeds.is_dir():
            for seed in seeds.iterdir():
                if seed.is_file():
                    shutil.copy2(seed, corpus / seed.name)
        if args.corpus:
            source = args.corpus / target if args.target == "all" else args.corpus
            if not source.is_dir():
                raise ValueError(f"missing resumed corpus: {source}")
            for seed in source.iterdir():
                if seed.is_file():
                    shutil.copy2(seed, corpus / hashlib.sha256(seed.read_bytes()).hexdigest())
        artifacts = target_output / "artifacts"
        artifacts.mkdir()
        binary = fuzz_binary(args, target)
        executable_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
        command = [str(binary), str(corpus), "-timeout=10",
            "-rss_limit_mb=2048", "-use_value_profile=1", f"-artifact_prefix={artifacts}/"]
        if args.engine == "libafl":
            text_target = target in {"sse", "ndjson", "tool_json", "model_json", "json_healing", "workspace_path"}
            command += [f"-runs={args.runs}", "-tui=0", "-ignore_crashes=0", "-ignore_timeouts=0",
                        "-ignore_ooms=0", "-malloc_limit_mb=256", f"-grimoire={int(text_target)}"]
        else:
            command += [f"-max_total_time={args.seconds}", f"-seed={args.seed}", "-max_len=65536", "-print_final_stats=1"]
        dictionary = FUZZ / "dictionary.dict"
        if dictionary.is_file() and args.engine == "libfuzzer":
            command.append(f"-dict={dictionary}")
        result = run(command, target_output / "search", args.seconds + 120, args.jobs,
                     sanitized=True, detect_leaks=args.detect_leaks)
        log = (target_output / "search/stderr.log").read_text(errors="replace")
        executions = re.findall(r"stat::number_of_executed_units:\s*(\d+)", log)
        result.update(target=target, executions=int(executions[-1]) if executions else None,
                      executable_sha256=executable_hash,
                      artifacts=[str(p) for p in sorted(artifacts.iterdir()) if p.is_file()],
                      corpus=str(corpus), corpus_entries=sum(p.is_file() for p in corpus.iterdir()))
        if args.engine == "libafl":
            stdout = (target_output / "search/stdout.log").read_text(errors="replace")
            monitor = libafl_execution_counts(log + stdout)
            result.update(executions=None, requested_fuzzer_stages=args.runs,
                          observed_execution_lower_bound=max(monitor, default=None))
            if result["status"] == "pass" and "inputs from disk" not in log + stdout:
                result.update(status="incomplete", reason="no LibAFL corpus-import evidence")
            elif result["status"] == "pass" and not monitor:
                result.update(status="incomplete", reason="no LibAFL execution-count evidence")
            if "Unrecognised options:" in log + stdout:
                result.update(status="incomplete", reason="LibAFL ignored unsupported options")
        elif result["status"] == "pass" and (not executions or int(executions[-1]) < 1 or "INFO: Loaded" not in log):
            result.update(status="incomplete", reason="successful exit without instrumented execution evidence")
        if "WARNING: unrecognized flag" in log:
            result.update(status="incomplete", reason="fuzzer ignored unsupported options")
        if result["artifacts"] and result["status"] == "pass":
            result.update(status="fail", reason="fuzzer retained failure artifacts")
        if hashlib.sha256(binary.read_bytes()).hexdigest() != executable_hash:
            result.update(status="incomplete", reason="fuzz executable changed during execution")
        results.append(result)
        print(json.dumps({key: result[key] for key in ("target", "status", "executions", "artifacts")}), flush=True)
    runtime_locks = []
    if args.engine == "libafl":
        for lock in sorted(args.target_dir.rglob("Cargo.lock")):
            if any("libafl" in part for part in lock.relative_to(args.target_dir).parts):
                content = lock.read_bytes()
                digest = hashlib.sha256(content).hexdigest()
                destination = args.output / ("runtime-" + digest + ".Cargo.lock")
                destination.write_bytes(content)
                runtime_locks.append({"source": str(lock), "sha256": digest, "saved": str(destination)})
    return {"status": combine(results), "results": results, "engine": args.engine,
            "budget_seconds_per_target": args.seconds, "sanitizer": args.sanitizer, "careful": args.careful,
            "runtime_locks": runtime_locks,
            "scope": "named fuzz targets only; see coverage.json for package and platform gaps"}


def replay(args):
    if args.target == "all" or not args.case or not args.case.is_file():
        raise ValueError("replay/minimize requires one --target and an existing --case")
    if args.target not in targets():
        raise ValueError("unknown fuzz target")
    if args.engine == "libafl":
        raise ValueError("use libFuzzer for exact replay/minimization of the shared target inputs")
    build = run(fuzz_command("build", args, args.target), args.output / "build", args.build_timeout, args.jobs)
    if build["status"] != "pass":
        return {"status": build["status"], "build": build}
    action = "minimize" if args.command == "minimize" else "replay"
    binary = fuzz_binary(args, args.target)
    command = [str(binary), str(args.case.resolve()), "-timeout=10", "-rss_limit_mb=2048"]
    command.append(f"-artifact_prefix={args.output}/")
    if action == "replay":
        command.append("-runs=1")
    else:
        command += ["-minimize_crash=1", f"-exact_artifact_path={args.output}/minimized", f"-max_total_time={args.seconds}"]
    result = run(command, args.output / action, args.seconds + 120, args.jobs,
                 sanitized=True, detect_leaks=args.detect_leaks)
    if action == "minimize" and result["status"] == "pass" and not (args.output / "minimized").is_file():
        result.update(status="incomplete", reason="minimizer did not retain a minimized crash input")
    return {"status": result["status"], "result": result, "input_sha256": hashlib.sha256(args.case.read_bytes()).hexdigest()}


def baseline(args):
    """Run root owners plus all public, owner-only and policy campaign entrypoints."""
    praxis = HERE / "proptest-praxis"
    results = []

    def execute(name, command, report=None):
        result = run(command, args.output / name, args.build_timeout, args.jobs)
        if report and result["status"] == "pass":
            try:
                value = json.loads(report.read_text())
                if value.get("status") != "pass":
                    raise ValueError("child report did not pass")
            except (OSError, ValueError) as error:
                result.update(status="incomplete", reason=str(error))
        results.append({"check": name, **result})
        return result["status"] == "pass"

    execute("root-tests", ["cargo", "test", "--locked", "--workspace", "--lib", "--tests",
            "--target-dir", args.target_dir, "-j", str(args.jobs)])
    execute("qualification-feature", ["cargo", "test", "--locked", "-p", "peritus-evolution",
            "--features", "qualification", "--lib", "--tests", "--target-dir", args.target_dir, "-j", str(args.jobs)])
    execute("agent-without-bridge", ["cargo", "check", "--locked", "-p", "peritus-agent",
            "--no-default-features", "--target-dir", args.target_dir, "-j", str(args.jobs)])
    execute("system-package-client", ["cargo", "check", "--locked", "-p", "peritus-cli", "-p", "peritus-launcher",
            "--features", "peritus-cli/system-package", "--target-dir", args.target_dir, "-j", str(args.jobs)])
    public = args.output / "public-build-evidence"
    if execute("public-build", [sys.executable, praxis / "campaign.py", "build", "--output", public,
                "--target-dir", args.target_dir, "--timeout", str(args.build_timeout)]):
        for action in ("examples", "search"):
            report = args.output / ("public-" + action + "-evidence")
            execute("public-" + action, [sys.executable, praxis / "campaign.py", action,
                    "--output", report, "--target-dir", args.target_dir, "--target", "all",
                    "--cases", str(args.cases), "--max-steps", "96", "--seed", str(args.seed)],
                    report / "summary.json")
    for package in ("peritus-product-runner", "peritus-daemon", "peritus-provider-openai"):
        build = args.output / (package + "-build-evidence")
        if execute(package + "-build", [sys.executable, praxis / "owner.py", "build", "--package", package,
                    "--output", build, "--target-dir", args.target_dir, "--timeout", str(args.build_timeout)]):
            receipt = build / "receipt.json"
            for action in ("examples", "search"):
                report = args.output / (package + "-" + action + "-evidence")
                execute(package + "-" + action, [sys.executable, praxis / "owner.py", action,
                        "--output", report, "--receipt", receipt, "--target", "all",
                        "--cases", str(args.cases), "--max-steps", "64", "--seed", str(args.seed)],
                        report / "summary.json")
    execute("policy-tests", ["cargo", "test", "--locked", "--manifest-path", praxis / "stage8-policy/Cargo.toml",
            "--target-dir", args.target_dir / "policy", "-j", str(args.jobs)])
    return {"status": combine(results), "results": results,
            "scope": "current-host root tests and existing public/owner/policy campaigns; no all-platform or full-product claim"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("setup", "baseline", "fuzz", "replay", "minimize"))
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory, outside source")
    parser.add_argument("--target", default="all")
    parser.add_argument("--engine", choices=("libfuzzer", "libafl"), default="libfuzzer")
    parser.add_argument("--runs", type=int, default=500, help="finite LibAFL fuzzer stages per target")
    parser.add_argument("--seconds", type=int, default=60)
    parser.add_argument("--seed", type=int, default=412679)
    parser.add_argument("--jobs", type=int, default=1)
    parser.add_argument("--cases", type=int, default=256)
    parser.add_argument("--build-timeout", type=int, default=1800)
    parser.add_argument("--sanitizer", choices=("address", "leak", "memory", "thread", "none"), default="address")
    parser.add_argument("--careful", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--extra-traces", action="store_true", help="also instrument division and index expressions")
    parser.add_argument("--detect-leaks", action="store_true",
                        help="enable LeakSanitizer; requires a host that permits its ptrace operations")
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--corpus", type=Path, help="resume a corpus, or per-target corpus folders for --target all")
    parser.add_argument("--case", type=Path)
    args = parser.parse_args()
    if min(args.seconds, args.jobs, args.build_timeout, args.cases, args.runs) < 1 or not 0 <= args.seed < 2**32:
        parser.error("positive budgets/jobs and a u32 seed are required")
    args.output = args.output.resolve()
    if args.output.is_relative_to(HERE):
        parser.error("keep generated evidence outside discovery source")
    args.output.mkdir(parents=True, exist_ok=False, mode=0o700)
    default_directory = "praxis" if args.command == "baseline" else "fuzz-libafl" if args.engine == "libafl" else "fuzz"
    args.target_dir = (args.target_dir or ROOT / "target" / default_directory).resolve()
    os.environ["PROPTEST_RNG_SEED"] = str(args.seed)
    before = fingerprint()
    save(args.output / "inputs.json", {"source": before, "tools": TOOLS, "arguments": vars(args)},)
    try:
        if args.command != "setup":
            observed = subprocess.check_output(["cargo", "fuzz", "--version"], cwd=ROOT, text=True).strip()
            if observed.split()[-1:] != [TOOLS["cargo_fuzz"]]:
                raise ValueError("cargo-fuzz version differs from tools.toml; run setup")
            compiler = subprocess.check_output(["rustc", "+" + TOOLS["fuzz_toolchain"], "--version", "--verbose"], text=True)
            clang = subprocess.check_output(["clang", "--version"], text=True)
            save(args.output / "tool-versions.json", {"cargo_fuzz": observed, "fuzz_compiler": compiler, "clang": clang})
        if args.command in {"fuzz", "replay", "minimize"}:
            manifest = HERE / ("fuzz-libafl" if args.engine == "libafl" else "fuzz") / "Cargo.toml"
            metadata = run(["cargo", "+" + TOOLS["fuzz_toolchain"], "metadata", "--locked", "--offline",
                            "--format-version=1", "--manifest-path", manifest],
                           args.output / "locked-metadata", args.build_timeout, args.jobs)
            if metadata["status"] != "pass":
                raise ValueError("fuzz dependencies are not available in the locked graph; inspect locked-metadata")
        result = (setup(args) if args.command == "setup" else baseline(args) if args.command == "baseline"
                  else fuzz(args) if args.command == "fuzz" else replay(args))
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        result = {"status": "incomplete", "error": str(error)}
    if fingerprint() != before:
        result.update(status="incomplete", source_changed=True)
    save(args.output / "summary.json", result)
    print(json.dumps({"status": result["status"], "output": str(args.output)}))
    return {"pass": 0, "fail": 1, "incomplete": 2}[result["status"]]


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(json.dumps({"status": "incomplete", "error": str(error)}), file=sys.stderr)
        sys.exit(2)
