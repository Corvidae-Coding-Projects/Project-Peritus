"""Restore only the checksum-verified release archive for native qualification.

Loose files from an Actions download are not executable release evidence. This
adapter validates every archive member before materializing a fresh package tree
and preserves the original archive and checksum beside it. It never rebuilds.
"""

import argparse
import hashlib
from pathlib import Path, PurePosixPath
import shutil
import stat
import tarfile
import tempfile
import zipfile


def regular(path):
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError(f"required regular file: {path}")
    return path


def validated_members(archive, package, zipped):
    members = archive.infolist() if zipped else archive.getmembers()
    if not 1 <= len(members) <= 10000:
        raise ValueError("release archive member count is outside the bound")
    result = []
    names = set()
    total = 0
    for member in members:
        raw = member.filename if zipped else member.name
        name = raw.removesuffix("/")
        path = PurePosixPath(name)
        if (not name or "\\" in name or ":" in name
                or any(ord(character) < 32 for character in name)
                or any(part in ("", ".", "..") for part in name.split("/"))
                or path.parts[0] != package or name in names):
            raise ValueError(f"noncanonical or duplicate release member: {raw}")
        directory = member.is_dir() if zipped else member.isdir()
        mode = member.external_attr >> 16 if zipped else member.mode
        is_file = stat.S_ISREG(mode) if zipped else member.isfile()
        is_directory = stat.S_ISDIR(mode) if zipped else directory
        if not ((directory and is_directory) or (not directory and is_file)):
            raise ValueError(f"non-regular release member: {raw}")
        if mode & 0o7777 != (0o755 if directory or mode & 0o111 else 0o644):
            raise ValueError(f"noncanonical release permissions: {raw}")
        size = member.file_size if zipped else member.size
        total += size
        if size < 0 or size > 512 * 1024 * 1024 or total > 2 * 1024**3:
            raise ValueError("release archive payload exceeds the bound")
        if directory and size != 0:
            raise ValueError(f"nonempty directory member: {raw}")
        if name == package:
            if not directory:
                raise ValueError("release archive root must be a directory")
        elif str(path.parent) not in names:
            raise ValueError(f"release member precedes its parent: {raw}")
        names.add(name)
        result.append((member, name, directory, mode & 0o777))
    if f"{package}/manifest.toml" not in names:
        raise ValueError("release archive lacks its package manifest")
    return result


def extract_checked(source, destination, package):
    zipped = source.suffix == ".zip"
    opener = zipfile.ZipFile if zipped else tarfile.open
    with opener(source, "r") as archive:
        members = validated_members(archive, package, zipped)
        for member, name, directory, mode in members:
            target = destination / name
            if directory:
                target.mkdir()
            else:
                payload = archive.open(member) if zipped else archive.extractfile(member)
                with payload, target.open("xb") as output:
                    shutil.copyfileobj(payload, output)
            target.chmod(mode)


def prepare(source, root, package):
    if (not package.startswith("peritus-") or "/" in package or "\\" in package
            or ":" in package or package in (".", "..")):
        raise ValueError("expected a single native package directory name")
    extension = ".zip" if package.startswith("peritus-windows-") else ".tar.gz"
    archive = regular(source / f"{package}{extension}")
    checksum = regular(source / f"{package}{extension}.sha256")
    expected = checksum.read_bytes()
    with archive.open("rb") as payload:
        actual = hashlib.file_digest(payload, "sha256").hexdigest().encode() + b"\n"
    if expected != actual:
        raise ValueError("release archive checksum mismatch")
    destination = root / "dist"
    if destination.exists() or destination.is_symlink():
        raise ValueError("qualification destination must be fresh")
    target = root / "target"
    target.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="release-qualification-", dir=target) as scratch:
        staged = Path(scratch) / "dist"
        staged.mkdir()
        extract_checked(archive, staged, package)
        shutil.copyfile(archive, staged / archive.name)
        shutil.copyfile(checksum, staged / checksum.name)
        staged.rename(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("root", type=Path)
    parser.add_argument("package")
    arguments = parser.parse_args()
    prepare(arguments.source, arguments.root, arguments.package)
