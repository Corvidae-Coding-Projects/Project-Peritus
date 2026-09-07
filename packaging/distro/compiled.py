"""Transfer one native compilation into its same-run full package recipe.

This is a candidate-bound intermediate artifact, never a shared build cache or
release asset. Native package recipes still execute every test and packaging step.
"""

import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile
import tempfile
import time

import build
from common import ROOT, architecture, digest, maintainer, output_directory, package_build_jobs, package_format
from source import source_provenance

ARCHIVE = "compiled.tar.gz"
RECORD = "compiled.json"
MAX_ARCHIVE_BYTES = 4 * 1024**3
MAX_EXPANDED_BYTES = 12 * 1024**3
MAX_MEMBERS = 100_000
BINARIES = ("peritus", "peritusd", "peritus-tui", "peritus-linux-sandbox-helper")


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError(f"expected a regular compiled-tree file: {path}")
    return path


def current_binding(kind, jobs):
    source = source_provenance()
    image = build.image_identity(kind)
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", image):
        raise ValueError("native builder must have an observed content-addressed image identity")
    workflow = None
    if os.environ.get("GITHUB_ACTIONS") == "true":
        names = ("GITHUB_REPOSITORY", "GITHUB_RUN_ID", "GITHUB_WORKFLOW_REF", "GITHUB_REF", "GITHUB_SHA")
        workflow = {name: os.environ.get(name) for name in names}
        if not all(workflow.values()) or workflow["GITHUB_SHA"] != source["git_commit"]:
            raise ValueError("compiled-tree workflow must identify the actual checkout and run")
    return {"format": kind, "architecture": architecture(), "container_image": image,
            "maintainer": maintainer(), "cargo_build_jobs": jobs, "source": source, "workflow": workflow}


def verify_source_trees(root, binding):
    name = f"peritus-{binding['source']['version']}"
    original = root / name
    sources = [original]
    if binding["format"] == "rpm":
        # RPM versions own their build-directory layout; discover the one actual
        # unpacked source, then validate it instead of accepting an input path.
        unpacked = [path for path in (root / "BUILD").rglob(name)
                    if path.is_dir() and (path / "PACKAGE-SOURCE.json").is_file()]
        if len(unpacked) != 1:
            raise ValueError("staging requires exactly one compiled RPM source tree")
        sources.extend(unpacked)
    for source in sources:
        record = json.loads(regular(source / "PACKAGE-SOURCE.json").read_bytes())
        if record != binding["source"]:
            raise ValueError("compiled source provenance differs from its candidate binding")
        for name, expected in record["source_files_sha256"].items():
            if digest(regular(source / name)) != expected:
                raise ValueError(f"compiled source changed: {name}")
    for name in BINARIES:
        binary = sources[-1] / "target/release" / name
        if (not binary.exists() or not stat.S_ISREG(binary.lstat().st_mode)
                or not binary.stat().st_size or not binary.stat().st_mode & stat.S_IXUSR):
            raise ValueError(f"missing or invalid compiled binary: {name}")
    return original


def validate_members(entries):
    members, names, expanded = [], set(), 0
    for entry in entries:
        name = entry.name.rstrip("/") if entry.isdir() else entry.name
        path = PurePosixPath(name)
        if (not name or "\\" in name or any(ord(char) < 32 for char in name)
                or path.as_posix() != name or path.is_absolute() or ".." in path.parts
                or not path.parts or path.parts[0] != "build"
                or (len(path.parts) == 1 and not entry.isdir()) or name in names
                or not (entry.isfile() or entry.isdir()) or entry.mode & 0o7000
                or entry.size < 0 or not math.isfinite(entry.mtime) or entry.mtime < 0):
            raise ValueError(f"unsafe or duplicate compiled-tree archive member: {entry.name}")
        names.add(name)
        expanded += entry.size
        members.append(entry)
        if len(members) > MAX_MEMBERS or expanded > MAX_EXPANDED_BYTES:
            raise ValueError("compiled-tree archive exceeds its inventory or expanded-byte bound")
    if not members:
        raise ValueError("compiled-tree archive must not be empty")
    return members


def save_bundle(root, directory, binding, observation):
    verify_source_trees(root, binding)
    paths, expanded = [root], 0
    for path in root.rglob("*"):
        metadata = path.lstat()
        if not (stat.S_ISREG(metadata.st_mode) or stat.S_ISDIR(metadata.st_mode)):
            raise ValueError(f"compiled tree contains a link or special file: {path}")
        paths.append(path)
        expanded += metadata.st_size if stat.S_ISREG(metadata.st_mode) else 0
        if len(paths) > MAX_MEMBERS or expanded > MAX_EXPANDED_BYTES:
            raise ValueError("compiled tree exceeds its inventory or expanded-byte bound")
    directory.mkdir(parents=True)
    archive = directory / ARCHIVE
    count, expanded = 0, 0

    def preserve(info):
        nonlocal count, expanded
        validate_members([info])
        count += 1
        expanded += info.size
        if count > MAX_MEMBERS or expanded > MAX_EXPANDED_BYTES:
            raise ValueError("compiled tree exceeds its inventory or expanded-byte bound")
        info.uid = info.gid = 0
        info.uname = info.gname = "root"
        return info  # Preserve permissions and mtime: Cargo must see its real compiled tree.

    # Cargo hard-links several ordinary output files. Serialize their bytes as
    # regular entries; symbolic links were rejected by the lstat inventory above.
    with tarfile.open(archive, "x:gz", compresslevel=1, dereference=True) as transport:
        for path in sorted(paths):
            relative = path.relative_to(root).as_posix()
            name = "build" if relative == "." else f"build/{relative}"
            transport.add(path, arcname=name, recursive=False, filter=preserve)
    if archive.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ValueError("compiled-tree transport exceeds its compressed-byte bound")
    record = {"schema_version": 1, "kind": "native-distribution-compilation", "binding": binding,
              "compile_observation": observation, "archive_sha256": digest(archive),
              "archive_byte_length": archive.stat().st_size}
    with (directory / RECORD).open("x") as output:
        json.dump(record, output, indent=2, sort_keys=True)
        output.write("\n")
    return record


def restore_bundle(directory, destination, expected_binding):
    if {path.name for path in directory.iterdir()} != {ARCHIVE, RECORD}:
        raise ValueError("compiled-tree transport inventory must contain exactly its archive and record")
    record_file = regular(directory / RECORD)
    if record_file.stat().st_size > 8 * 1024**2:
        raise ValueError("compiled-tree record exceeds its size bound")
    record = json.loads(record_file.read_bytes())
    fields = {"schema_version", "kind", "binding", "compile_observation", "archive_sha256", "archive_byte_length"}
    if (set(record) != fields or type(record["schema_version"]) is not int
            or record["schema_version"] != 1 or record["kind"] != "native-distribution-compilation"
            or record["binding"] != expected_binding):
        raise ValueError("compiled-tree binding differs from this candidate, builder, or native run")
    observation = record["compile_observation"]
    if (not isinstance(observation, dict) or not observation.get("host") or not observation.get("invocation")
            or observation.get("cargo_build_jobs") != expected_binding["cargo_build_jobs"]
            or type(observation.get("started_unix_nanos")) is not int
            or type(observation.get("finished_unix_nanos")) is not int
            or not 0 < observation["started_unix_nanos"] <= observation["finished_unix_nanos"]):
        raise ValueError("compiled-tree observation is incomplete or invalid")
    archive = regular(directory / ARCHIVE)
    if (not 0 < archive.stat().st_size <= MAX_ARCHIVE_BYTES
            or archive.stat().st_size != record["archive_byte_length"]
            or digest(archive) != record["archive_sha256"]):
        raise ValueError("compiled-tree archive digest or length changed after compilation")
    if (destination.name != "build" or destination.exists() or destination.is_symlink()
            or any(destination.parent.iterdir())):
        raise ValueError("compiled tree requires a fresh, empty extraction parent")
    with tarfile.open(archive, "r:gz") as transport:
        members = validate_members(transport)
        transport.extractall(destination.parent, members=members, filter="data")
    verify_source_trees(destination, expected_binding)
    return record


def compile_packages():
    kind, jobs = package_format(), package_build_jobs()
    binding = current_binding(kind, jobs)
    output = ROOT / "target/distro-compiled"
    if output.exists():
        raise ValueError("refusing to overwrite an existing compiled package tree")
    (ROOT / "target").mkdir(exist_ok=True)
    started = time.time_ns()
    with tempfile.TemporaryDirectory(prefix=f"package-{kind}-compile-", dir=ROOT / "target", delete=False) as temporary:
        root = Path(temporary)
        source, epoch = build.prepare_native_source(kind, root)
        build.run_native_build(kind, root, source, epoch, jobs, "compile")
        if current_binding(kind, jobs) != binding:
            raise ValueError("candidate or builder changed while compiling packages")
        observation = build.build_observation(root, jobs, started)
        save_bundle(root, output, binding, observation)
    print(f"Retained complete native compilation in {output}")


def package_compiled():
    kind, jobs = package_format(), package_build_jobs()
    binding = current_binding(kind, jobs)
    started = time.time_ns()
    (ROOT / "target").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"package-{kind}-finish-", dir=ROOT / "target", delete=False) as temporary:
        root = Path(temporary) / "build"
        compilation = restore_bundle(ROOT / "target/distro-compiled", root, binding)
        out = output_directory(kind)
        source = root / f"peritus-{binding['source']['version']}"
        build.run_native_build(kind, root, source, binding["source"]["source_date_epoch"], jobs, "package")
        if current_binding(kind, jobs) != binding:
            raise ValueError("candidate or builder changed while finishing packages")
        build.retain_packages(kind, root, source, out, jobs, started, compilation, root.parent)
    print(f"Built and tested {kind} packages from the retained native compilation in {out}")
