"""Shared, fail-closed helpers for distribution package tooling (Python 3.12+)."""

import hashlib
import os
import platform
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parent.parent.parent
FORMATS = {"deb": "debian", "rpm": "fedora"}


def run(*args, cwd=None, capture=False, env=None, input=None):
    """Run an argument vector, never a shell; failures stop the pipeline."""
    result = subprocess.run(
        [str(arg) for arg in args], cwd=cwd, check=True, text=True,
        stdout=subprocess.PIPE if capture else None, env=env, input=input,
    )
    return result.stdout.strip() if capture else None


def version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", value):
        raise ValueError("package version must be MAJOR.MINOR.PATCH")
    return value


def maintainer():
    value = os.environ.get("PERITUS_PACKAGE_MAINTAINER", "")
    if not re.fullmatch(r"[^\r\n<>%]+ <[^\s<>%]+@[^\s<>%]+>", value):
        raise ValueError("set PERITUS_PACKAGE_MAINTAINER to 'Name <email>'")
    return value


def package_format():
    value = os.environ.get("PERITUS_PACKAGE_FORMAT", "")
    if value not in FORMATS:
        raise ValueError("PERITUS_PACKAGE_FORMAT must be deb or rpm")
    return value


def engine():
    value = os.environ.get("PERITUS_CONTAINER_ENGINE", "docker")
    if value not in ("docker", "podman"):
        raise ValueError("PERITUS_CONTAINER_ENGINE must be docker or podman")
    return value


def image_name(kind):
    rust = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    return f"localhost/peritus-package-{FORMATS[kind]}:{rust}"


def architecture():
    value = platform.machine()
    if value not in ("x86_64", "aarch64"):
        raise ValueError(f"unsupported native package architecture: {value}")
    return value


def digest(path, algorithm="sha256"):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, algorithm).hexdigest()


def output_directory(kind):
    path = ROOT / "dist" / "packages" / kind
    path.mkdir(parents=True, exist_ok=True)
    if any(path.iterdir()):
        raise ValueError(f"refusing to overwrite existing package output: {path}")
    return path


def container(kind, mounts, *command, network="none", root=False, environment=(), forward_agent=False):
    args = [engine(), "run", "--rm", "--network", network]
    if engine() == "podman" and forward_agent:
        # The host agent socket must retain its original SELinux label. This changes
        # labeling only for this disposable, network-isolated signing container.
        args.extend(["--security-opt", "label=disable"])
    if not root:
        if engine() == "podman":
            args.extend(["--userns", "keep-id"])
        args.extend(["--user", f"{os.getuid()}:{os.getgid()}"])
    for source, target, readonly in mounts:
        label = ",z" if engine() == "podman" and not forward_agent else ""
        args.extend(["--volume", f"{source}:{target}:{'ro' if readonly else 'rw'}{label}"])
    for entry in environment:
        args.extend(["--env", entry])
    args.extend(["--env", "CARGO_BUILD_JOBS=2", image_name(kind)])
    return run(*args, *command)
