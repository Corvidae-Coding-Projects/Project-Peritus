"""Native x86-64 Windows release builds with an explicit, reproducible C compiler."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
CLANG_VERSION = "20.1.8"
BINARIES = {
    "peritus": "peritus-cli",
    "peritusd": "peritus-daemon",
    "peritus-tui": "peritus-tui",
    "peritus-windows-sandbox-helper": "peritus-sandbox-windows",
}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def compiler_environment():
    if platform.system() != "Windows" or platform.machine().lower() not in ("amd64", "x86_64"):
        raise ValueError("the pinned C compiler requires a native x86-64 Windows host")
    compiler = shutil.which("clang-cl")
    if compiler is None:
        raise ValueError("install the pinned native LLVM clang-cl compiler before release compilation")
    observed = subprocess.check_output([compiler, "--version"], text=True).strip()
    if not re.search(r"^clang version " + re.escape(CLANG_VERSION) + r"(?:\s|$)", observed):
        raise ValueError(f"release C compiler must be clang-cl {CLANG_VERSION}; observed {observed}")
    if "Target: x86_64-pc-windows-msvc" not in observed:
        raise ValueError("release C compiler must target the native Windows MSVC ABI")
    record = {"path": compiler, "version": observed, "sha256": digest(Path(compiler))}
    print(json.dumps({"native_c_compiler": record}, sort_keys=True), flush=True)
    return {**os.environ, "CC_x86_64_pc_windows_msvc": compiler}, record


def build(binary):
    package = BINARIES[binary]
    environment, _ = compiler_environment()
    subprocess.run(["cargo", "rustc", "--release", "--locked", "--package", package,
                    "--bin", binary, "--", "-C", "link-arg=/Brepro"],
                   cwd=ROOT, env=environment, check=True)


def sqlite_object(target):
    files = list(target.glob("release/build/libsqlite3-sys-*/out/*sqlite3.o"))
    files += list(target.glob("release/build/libsqlite3-sys-*/out/*sqlite3.obj"))
    if len(files) != 1 or not files[0].is_file() or files[0].is_symlink():
        raise ValueError("expected exactly one freshly compiled bundled SQLite object")
    return files[0]


def check_sqlite():
    environment, compiler = compiler_environment()
    (ROOT / "target").mkdir(exist_ok=True)
    # Retain both fresh build trees, including on failure, for compiler-level diagnosis.
    directory = Path(tempfile.mkdtemp(prefix="windows-sqlite-repro-", dir=ROOT / "target"))
    observations = []
    for role in ("first", "second"):
        target = directory / role
        subprocess.run(["cargo", "build", "--release", "--locked", "--package", "peritus-journal",
                        "--target-dir", str(target)], cwd=ROOT, env=environment, check=True)
        payload = sqlite_object(target)
        observations.append({"role": role, "object": str(payload), "sha256": digest(payload)})
    identical = observations[0]["sha256"] == observations[1]["sha256"]
    report = {"compiler": compiler, "objects": observations, "byte_identical": identical}
    (ROOT / "target/windows-sqlite-rebuild.json").write_text(json.dumps(report, indent=2) + "\n")
    if not identical:
        raise ValueError("fresh native SQLite compilations differ; inspect windows-sqlite-rebuild.json")
    print("Two fresh native bundled SQLite compilations are byte-identical", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("build", "check-sqlite"))
    parser.add_argument("binary", nargs="?", choices=tuple(BINARIES))
    arguments = parser.parse_args()
    if arguments.operation == "build":
        if arguments.binary is None:
            parser.error("build requires a binary")
        build(arguments.binary)
    else:
        if arguments.binary is not None:
            parser.error("check-sqlite does not accept a binary")
        check_sqlite()
