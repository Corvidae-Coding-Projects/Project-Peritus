"""Bounded, same-candidate transport of native daemon libraries, never a binary cache."""

import hashlib
import json
import math
from pathlib import PurePosixPath
import re
import stat
import tarfile

import archive as native_archive

ARCHIVE = "libraries.tar.gz"
RECORD = "libraries.json"
MAX_ARCHIVE_BYTES = 2 * 1024**3
MAX_EXPANDED_BYTES = 8 * 1024**3
MAX_MEMBERS = 100_000
MAX_RECORD_BYTES = 4 * 1024**2


def cargo_arguments(stage):
    if stage not in ("library", "binary"):
        raise ValueError("native daemon phase must be library or binary")
    return ["cargo", "build", "--release", "--locked", "--package", "peritus-daemon",
            *(["--lib"] if stage == "library" else ["--bin", "peritusd"])]


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError(f"native compilation requires a regular file: {path}")
    return path


def digest(path):
    with regular(path).open("rb") as payload:
        return hashlib.file_digest(payload, "sha256").hexdigest()


def members(entries):
    result, names, expanded = [], set(), 0
    for entry in entries:
        name = entry.name.rstrip("/") if entry.isdir() else entry.name
        path = PurePosixPath(name)
        if (not name or "\\" in name or any(ord(char) < 32 for char in name)
                or path.as_posix() != name or path.is_absolute() or ".." in path.parts
                or path.parts[0] != "native-daemon" or name in names
                or (len(path.parts) == 1 and not entry.isdir())
                or not (entry.isfile() or entry.isdir()) or entry.mode & 0o7000
                or entry.size < 0 or not math.isfinite(entry.mtime) or entry.mtime < 0):
            raise ValueError(f"unsafe or duplicate native library archive member: {entry.name}")
        names.add(name)
        expanded += entry.size
        result.append(entry)
        if len(result) > MAX_MEMBERS or expanded > MAX_EXPANDED_BYTES:
            raise ValueError("native library archive exceeds its resource bound")
    if not result:
        raise ValueError("native library archive must not be empty")
    return result


def library_tree(root):
    if root.name != "native-daemon":
        raise ValueError("native libraries require their fixed target directory")
    tree = native_archive.entries(root)
    if len(tree) > MAX_MEMBERS:
        raise ValueError("native library tree exceeds its inventory bound")
    expanded = 0
    for path, _, mode in tree:
        if mode & 0o7000:
            raise ValueError("native library tree contains special permissions")
        if stat.S_ISREG(mode):
            expanded += path.stat().st_size
        if (path.name == "peritusd" or path.name.startswith("peritusd-")
                or path.name.startswith("bin-peritusd")):
            raise ValueError("library compilation cannot contain a prebuilt product binary")
    if expanded > MAX_EXPANDED_BYTES:
        raise ValueError("native library tree exceeds its expanded-byte bound")
    library = root / "release/libperitus_daemon.rlib"
    candidates = list((root / "release/deps").glob("libperitus_daemon-*.rlib"))
    if not library.is_file() or not regular(library).stat().st_size or len(candidates) != 1:
        raise ValueError("native compilation requires exactly one complete daemon library")
    if not regular(candidates[0]).stat().st_size:
        raise ValueError("native daemon dependency library is empty")
    return tree


def validate_observation(observation, stage):
    if (not isinstance(observation, dict) or not observation.get("host")
            or not observation.get("invocation")
            or type(observation.get("started_unix_nanos")) is not int
            or type(observation.get("finished_unix_nanos")) is not int
            or not 0 < observation["started_unix_nanos"] <= observation["finished_unix_nanos"]
            or observation.get("command") != cargo_arguments(stage)):
        raise ValueError("native compilation observation is incomplete or unordered")


def validate_record(record, expected):
    if (not isinstance(record, dict) or set(record) != {
            "schema_version", "kind", "binding", "observation",
            "archive_byte_length", "archive_sha256"}
            or type(record["schema_version"]) is not int or record["schema_version"] != 1
            or record["kind"] != "native-daemon-library-compilation"
            or record["binding"] != expected):
        raise ValueError("native library binding differs from this candidate, role, or environment")
    if (type(record["archive_byte_length"]) is not int
            or not 0 < record["archive_byte_length"] <= MAX_ARCHIVE_BYTES
            or not isinstance(record["archive_sha256"], str)
            or not re.fullmatch(r"[0-9a-f]{64}", record["archive_sha256"])):
        raise ValueError("native library archive identity is invalid")
    validate_observation(record["observation"], "library")


def save(root, directory, binding, observation):
    tree = library_tree(root)
    validate_observation(observation, "library")
    directory.mkdir(parents=True)
    archive = directory / ARCHIVE
    with tarfile.open(archive, "x:gz", compresslevel=1, dereference=True) as output:
        for path, name, _ in tree:
            info = output.gettarinfo(path, arcname=name)
            members([info])
            info.uid = info.gid = 0
            info.uname = info.gname = "root"
            if info.isfile():
                with regular(path).open("rb") as payload:
                    output.addfile(info, payload)
            else:
                output.addfile(info)
    record = {"schema_version": 1, "kind": "native-daemon-library-compilation",
              "binding": binding, "observation": observation,
              "archive_byte_length": archive.stat().st_size, "archive_sha256": digest(archive)}
    validate_record(record, binding)
    with (directory / RECORD).open("x", newline="\n") as output:
        json.dump(record, output, indent=2, sort_keys=True)
        output.write("\n")
    return record


def restore(directory, destination, expected):
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("native library bundle requires a regular directory")
    if {path.name for path in directory.iterdir()} != {ARCHIVE, RECORD}:
        raise ValueError("native library transport inventory is not closed")
    record_file = regular(directory / RECORD)
    if record_file.stat().st_size > MAX_RECORD_BYTES:
        raise ValueError("native library record exceeds its size bound")
    record = json.loads(record_file.read_bytes())
    validate_record(record, expected)
    archive = regular(directory / ARCHIVE)
    if (archive.stat().st_size != record["archive_byte_length"]
            or digest(archive) != record["archive_sha256"]):
        raise ValueError("native library archive length or digest changed")
    if (destination.name != "native-daemon" or destination.exists() or destination.is_symlink()
            or destination.parent.is_symlink() or not destination.parent.is_dir()):
        raise ValueError("native libraries require a fresh fixed target directory")
    with tarfile.open(archive, "r:gz") as source:
        inventory = members(source)
        source.extractall(destination.parent, members=inventory, filter="data")
    library_tree(destination)
    return record
