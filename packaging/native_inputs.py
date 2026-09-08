"""Observed inputs for native daemon libraries and their same-run binary consumers."""

import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import time
import tomllib
import uuid

from native_transport import binary_package, digest, validate_later, validate_observation, validate_record
import windows_release

ROOT = Path(__file__).resolve().parent.parent
ROLE_ENV = "PERITUS_RELEASE_BUILD_ROLE"
EXCLUDED = {".crosslink", ".claude", ".codex", ".agents", ".git", ".worktrees"}
WORKFLOW_KEYS = ("GITHUB_REPOSITORY", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT",
                 "GITHUB_SHA", "GITHUB_REF", "GITHUB_WORKFLOW_REF")


def command(*arguments):
    return subprocess.check_output(arguments, cwd=ROOT, text=True).strip()


def source_files():
    names = command("git", "ls-files", "--cached", "--others", "--exclude-standard", "-z")
    result = {}
    for name in sorted(set(names.split("\0"))):
        if not name or Path(name).parts[0] in EXCLUDED or name in ("AGENTS.md", ".mcp.json"):
            continue
        path = ROOT / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"native build source must be a regular file: {name}")
        result[name] = digest(path)
    if not result:
        raise ValueError("native build source inventory is empty")
    return result


def candidate():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    head = command("git", "rev-parse", "HEAD")
    if os.environ.get("GITHUB_ACTIONS") == "true":
        if os.environ.get("GITHUB_SHA") != head:
            raise ValueError("native compilation checkout differs from its workflow candidate")
        subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], cwd=ROOT, check=True)
    return {"git_commit": head, "source_files_sha256": source_files(),
            "source_date_epoch": int(command("git", "log", "-1", "--format=%ct")),
            "version": manifest["workspace"]["package"]["version"],
            "release_profile": manifest["profile"]["release"]}


def environment():
    if platform.system() not in ("Darwin", "Linux", "Windows"):
        raise ValueError("unsupported native library build host")
    forbidden = {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS",
                 "RUSTC", "CARGO_BUILD_RUSTC", "RUSTC_BOOTSTRAP",
                 "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET"}
    if any(value and (key in forbidden or key.startswith("CARGO_PROFILE_")
                      or (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS")))
           for key, value in os.environ.items()):
        raise ValueError("native release compilation does not accept profile, flag, target, or wrapper overrides")
    if os.environ.get("CARGO_INCREMENTAL", "0") != "0":
        raise ValueError("native release compilation must keep incremental compilation disabled")
    jobs = os.environ.get("CARGO_BUILD_JOBS", "2")
    if jobs not in ("1", "2", "3", "4"):
        raise ValueError("native release compilation requires one through four build jobs")
    compiler = windows_release.compiler_environment()[1] if platform.system() == "Windows" else None
    result = {"system": platform.system(), "machine": platform.machine(),
              "release": platform.release(), "version": platform.version(),
              "rustc": command("rustc", "--version", "--verbose"),
              "rust_sysroot": command("rustc", "--print", "sysroot"),
              "cargo": command("cargo", "--version"),
              "cc": compiler if compiler is not None else command("cc", "--version"),
              "image_os": os.environ.get("ImageOS"), "image_version": os.environ.get("ImageVersion"),
              "cargo_build_jobs": int(jobs), "checkout": str(ROOT.resolve()),
              "cargo_home": str(Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")).resolve()),
              "compiler_environment": {key: os.environ.get(key) for key in (
                  "CC", "CXX", "AR", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS",
                  "SDKROOT", "MACOSX_DEPLOYMENT_TARGET", "CARGO_BUILD_TARGET")}}
    if result["system"] == "Darwin":
        result["sdk_path"] = command("xcrun", "--show-sdk-path")
        result["sdk_version"] = command("xcrun", "--show-sdk-version")
    return result


def binding(binary="peritusd"):
    package = binary_package(binary)
    role = os.environ.get(ROLE_ENV)
    if role not in ("primary", "independent"):
        raise ValueError("native release build role must be primary or independent")
    workflow = None
    if os.environ.get("GITHUB_ACTIONS") == "true":
        workflow = {key: os.environ.get(key) for key in WORKFLOW_KEYS}
        if not all(workflow.values()):
            raise ValueError("native compilation requires a complete same-run workflow binding")
    return {"candidate": candidate(), "environment": environment(), "role": role,
            "package": package, "binary": binary, "workflow": workflow}


def library_binding(consumer):
    """The final binary consumes its own package's observed library compilation."""
    return dict(consumer)


def daemon_binding(consumer):
    """The CLI library stage starts with this role's original daemon libraries."""
    return dict(consumer, package="peritus-daemon", binary="peritusd")


def normalize_verified_sources(expected):
    """Only timestamp bytes already verified equal to this compilation's source."""
    if candidate() != expected:
        raise ValueError("native compilation source changed before timestamp preparation")
    epoch = expected["source_date_epoch"] * 1_000_000_000
    for name in expected["source_files_sha256"]:
        os.utime(ROOT / name, ns=(epoch, epoch), follow_symlinks=False)


def observation(started, arguments):
    return {"host": platform.node(), "invocation": str(uuid.uuid4()),
            "started_unix_nanos": started, "finished_unix_nanos": time.time_ns(),
            "command": arguments, "workflow_job": os.environ.get("GITHUB_JOB")}


def validate_binary_record(record, expected_candidate, role, binary, binary_name="peritusd"):
    if not stat.S_ISREG(binary.lstat().st_mode) or not binary.stat().st_size:
        raise ValueError("native product must be a nonempty regular file")
    observed = {"sha256": digest(binary), "byte_length": binary.stat().st_size}
    validate_binary_observation(record, expected_candidate, role, observed, binary_name)


def validate_binary_observation(record, expected_candidate, role, observed, binary_name="peritusd"):
    package = binary_package(binary_name)
    if (not isinstance(record, dict) or set(record) != {
            "schema_version", "kind", "binding", "library", "observation", "binary"}
            or type(record["schema_version"]) is not int or record["schema_version"] != 3
            or record["kind"] != "native-release-binary-compilation"):
        raise ValueError("invalid native binary compilation record")
    bound = record["binding"]
    if (not isinstance(bound, dict) or set(bound) != {
            "candidate", "environment", "role", "package", "binary", "workflow"}
            or bound["candidate"] != expected_candidate or bound["role"] != role
            or bound["package"] != package or bound["binary"] != binary_name):
        raise ValueError("native binary compilation candidate, role, or consumer differs")
    validate_record(record["library"], library_binding(bound))
    validate_observation(record["observation"], "binary", binary_name, bound["environment"].get("system"))
    first, last = record["library"]["observation"], record["observation"]
    validate_later(first, last)
    if record["library"]["previous_library"] is not None:
        validate_later(record["library"]["previous_library"]["observation"], last)
    if record["binary"] != observed:
        raise ValueError("native binary compilation record differs from its product bytes")


def write_record(path, record):
    with path.open("x", newline="\n") as output:
        json.dump(record, output, indent=2, sort_keys=True)
        output.write("\n")
