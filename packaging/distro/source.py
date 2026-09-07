"""Create a source-complete, locked, offline-buildable upstream release tarball."""

from datetime import datetime, timezone
from email.utils import format_datetime
import gzip
import json
from pathlib import Path
import shutil
import tarfile
import tomllib

from common import ROOT, digest, maintainer, run, version
from licenses import SUPPLEMENTS, debian_copyright


def prepare(build):
    source = build / f"peritus-{version()}"
    source.mkdir()
    inventory = {}
    for name, original in project_files():
        target = source / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(original, target)
        inventory[name] = digest(target)
    provenance = source_provenance(inventory)
    epoch = provenance["source_date_epoch"]
    (source / "PACKAGE-SOURCE.json").write_text(json.dumps(provenance, indent=2) + "\n")
    config = run("cargo", "vendor", "--locked", "--versioned-dirs", "vendor",
                 cwd=source, capture=True)
    (source / ".cargo" / "vendor.toml").write_text(config + "\n")
    notices(source)
    archive = build / f"peritus-{version()}.tar.gz"
    archive_source(source, archive, epoch)
    return source, archive, epoch


def project_files():
    """Select the same source inventory for initial builds and staged admission."""
    files = run("git", "ls-files", "--cached", "--others", "--exclude-standard", "-z",
                cwd=ROOT, capture=True).split("\0")
    excluded = {".crosslink", ".claude", ".codex", ".agents", ".git", ".worktrees"}
    for name in sorted(set(files)):
        relative = Path(name)
        if not name or relative.parts[0] in excluded or name in ("AGENTS.md", ".mcp.json"):
            continue
        original = ROOT / relative
        if original.is_symlink() or not original.is_file():
            raise ValueError(f"source must be a regular file: {name}")
        yield name, original


def source_provenance(inventory=None):
    if inventory is None:
        inventory = {name: digest(path) for name, path in project_files()}
    return {
        "git_commit": run("git", "rev-parse", "HEAD", cwd=ROOT, capture=True),
        "source_date_epoch": int(run("git", "log", "-1", "--format=%ct", cwd=ROOT, capture=True)),
        "version": version(),
        "source_files_sha256": inventory,
    }


def archive_source(source, archive, epoch):
    def normalize(info):
        info.uid = info.gid = 0
        info.uname = info.gname = "root"
        info.mtime = epoch
        if not (info.isfile() or info.isdir()):
            raise ValueError(f"non-regular source entry: {info.name}")
        info.mode = 0o755 if info.isdir() or info.mode & 0o111 else 0o644
        return info

    with archive.open("wb") as raw:
        with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as tar:
                tar.add(source, arcname=source.name, filter=normalize)


def notices(source):
    sections = ["Peritus vendored dependency license notices\n"]
    sections.append((source / "packaging/licenses/README.md").read_text())
    for crate in sorted((source / "vendor").iterdir()):
        manifest = tomllib.loads((crate / "Cargo.toml").read_text())["package"]
        sections.append(f"\n{manifest['name']} {manifest['version']}\n"
                        f"Declared license: {manifest.get('license', 'see license file')}\n"
                        f"Declared authors: {', '.join(manifest.get('authors', []))}\n")
        candidates = set()
        for path in crate.rglob("*"):
            if path.is_file() and any(token in path.name.upper()
                                      for token in ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE")):
                candidates.add(path)
        if "license-file" in manifest:
            candidates.add(crate / manifest["license-file"])
        candidates.update(source / path for path in SUPPLEMENTS.get(crate.name, []))
        if not candidates:
            raise ValueError(f"vendored dependency has no redistributable license notice: {crate.name}")
        for path in sorted(candidates):
            if not path.resolve().is_relative_to(source.resolve()) or path.is_symlink():
                raise ValueError(f"license path escapes its source bundle: {path}")
            sections.append(f"\n--- {path.relative_to(source)} ---\n{path.read_text(errors='replace')}\n")
    (source / "THIRD-PARTY-NOTICES.txt").write_text("".join(sections))


def debian_metadata(source, epoch):
    directory = source / "debian"
    shutil.copytree(source / "packaging" / "debian", directory)
    control = directory / "control.in"
    (directory / "control").write_text(control.read_text().replace("@MAINTAINER@", maintainer()))
    control.unlink()
    (directory / "changelog").write_text(
        f"peritus ({version()}-1) unstable; urgency=medium\n\n"
        "  * Build the upstream release with locked, vendored dependencies.\n\n"
        f" -- {maintainer()}  {format_datetime(datetime.fromtimestamp(epoch, timezone.utc))}\n"
    )
    (directory / "copyright").write_text(
        "Upstream: https://github.com/Corvidae-Coding-Projects/Project-Peritus\n\n"
        + (source / "LICENSE").read_text() + "\n"
        + debian_copyright((source / "THIRD-PARTY-NOTICES.txt").read_text())
    )
    linker_script_overrides(source, directory)


def linker_script_overrides(source, directory):
    """Lintian misidentifies upstream Windows ld INPUT scripts ending in .a as ar archives."""
    lines = ["# Exact upstream text linker scripts, not corrupt archives.\n"]
    for architecture in ("i686", "x86_64"):
        crate = source / "vendor" / f"winapi-{architecture}-pc-windows-gnu-0.4.0"
        for path in sorted((crate / "lib").glob("*.a")):
            with path.open("rb") as stream:
                prefix = stream.read(7)
            if prefix != b"INPUT(\n":
                continue
            contents = path.read_text()
            if not contents.rstrip().endswith(")"):
                raise ValueError(f"malformed upstream Windows linker script: {path.name}")
            relative = path.relative_to(source)
            lines.append(f"peritus source: unpack-message-for-orig peritus_*.orig.tar.gz . "
                         f"ar failed for peritus-*/{relative}\n")
    (directory / "source" / "lintian-overrides").write_text("".join(lines))
