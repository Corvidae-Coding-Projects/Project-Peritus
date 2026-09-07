"""Compile native daemon libraries and its final binary in independently observed phases."""

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
    arguments = cargo_arguments(stage)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/native-daemon")
    environment["CARGO_BUILD_JOBS"] = str(expected["environment"]["cargo_build_jobs"])
    environment["CARGO_INCREMENTAL"] = "0"
    started = time.time_ns()
    subprocess.run(arguments, cwd=ROOT, env=environment, check=True)
    observed = inputs.observation(started, arguments)
    if inputs.binding() != expected:
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


def binary():
    expected = inputs.binding()
    tree = ROOT / "target/native-daemon"
    product = ROOT / "target/release/peritusd"
    record_path = ROOT / "target/native-daemon-build.json"
    if product.exists() or product.is_symlink() or record_path.exists() or record_path.is_symlink():
        raise ValueError("native final compilation refuses to overwrite a product or record")
    inputs.normalize_verified_sources(expected["candidate"])
    earlier = transport.restore(ROOT / "target/native-daemon-libraries", tree, expected)
    observed = compile_phase("binary", expected)
    compiled = transport.regular(tree / "release/peritusd")
    if not compiled.stat().st_size or not compiled.stat().st_mode & 0o100:
        raise ValueError("native final compilation did not produce an executable daemon")
    record = {"schema_version": 1, "kind": "native-daemon-binary-compilation",
              "binding": expected, "library": earlier, "observation": observed,
              "binary": {"sha256": transport.digest(compiled), "byte_length": compiled.stat().st_size}}
    inputs.validate_binary_record(record, expected["candidate"], expected["role"], compiled)
    product.parent.mkdir(parents=True, exist_ok=True)
    with compiled.open("rb") as source, product.open("xb") as output:
        shutil.copyfileobj(source, output)
    shutil.copymode(compiled, product)
    inputs.validate_binary_record(record, expected["candidate"], expected["role"], product)
    inputs.write_record(record_path, record)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=("library", "binary"))
    arguments = parser.parse_args()
    (library if arguments.phase == "library" else binary)()
