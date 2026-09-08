"""Manual-only byte comparison with the previous native compilation boundaries.

These diagnostic products are never uploaded as release binaries or admitted as
release compilation records. The workflow supplies the actual staged product.
"""

import argparse
import filecmp
from pathlib import Path
import time

import native_build
import native_inputs as inputs
import native_transport as transport
import windows_release

ROOT = Path(__file__).resolve().parent.parent


def check(binary):
    expected = inputs.binding(binary)
    environment = expected["environment"]
    host = environment["system"], environment["machine"].lower(), binary
    if host not in (("Darwin", "x86_64", "peritus"),
                    ("Windows", "amd64", "peritusd"), ("Windows", "x86_64", "peritusd")):
        raise ValueError("staging comparison is limited to the changed native compilation paths")
    windows = environment["system"] == "Windows"
    filename = binary + (".exe" if windows else "")
    directory = ROOT / "target/staging-candidate"
    if (directory.is_symlink() or not directory.is_dir()
            or {path.name for path in directory.iterdir()} != {filename}):
        raise ValueError("staging comparison requires exactly the actual same-run staged binary")
    staged = transport.regular(directory / filename)
    target = ROOT / ("target/release" if windows else "target/native-daemon")
    report = ROOT / "target/native-staging-check.json"
    if target.exists() or target.is_symlink() or report.exists() or report.is_symlink():
        raise ValueError("previous-path compilation requires fresh diagnostic outputs")
    inputs.normalize_verified_sources(expected["candidate"])
    if windows:
        started = time.time_ns()
        windows_release.build(binary)
        observation = inputs.observation(started, transport.cargo_arguments("binary", binary, "Windows"))
        product = target / filename
    else:
        transport.restore(ROOT / "target/native-daemon-libraries", target, inputs.daemon_binding(expected))
        observation = native_build.compile_phase("binary", expected)
        product = target / "release" / filename
    if inputs.binding(binary) != expected:
        raise ValueError("previous-path inputs changed while Cargo was running")
    transport.regular(product)
    identical = bool(staged.stat().st_size and filecmp.cmp(staged, product, shallow=False))
    inputs.write_record(report, {"schema_version": 1, "kind": "native-staging-diagnostic",
        "binding": expected, "observation": observation, "byte_identical": identical,
        "staged_sha256": transport.digest(staged), "previous_path_sha256": transport.digest(product)})
    if not identical:
        raise ValueError("staged binary differs from the previous native compilation path")
    print(f"Staged {filename} is byte-identical to the previous native compilation path", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", choices=tuple(transport.BINARY_PACKAGES))
    check(parser.parse_args().binary)
