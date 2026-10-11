"""Container-side native package signing and Debian checksum closure."""

from pathlib import Path
import json
import re
import shutil
import sys

from common import digest, run
from source import archive_source


def refresh_checksums(path):
    """Refresh every Debian checksum section after embedded package signatures change bytes."""
    section = None
    algorithms = {"Files": "md5", "Checksums-Sha1": "sha1", "Checksums-Sha256": "sha256"}
    lines = []
    for line in path.read_text().splitlines():
        if line and not line.startswith(" "):
            section = line.partition(":")[0]
        elif line.startswith(" ") and section in algorithms:
            fields = line.split()
            name = fields[-1]
            if Path(name).name != name or len(fields) not in (3, 5):
                raise ValueError(f"unsafe Debian checksum record in {path.name}")
            payload = path.parent / name
            fields[0] = digest(payload, algorithms[section])
            fields[1] = str(payload.stat().st_size)
            line = " " + " ".join(fields)
        lines.append(line)
    path.write_text("\n".join(lines) + "\n")


def clearsign(path, key):
    temporary = path.with_name(path.name + ".signed")
    run("gpg", "--batch", "--local-user", key, "--digest-algo", "SHA256",
        "--output", temporary, "--clearsign", path)
    temporary.replace(path)


def main():
    kind, key = sys.argv[1:]
    if kind not in ("deb", "rpm") or not re.fullmatch(r"[A-F0-9]{40}", key):
        raise ValueError("invalid signing parameters")
    root = Path("/packages")
    run("gpg", "--batch", "--import", "/release-key.asc")
    if kind == "deb":
        packages = sorted(root.glob("*.deb"))
        if not packages:
            raise ValueError("no Debian packages to sign")
        for package in packages:
            run("debsigs", "--sign=origin", f"--default-key={key}", package)
        for suffix in ("dsc", "buildinfo", "changes"):
            files = sorted(root.glob(f"*.{suffix}"))
            if not files:
                raise ValueError(f"missing Debian {suffix} metadata")
            for path in files:
                refresh_checksums(path)
                clearsign(path, key)
    else:
        packages = sorted(root.glob("*.rpm"))
        if not packages:
            raise ValueError("no RPMs to sign")
        run("rpmsign", "--define", "_openpgp_sign gpg",
            "--define", f"_openpgp_sign_id {key}", "--addsign", *packages)
    shutil.copyfile("/release-key.asc", root / "peritus-release.asc")
    entries = []
    for path in sorted(root.iterdir()):
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"non-regular package asset: {path}")
        entries.append(f"{digest(path)}  {path.name}\n")
    sums = root / "SHA256SUMS"
    sums.write_text("".join(entries))
    run("gpg", "--batch", "--local-user", key, "--digest-algo", "SHA256", "--armor",
        "--output", root / "SHA256SUMS.asc", "--detach-sign", sums)
    bundle(root, kind, key)


def bundle(root, kind, key):
    record = json.loads((root / f"peritus-{kind}-build.json").read_text())
    assets = Path("/assets")
    archive = assets / f"peritus-{kind}-{record['architecture']}.tar.gz"
    archive_source(root, archive, record["source_date_epoch"])
    run("gpg", "--batch", "--local-user", key, "--digest-algo", "SHA256", "--armor",
        "--output", archive.with_name(archive.name + ".asc"), "--detach-sign", archive)
    archive.with_name(archive.name + ".sha256").write_text(f"{digest(archive)}  {archive.name}\n")
    for path in root.iterdir():
        if path.suffix == ".deb" or path.suffix == ".rpm" and not path.name.endswith(".src.rpm"):
            shutil.copyfile(path, assets / path.name)
    shutil.copyfile("/release-key.asc", assets / "peritus-release.asc")


if __name__ == "__main__":
    main()
