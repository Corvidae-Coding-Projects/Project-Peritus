"""Sign packages via GnuPG's restricted agent socket; never export private keys."""

import os
import json
from pathlib import Path
import re
import shutil
import tempfile

from common import ROOT, architecture, container, digest, package_format, run, version

PUBLIC_KEY = ROOT / "packaging" / "keys" / "peritus-release.asc"


def fingerprint():
    value = os.environ.get("PERITUS_SIGNING_FINGERPRINT", "").upper()
    if not re.fullmatch(r"[A-F0-9]{40}", value):
        raise ValueError("set PERITUS_SIGNING_FINGERPRINT to the full OpenPGP fingerprint")
    metadata = run("gpg", "--batch", "--with-colons", "--show-keys", PUBLIC_KEY, capture=True)
    fingerprints = [line.split(":")[9] for line in metadata.splitlines() if line.startswith("fpr:")]
    if not fingerprints or fingerprints[0] != value:
        raise ValueError("signing fingerprint does not match the reviewed public release key")
    return value


def sign():
    kind = package_format()
    key = fingerprint()
    packages = ROOT / "dist" / "packages" / kind
    if not (packages / f"peritus-{kind}-build.json").is_file():
        raise ValueError("build the packages before signing")
    if (packages / "SHA256SUMS.asc").exists():
        raise ValueError("refusing to re-sign an already signed package set")
    validate_unsigned(packages, kind)
    destination = ROOT / "dist" / "signed" / kind
    if destination.exists():
        raise ValueError(f"refusing to overwrite a signed release set: {destination}")
    run("gpgconf", "--launch", "gpg-agent")
    socket = Path(run("gpgconf", "--list-dirs", "agent-extra-socket", capture=True))
    if not socket.is_socket():
        raise ValueError("GnuPG's restricted signing-agent socket is unavailable")
    with tempfile.TemporaryDirectory(prefix="peritus-signing-") as temporary, \
            tempfile.TemporaryDirectory(prefix=f"signed-{kind}-", dir=ROOT / "target") as staged:
        home = Path(temporary)
        stage = Path(staged)
        shutil.copytree(packages, stage / "packages")
        (stage / "assets").mkdir()
        (home / "gpg.conf").write_text("no-autostart\n")
        container(kind, [
            (stage / "packages", "/packages", False),
            (stage / "assets", "/assets", False),
            (ROOT / "packaging" / "distro", "/tools", True),
            (PUBLIC_KEY, "/release-key.asc", True),
            (home, "/signing", False),
            (socket, "/signing/S.gpg-agent", False),
        ], "python3", "/tools/sign_inside.py", kind, key,
            environment=["GNUPGHOME=/signing"], forward_agent=True)
        destination.parent.mkdir(parents=True, exist_ok=True)
        stage.rename(destination)
    print(f"Signed {kind} package set with {key}; no private key was exported")


def validate_unsigned(packages, kind):
    name = f"peritus-{kind}-build.json"
    record = json.loads((packages / name).read_text())
    if (record["format"], record["architecture"], record["version"]) != (kind, architecture(), version()):
        raise ValueError("unsigned package format, native architecture, or version does not match")
    expected = record["unsigned_files_sha256"]
    if set(expected) != {p.name for p in packages.iterdir()} - {name}:
        raise ValueError("unsigned build inventory does not exactly cover the package assets")
    for name, checksum in expected.items():
        path = packages / name
        if Path(name).name != name or path.is_symlink() or not path.is_file():
            raise ValueError("unsigned build inventory has an unsafe member")
        if digest(path) != checksum:
            raise ValueError(f"unsigned package changed after build: {name}")
    if os.environ.get("GITHUB_ACTIONS") == "true":
        if record["git_commit"] != os.environ.get("GITHUB_SHA"):
            raise ValueError("package was built from a different release commit")
        for name, checksum in record["source_files_sha256"].items():
            path = ROOT / name
            if Path(name).is_absolute() or ".." in Path(name).parts or not path.is_file():
                raise ValueError("unsafe or missing source inventory member")
            if digest(path) != checksum:
                raise ValueError(f"package source differs from tagged checkout: {name}")
