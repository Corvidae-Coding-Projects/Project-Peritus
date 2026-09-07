"""Canonical native release archives using Python's standard library (3.12+).

The caller supplies the committed source epoch. Filesystem dates, owner names,
umask, traversal order, checkout path, and archive filename are not build inputs.
Only directories and regular files are accepted; executable bits are preserved.
"""

import argparse
from datetime import datetime, timezone
import gzip
from pathlib import Path
import shutil
import stat
import tarfile
import zipfile


def entries(source):
    """Validate the entire tree before creating an archive, without following links."""
    if source.is_symlink() or not source.is_dir():
        raise ValueError("archive source must be a regular directory")
    result = []

    def visit(path):
        mode = path.lstat().st_mode
        if not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
            raise ValueError(f"non-regular archive entry: {path}")
        name = path.relative_to(source.parent).as_posix()
        if "\\" in name or any(part in ("", ".", "..") for part in name.split("/")):
            raise ValueError(f"noncanonical archive entry: {name}")
        result.append((path, name, mode))
        if stat.S_ISDIR(mode):
            for child in sorted(path.iterdir()):
                visit(child)

    visit(source)
    return result


def normalized_mode(mode):
    return 0o755 if stat.S_ISDIR(mode) or mode & 0o111 else 0o644


def write_tar(tree, raw, epoch):
    with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=epoch) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
            for path, name, mode in tree:
                info = tarfile.TarInfo(name)
                info.uid = info.gid = 0
                info.uname = info.gname = "root"
                info.mtime = epoch
                info.mode = normalized_mode(mode)
                if stat.S_ISDIR(mode):
                    info.type = tarfile.DIRTYPE
                    archive.addfile(info)
                else:
                    with path.open("rb") as payload:
                        info.size = path.stat().st_size
                        archive.addfile(info, payload)


def write_zip(tree, raw, epoch):
    # DOS ZIP dates cannot represent dates before 1980 and have two-second precision.
    date = datetime.fromtimestamp(max(epoch, 315532800), timezone.utc).timetuple()[:6]
    with zipfile.ZipFile(raw, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path, name, mode in tree:
            directory = stat.S_ISDIR(mode)
            info = zipfile.ZipInfo(name + ("/" if directory else ""), date_time=date)
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = ((stat.S_IFDIR if directory else stat.S_IFREG)
                                  | normalized_mode(mode)) << 16
            if directory:
                info.external_attr |= 0x10
                archive.writestr(info, b"")
            else:
                with path.open("rb") as payload, archive.open(info, "w", force_zip64=True) as member:
                    shutil.copyfileobj(payload, member)


def archive_tree(source, destination, epoch):
    """Write a fresh canonical archive; do not mutate or timestamp the source tree."""
    source = Path(source).absolute()
    destination = Path(destination).absolute()
    if not 0 <= epoch <= 0xFFFFFFFF:
        raise ValueError("source epoch must fit an unsigned gzip timestamp")
    if destination.resolve().is_relative_to(source.resolve()):
        raise ValueError("archive destination must be outside the source tree")
    if destination.name.endswith(".tar.gz"):
        writer = write_tar
    elif destination.suffix == ".zip":
        writer = write_zip
    else:
        raise ValueError("native archive must be .tar.gz or .zip")
    tree = entries(source)
    with destination.open("xb") as raw:
        writer(tree, raw, epoch)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("epoch", type=int)
    arguments = parser.parse_args()
    archive_tree(arguments.source, arguments.destination, arguments.epoch)
