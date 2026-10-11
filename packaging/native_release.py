"""Build and package the current release on its native host."""

import hashlib
import json
from pathlib import Path
import platform
import subprocess
import time
import tomllib

from archive import archive_tree
from windows_release import build as build_windows_binary

ROOT = Path(__file__).resolve().parent.parent


def run(arguments):
    subprocess.run(arguments, cwd=ROOT, check=True)


def main():
    started = int(time.time())
    system = platform.system()
    host = {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}[system]
    architecture = platform.machine().lower()
    architecture = {"amd64": "x86_64", "arm64": "aarch64"}.get(architecture, architecture)
    if architecture not in ("x86_64", "aarch64"):
        raise ValueError(f"unsupported native architecture: {architecture}")
    helper = f"peritus-{host}-sandbox-helper"
    products = {
        "peritus": "peritus-cli",
        "peritusd": "peritus-daemon",
        "peritus-tui": "peritus-tui",
        helper: f"peritus-sandbox-{host}",
    }
    if system == "Windows":
        for binary, package in products.items():
            if architecture == "x86_64":
                build_windows_binary(binary)
            else:
                run(["cargo", "rustc", "--release", "--locked", "--package", package,
                     "--bin", binary, "--", "-C", "link-arg=/Brepro"])
    else:
        arguments = ["cargo", "build", "--release", "--locked", "--bins"]
        for package in products.values():
            arguments.extend(["--package", package])
        run(arguments)

    run(["cargo", "run", "--locked", "--package", "peritus-platform-qualification",
         "--bin", "peritus-package", "--", "--use-release-artifacts"])
    package = ROOT / "dist" / f"peritus-{host}-{architecture}"
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    manifest = tomllib.loads((package / "manifest.toml").read_text())
    if manifest["release"] != version:
        raise ValueError("assembled package version differs from source")
    for artifact in manifest["artifact"]:
        payload = package / artifact["path"]
        with payload.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        if payload.stat().st_size != artifact["bytes"] or digest != artifact["sha256"]:
            raise ValueError(f"assembled artifact differs from its manifest: {artifact['path']}")
    suffix = ".exe" if system == "Windows" else ""
    for binary in ("peritus", "peritusd"):
        observed = subprocess.check_output([str(package / "bin" / (binary + suffix)), "--version"],
                                           cwd=ROOT, text=True).strip()
        if observed != f"{binary} {version}":
            raise ValueError(f"assembled executable reports the wrong version: {observed}")

    extension = ".zip" if system == "Windows" else ".tar.gz"
    archive = package.with_name(package.name + extension)
    epoch = int(subprocess.check_output(["git", "log", "-1", "--format=%ct"], cwd=ROOT, text=True))
    archive_tree(package, archive, epoch)
    checksum = archive.with_name(archive.name + ".sha256")
    with archive.open("rb") as stream:
        checksum.write_bytes((hashlib.file_digest(stream, "sha256").hexdigest() + "\n").encode("ascii"))
    record = {"schema_version": 1, "archive": archive.relative_to(ROOT).as_posix(),
              "checksum": checksum.relative_to(ROOT).as_posix(),
              "build_started_unix": started, "build_finished_unix": int(time.time())}
    (ROOT / "dist" / "peritus-native-package.json").write_text(json.dumps(record, indent=2) + "\n")
    print(json.dumps({"platform": host, "architecture": architecture,
                      "version": version, "archive": record["archive"]}, sort_keys=True))


if __name__ == "__main__":
    main()
