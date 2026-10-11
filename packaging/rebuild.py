"""Retain real assembly observations and compare the complete native output inventory.

The native release operator defines its primary outputs as archive plus checksum.
Observed times, runner identities, and later signatures are separate evidence,
not distribution bytes to rewrite for reproducibility. Neither command compiles.
"""

import argparse
import filecmp
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import stat
import subprocess
import sys
import tarfile
import time
import tomllib
import uuid
import zlib
import zipfile

import native_inputs
import native_transport

ROOT = Path(__file__).resolve().parent.parent
OBSERVATION = "peritus-native-build.json"
PACKAGE_RECORD = "peritus-native-package.json"
INTEL_MAC_ARCHIVE = "dist/peritus-macos-x86_64.tar.gz"
WINDOWS_ARCHIVE = "dist/peritus-windows-x86_64.zip"


def compilation_names(package):
    target = package["archive"].replace("\\", "/")
    if target == INTEL_MAC_ARCHIVE:
        return set(native_transport.BINARY_PACKAGES)
    return {"peritusd"} if target == WINDOWS_ARCHIVE else set()


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError(f"required regular build output: {path}")
    return path


def digest(path):
    with regular(path).open("rb") as payload:
        return hashlib.file_digest(payload, "sha256").hexdigest()


def outputs(directory):
    record = json.loads(regular(directory / PACKAGE_RECORD).read_bytes())
    if (set(record) != {"schema_version", "archive", "checksum", "build_started_unix",
                       "build_finished_unix"} or record["schema_version"] != 1
            or not 0 < record["build_started_unix"] <= record["build_finished_unix"]):
        raise ValueError("invalid native package record")
    archive = record["archive"].replace("\\", "/")
    if not re.fullmatch(r"dist/peritus-(?:linux|macos)-(?:x86_64|aarch64)\.tar\.gz|"
                        r"dist/peritus-windows-(?:x86_64|aarch64)\.zip", archive):
        raise ValueError("native output archive path is not canonical")
    checksum = record["checksum"].replace("\\", "/")
    if checksum != archive + ".sha256":
        raise ValueError("native checksum path does not match its archive")
    names = [PurePosixPath(path).name for path in (archive, checksum)]
    observed = [{"path": path, "byte_length": regular(directory / name).stat().st_size,
                 "sha256": digest(directory / name)}
                for path, name in zip((archive, checksum), names)]
    if (directory / names[1]).read_bytes() != observed[0]["sha256"].encode() + b"\n":
        raise ValueError("native output checksum mismatch")
    package = names[0].removesuffix(".tar.gz").removesuffix(".zip")
    for path in directory.iterdir():
        if path.name == package and stat.S_ISDIR(path.lstat().st_mode):
            continue  # Loose package projection; H2 restores only the verified archive.
        if path.name not in {*names, PACKAGE_RECORD, OBSERVATION}:
            raise ValueError(f"unaccounted native build output: {path.name}")
        regular(path)
    return record, observed


def command(*arguments):
    return subprocess.check_output(arguments, cwd=ROOT, text=True).strip()


def candidate():
    head = command("git", "rev-parse", "HEAD")
    if os.environ.get("GITHUB_SHA", head) != head:
        raise ValueError("build checkout differs from the workflow candidate")
    subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], cwd=ROOT, check=True)
    with subprocess.Popen(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT,
                          stdout=subprocess.PIPE) as process:
        source = hashlib.file_digest(process.stdout, "sha256").hexdigest()
        if process.wait() != 0:
            raise ValueError("cannot retain the exact source-tree hash")
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    return {"candidate_commit": head, "source_tree_digest": source,
            "version": manifest["workspace"]["package"]["version"],
            "toolchain": manifest["workspace"]["metadata"]["peritus"],
            "release_profile": manifest["profile"]["release"]}


def record(directory, role):
    if role not in ("primary", "independent"):
        raise ValueError("build role must be primary or independent")
    package, artifacts = outputs(directory)
    compilations = binary_compilations(directory, package, role)
    observation = {
        "schema_version": 4, "kind": "native-release-assembly", "role": role,
        "invocation": str(uuid.uuid4()), "observed_unix_nanos": time.time_ns(),
        "candidate": candidate(), "artifacts": artifacts, "package_record": package,
        "binary_compilations": compilations,
        "host": platform.node(),
        "environment": {
            "system": platform.system(), "release": platform.release(),
            "version": platform.version(), "machine": platform.machine(),
            "rustc": command("rustc", "--version", "--verbose"),
            "python": sys.version, "zlib": zlib.ZLIB_RUNTIME_VERSION,
            "image_os": os.environ.get("ImageOS"),
            "image_version": os.environ.get("ImageVersion"),
            "runner_os": os.environ.get("RUNNER_OS"),
            "runner_arch": os.environ.get("RUNNER_ARCH"),
        },
        "workflow": {key: os.environ.get(key) for key in (
            "GITHUB_REPOSITORY", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB",
            "GITHUB_SHA", "GITHUB_REF", "RUNNER_NAME")},
    }
    with (directory / OBSERVATION).open("x", newline="\n") as output:
        json.dump(observation, output, indent=2)
        output.write("\n")


def load(directory, role):
    observation = json.loads(regular(directory / OBSERVATION).read_bytes())
    package, artifacts = outputs(directory)
    if (observation["schema_version"] != 4 or observation["kind"] != "native-release-assembly"
            or observation["role"] != role or observation["artifacts"] != artifacts
            or observation["package_record"] != package):
        raise ValueError("native build observation does not match its retained outputs")
    validate_binary_compilations(observation.get("binary_compilations"), directory, package, role)
    return observation


def binary_compilations(directory, package, role):
    source = ROOT / "target/native-compile-record"
    required = compilation_names(package)
    if not required:
        if source.exists() or source.is_symlink():
            raise ValueError("unexpected binary compilation evidence on this native target")
        return {}
    names = {binary: native_transport.record_filename(binary)
             for binary in required}
    if (source.is_symlink() or not source.is_dir()
            or {path.name for path in source.iterdir()} != set(names.values())):
        raise ValueError("native compilation evidence requires exactly this target's staged binary records")
    records = {}
    for binary, name in names.items():
        path = regular(source / name)
        if path.stat().st_size > native_transport.MAX_RECORD_BYTES:
            raise ValueError("native binary compilation evidence exceeds its size bound")
        records[binary] = json.loads(path.read_bytes())
    validate_binary_compilations(records, directory, package, role)
    return records


def validate_binary_compilations(records, directory, package, role):
    names = compilation_names(package)
    if not isinstance(records, dict) or set(records) != names:
        raise ValueError("native binary compilation inventory differs from this archive target")
    if not names:
        return
    archive_path = package["archive"].replace("\\", "/")
    windows = archive_path == WINDOWS_ARCHIVE
    observed = archived_binaries(directory / PurePosixPath(archive_path).name, windows)
    candidate_inputs = native_inputs.candidate()
    rustc = command("rustc", "--version", "--verbose")
    for name, record in records.items():
        native_inputs.validate_binary_observation(record, candidate_inputs, role, observed[name], name)
        validate_compilation_environment(record["binding"], rustc, windows)
    if not windows:
        daemon, cli = records["peritusd"], records["peritus"]
        if (daemon["library"] != cli["library"]["previous_library"]
                or daemon["observation"]["invocation"] in (
                    cli["observation"]["invocation"], cli["library"]["observation"]["invocation"])):
            raise ValueError("native CLI and daemon must have distinct invocations from the same role's library")


def validate_compilation_environment(bound, rustc, windows=False):
    machines = ("amd64", "x86_64") if windows else ("x86_64",)
    if (bound["environment"]["system"] != ("Windows" if windows else "Darwin")
            or bound["environment"]["machine"].lower() not in machines
            or bound["environment"]["rustc"] != rustc
            or bound["environment"]["image_version"] != os.environ.get("ImageVersion")):
        raise ValueError("native binary compilation environment differs from native assembly")
    if windows and bound["environment"]["cc"] != native_inputs.windows_release.compiler_environment()[1]:
        raise ValueError("native Windows compilation C compiler differs from native assembly")
    if os.environ.get("GITHUB_ACTIONS") == "true":
        workflow = {key: os.environ.get(key) for key in native_inputs.WORKFLOW_KEYS}
        if not all(workflow.values()) or bound["workflow"] != workflow:
            raise ValueError("native binary compilation differs from this assembly's workflow run")


def archived_binaries(archive, windows=False):
    """Inspect the shipped bytes, never the untrusted loose package projection."""
    if windows:
        return archived_windows_daemon(archive)
    wanted = {f"peritus-macos-x86_64/bin/{name}": name for name in native_transport.BINARY_PACKAGES}
    result, expanded, count = {}, 0, 0
    with tarfile.open(regular(archive), "r:gz") as source:
        for entry in source:
            count += 1
            expanded += entry.size
            if count > 10_000 or expanded > 1024**3 or entry.size < 0:
                raise ValueError("native archive exceeds its inspection bound")
            if entry.name not in wanted:
                continue
            name = wanted[entry.name]
            if name in result or not entry.isfile() or not 0 < entry.size <= 256 * 1024**2:
                raise ValueError("native archive requires one nonempty regular entry for each binary")
            with source.extractfile(entry) as binary:
                result[name] = {"byte_length": entry.size,
                                "sha256": hashlib.file_digest(binary, "sha256").hexdigest()}
    if set(result) != set(wanted.values()):
        raise ValueError("native archive is missing a compiled CLI or daemon")
    return result


def archived_windows_daemon(archive):
    wanted = "peritus-windows-x86_64/bin/peritusd.exe"
    with zipfile.ZipFile(regular(archive)) as source:
        entries = source.infolist()
        if len(entries) > 10_000 or sum(entry.file_size for entry in entries) > 1024**3:
            raise ValueError("native archive exceeds its inspection bound")
        products = [entry for entry in entries if entry.filename == wanted]
        if len(products) != 1:
            raise ValueError("native archive requires exactly one Windows daemon")
        entry = products[0]
        mode = entry.external_attr >> 16
        if (entry.is_dir() or stat.S_IFMT(mode) not in (0, stat.S_IFREG)
                or not 0 < entry.file_size <= 256 * 1024**2):
            raise ValueError("native archive requires one nonempty regular Windows daemon")
        with source.open(entry) as binary:
            digest_value = hashlib.file_digest(binary, "sha256").hexdigest()
        return {"peritusd": {"byte_length": entry.file_size, "sha256": digest_value}}


def compare(first, second, report):
    if first.resolve() == second.resolve():
        raise ValueError("independent build must use a different output directory")
    primary, independent = load(first, "primary"), load(second, "independent")
    if primary["invocation"] == independent["invocation"]:
        raise ValueError("independent build must use a different invocation")
    differences = []
    for field in ("candidate", "environment"):
        if primary[field] != independent[field]:
            differences.append({"kind": f"incompatible-{field}"})
    first_files = {entry["path"]: entry for entry in primary["artifacts"]}
    second_files = {entry["path"]: entry for entry in independent["artifacts"]}
    for path in sorted(first_files.keys() | second_files.keys()):
        if path not in first_files or path not in second_files:
            differences.append({"path": path, "kind": "missing-output"})
        elif (first_files[path] != second_files[path]
              or not filecmp.cmp(first / PurePosixPath(path).name,
                                 second / PurePosixPath(path).name, shallow=False)):
            differences.append({"path": path, "kind": "content-mismatch"})
    result = {"schema_version": 1, "kind": "native-independent-rebuild",
              "compatible": primary["candidate"] == independent["candidate"]
              and primary["environment"] == independent["environment"],
              "byte_identical": not any("path" in difference for difference in differences),
              "reproducible": not differences, "differences": differences,
              "primary": primary, "independent": independent}
    report.parent.mkdir(parents=True, exist_ok=True)
    with report.open("x", newline="\n") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    if differences:
        raise ValueError(f"independent native rebuild differs; retained report: {report}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="operation", required=True)
    recording = commands.add_parser("record")
    recording.add_argument("directory", type=Path)
    recording.add_argument("role", choices=("primary", "independent"))
    comparison = commands.add_parser("compare")
    comparison.add_argument("primary", type=Path)
    comparison.add_argument("independent", type=Path)
    comparison.add_argument("report", type=Path)
    arguments = parser.parse_args()
    if arguments.operation == "record":
        record(arguments.directory, arguments.role)
    else:
        compare(arguments.primary, arguments.independent, arguments.report)
