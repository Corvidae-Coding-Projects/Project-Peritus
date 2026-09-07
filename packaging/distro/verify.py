"""Run signature and installed-package qualification in disposable containers."""

from common import ROOT, container, digest, package_format
from sign import PUBLIC_KEY, fingerprint, validate_unsigned


def verify():
    kind = package_format()
    key = fingerprint()
    directory = ROOT / "dist" / "signed" / kind
    unsigned = ROOT / "dist" / "packages" / kind
    validate_unsigned(unsigned, kind)
    record = f"peritus-{kind}-build.json"
    if digest(unsigned / record) != digest(directory / "packages" / record):
        raise ValueError("signed provenance does not bind the retained unsigned build")
    container(kind, [
        (directory / "packages", "/packages", True),
        (directory / "assets", "/assets", True),
        (unsigned, "/unsigned", True),
        (ROOT / "packaging" / "distro", "/tools", True),
        (PUBLIC_KEY, "/release-key.asc", True),
    ], "python3", "/tools/verify_inside.py", kind, key, root=True,
        environment=["DEBIAN_FRONTEND=noninteractive"])
