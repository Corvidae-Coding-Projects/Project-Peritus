"""Compile native daemon libraries and their binary consumers in observed phases."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import time

import native_inputs as inputs
import native_transport as transport
from native_transport import cargo_arguments

ROOT = Path(__file__).resolve().parent.parent


def compile_phase(stage, expected):
    arguments = cargo_arguments(stage, expected["binary"])
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/native-daemon")
    environment["CARGO_BUILD_JOBS"] = str(expected["environment"]["cargo_build_jobs"])
    environment["CARGO_INCREMENTAL"] = "0"
    started = time.time_ns()
    subprocess.run(arguments, cwd=ROOT, env=environment, check=True)
    observed = inputs.observation(started, arguments)
    if inputs.binding(expected["binary"]) != expected:
        raise ValueError("native compilation inputs changed while Cargo was running")
    return observed


def library():
    expected = inputs.binding()
    tree, bundle = ROOT / "target/native-daemon", ROOT / "target/native-daemon-libraries"
    if tree.exists() or tree.is_symlink() or bundle.exists() or bundle.is_symlink():
        raise ValueError("native library compilation requires fresh output directories")
    inputs.normalize_verified_sources(expected["candidate"])
    observed = compile_phase("library", expected)
    transport.save(tree, bundle, expected, observed)


def binary(binary_name="peritusd"):
    expected = inputs.binding(binary_name)
    tree = ROOT / "target/native-daemon"
    product = ROOT / "target/release" / binary_name
    record_path = ROOT / "target" / transport.record_filename(binary_name)
    if product.exists() or product.is_symlink() or record_path.exists() or record_path.is_symlink():
        raise ValueError("native final compilation refuses to overwrite a product or record")
    inputs.normalize_verified_sources(expected["candidate"])
    earlier = transport.restore(ROOT / "target/native-daemon-libraries", tree,
                                inputs.library_binding(expected))
    observed = compile_phase("binary", expected)
    compiled = transport.regular(tree / "release" / binary_name)
    if not compiled.stat().st_size or not compiled.stat().st_mode & 0o100:
        raise ValueError("native final compilation did not produce its executable binary")
    record = {"schema_version": 2, "kind": "native-release-binary-compilation",
              "binding": expected, "library": earlier, "observation": observed,
              "binary": {"sha256": transport.digest(compiled), "byte_length": compiled.stat().st_size}}
    inputs.validate_binary_record(record, expected["candidate"], expected["role"], compiled, binary_name)
    product.parent.mkdir(parents=True, exist_ok=True)
    with compiled.open("rb") as source, product.open("xb") as output:
        shutil.copyfileobj(source, output)
    shutil.copymode(compiled, product)
    inputs.validate_binary_record(record, expected["candidate"], expected["role"], product, binary_name)
    inputs.write_record(record_path, record)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="phase", required=True)
    commands.add_parser("library")
    consumer = commands.add_parser("binary")
    consumer.add_argument("binary", choices=tuple(transport.BINARY_PACKAGES))
    arguments = parser.parse_args()
    if arguments.phase == "library":
        library()
    else:
        binary(arguments.binary)
