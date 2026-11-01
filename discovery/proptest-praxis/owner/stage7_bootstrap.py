#!/usr/bin/env python3
"""Exercise the real Unix bootstrap with local release archives and an isolated HOME."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import random
import subprocess
import tarfile
import tempfile

from stage7_install import CANARIES, MANAGED, contents, fixture_bundle, fixture_commands, sha, write
import stage7_install


HERE = Path(__file__).resolve().parent
ROOT = stage7_install.ROOT
ASSET = "peritus-linux-x86_64.tar.gz"
BUNDLE = "peritus-linux-x86_64"
VALID = "v7.0.0"


def archive(path: Path, bundle: Path, kind: str) -> None:
    with tarfile.open(path, "w:gz") as output:
        output.add(bundle, arcname=BUNDLE, recursive=True)
        if kind == "outside":
            content = b"outside member\n"
            entry = tarfile.TarInfo("outside-package.txt")
            entry.size = len(content)
            output.addfile(entry, io.BytesIO(content))
        elif kind == "link":
            entry = tarfile.TarInfo(f"{BUNDLE}/linked")
            entry.type = tarfile.SYMTYPE
            entry.linkname = "../outside"
            output.addfile(entry)


def run_case(case: dict[str, str], retain: Path | None = None) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="peritus-stage7-bootstrap-") as temporary:
        base = Path(temporary)
        home, tmp, stubs, release = (base / name for name in ("home", "tmp", "stubs", "releases"))
        for directory in (home, tmp, stubs, release / VALID):
            directory.mkdir(parents=True)
        fixture_commands(stubs)
        count = base / "install-count"
        count.write_text("0\n")
        for index, relative in enumerate(CANARIES):
            write(home / relative, f"bootstrap-user-{index}\n".encode())
        write(home / ".bashrc", b"# user profile\n")
        before = contents(home)
        bundle = base / BUNDLE
        fixture_bundle(bundle, "7.0.0")
        kind = case["kind"]
        asset = release / VALID / ASSET
        if kind != "missing_archive":
            archive(asset, bundle, kind)
            checksum = sha(asset.read_bytes())
            if kind == "checksum_mismatch":
                checksum = "0" * 64 if checksum != "0" * 64 else "1" * 64
            elif kind == "checksum_malformed":
                checksum = "not-a-sha256"
            (release / VALID / f"{ASSET}.sha256").write_text(checksum + "\n")
        env = dict(os.environ, HOME=str(home), TMPDIR=str(tmp), SHELL="/bin/bash",
                   PATH=f"{stubs}:/usr/bin:/bin", PERITUS_INSTALL_DEPS="0",
                   PRACTICE_INSTALL_COUNT=str(count), PRACTICE_FAIL_AT="0",
                   PERITUS_VERSION=case["version"], PERITUS_RELEASE_BASE_URL=release.as_uri())
        process = subprocess.run(["/bin/sh", str(ROOT / "install.sh")], env=env,
                                 capture_output=True, text=True, timeout=40, check=False)
        after = contents(home)
        problems = []
        for relative in CANARIES:
            if after.get(relative) != before.get(relative):
                problems.append(f"bootstrap changed user/sibling canary: {relative}")
        expected_error = {
            "invalid_version": "release version is not a vMAJOR.MINOR.PATCH tag",
            "checksum_mismatch": "release archive checksum did not match",
            "checksum_malformed": "release checksum is malformed",
            "outside": "release archive contains a path outside the package",
            "link": "release archive contains a link or special file",
            "missing_archive": "could not download",
        }
        if kind == "valid":
            if process.returncode != 0 or "is installed" not in process.stdout:
                problems.append("valid archive did not produce a truthful successful install")
            for relative in MANAGED:
                if relative not in after:
                    problems.append(f"valid archive omitted installed package path: {relative}")
            command = home / ".local/bin/peritus"
            if command.is_file():
                check = subprocess.run([str(command), "--version"], capture_output=True,
                                       text=True, timeout=5, check=False)
                if check.returncode or check.stdout.strip() != "peritus 7.0.0":
                    problems.append("installed command version readback disagreed")
        else:
            if process.returncode == 0 or expected_error[kind] not in process.stderr:
                problems.append(f"{kind} did not report its expected nonzero error")
            for relative in MANAGED:
                if relative in after:
                    problems.append(f"{kind} published package path: {relative}")
        if retain is not None:
            retain.mkdir(parents=True)
            (retain / "case.json").write_text(json.dumps(case, indent=2, sort_keys=True) + "\n")
            (retain / "observation.json").write_text(json.dumps({
                "command": ["/bin/sh", str(ROOT / "install.sh")],
                "exit_code": process.returncode, "stdout": process.stdout,
                "stderr": process.stderr, "before": before, "after": after,
                "problems": problems}, indent=2, sort_keys=True) + "\n")
        return {"status": "pass" if not problems else "fail", "case": case,
                "exit_code": process.returncode, "problems": problems}


def cases(seed: int, count: int) -> list[dict[str, str]]:
    kinds = ("valid", "invalid_version", "checksum_mismatch", "checksum_malformed",
             "outside", "link", "missing_archive")
    invalid = ("7.0.0", "v7.0", "v7.x.0", "v7.0.0.1", "v7.-1.0", "v7.0.0-extra")
    result = [{"kind": kind, "version": invalid[0] if kind == "invalid_version" else VALID}
              for kind in kinds]
    rng = random.Random(seed)
    for _ in range(count):
        kind = rng.choice(kinds)
        result.append({"kind": kind,
                       "version": rng.choice(invalid) if kind == "invalid_version" else VALID})
    return result


def main() -> int:
    global ROOT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=7319)
    parser.add_argument("--cases", type=int, default=64)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--replay", type=Path)
    parser.add_argument("--source-root", type=Path, help="checkout containing install.sh and packaging/linux")
    args = parser.parse_args()
    if args.source_root is not None:
        ROOT = args.source_root.resolve()
        stage7_install.ROOT = ROOT
        stage7_install.LINUX = ROOT / "packaging/linux"
    if args.output.exists():
        parser.error("output must not exist")
    args.output.mkdir(parents=True, mode=0o700)
    selected = [json.loads(args.replay.read_text())] if args.replay else cases(args.seed, args.cases)
    first_failure = None
    metrics = {kind: 0 for kind in ("valid", "invalid_version", "checksum_mismatch",
                                     "checksum_malformed", "outside", "link", "missing_archive")}
    for index, case in enumerate(selected):
        result = run_case(case, args.output / f"case-{index:04d}")
        metrics[case["kind"]] += 1
        if result["status"] != "pass":
            first_failure = result
            (args.output / "first-failure.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
            break
    report = {"status": "pass" if first_failure is None else "fail", "seed": args.seed,
              "completed_cases": sum(metrics.values()), "metrics": metrics,
              "source_sha256": {"install.sh": sha((ROOT / "install.sh").read_bytes()),
                                "harness": sha(Path(__file__).read_bytes())},
              "first_failure": first_failure,
              "interface": "real install.sh with local file release archives and disposable HOME"}
    (args.output / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({key: report[key] for key in ("status", "seed", "completed_cases", "metrics")}))
    return int(first_failure is not None)


if __name__ == "__main__":
    raise SystemExit(main())
