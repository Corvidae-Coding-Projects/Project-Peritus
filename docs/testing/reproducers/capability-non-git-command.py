#!/usr/bin/env python3
"""Operate the actual standalone command executor in a disposable non-Git folder."""

import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[3]
    build = subprocess.run(
        ["cargo", "build", "-p", "peritus-product-runner", "--message-format=json"],
        cwd=root,
        env={**os.environ, "CARGO_BUILD_JOBS": "2"},
        text=True,
        stdout=subprocess.PIPE,
        check=True,
    )
    libraries = {}
    for line in build.stdout.splitlines():
        item = json.loads(line)
        if item.get("reason") == "compiler-artifact":
            for filename in item["filenames"]:
                if filename.endswith(".rlib"):
                    libraries[item["target"]["name"]] = filename
    product_library = Path(libraries["peritus_product_runner"])
    dependencies = product_library.parent
    if dependencies.name != "deps":
        dependencies = dependencies / "deps"
    with tempfile.TemporaryDirectory(prefix="peritus-command-replay-") as output:
        binary = Path(output) / "consumer"
        subprocess.run(
            [
                "rustc", "--edition=2024", "--crate-name", "capability_command_replay",
                str(Path(__file__).with_suffix(".rs")),
                "-L", f"dependency={dependencies}",
                "--extern", f"peritus_product_runner={libraries['peritus_product_runner']}",
                "--extern", f"tempfile={libraries['tempfile']}",
                "-o", str(binary),
            ],
            cwd=root,
            check=True,
        )
        subprocess.run([str(binary)], cwd=root, check=True)


if __name__ == "__main__":
    main()
