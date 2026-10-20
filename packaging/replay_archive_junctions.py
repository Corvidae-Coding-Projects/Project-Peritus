"""Replay Windows junction rejection through the real archive command (Python 3.12+)."""

import argparse
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive-script", type=Path,
                        default=Path(__file__).with_name("archive.py"))
    parser.add_argument("--work-dir", type=Path, help="parent for disposable fixtures")
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.exit(2, "BLOCKED: this reproduction requires Windows directory junctions\n")
    script = args.archive_script.resolve()
    failures = 0
    with tempfile.TemporaryDirectory(dir=args.work_dir, prefix="archive-junction-") as temporary:
        root = Path(temporary)
        outside = root / "outside"
        outside.mkdir()
        (outside / "external.txt").write_bytes(b"outside package: must not be archived\n")
        for kind in ("ordinary", "nested", "root", "dangling"):
            fixture = root / kind
            fixture.mkdir()
            source = fixture / "package"
            if kind != "root":
                source.mkdir()
                (source / "payload.txt").write_bytes(b"ordinary package payload\n")
            if kind != "ordinary":
                link = source if kind == "root" else source / "linked"
                target = fixture / "missing" if kind == "dangling" else outside
                if kind == "dangling":
                    target.mkdir()
                environment = dict(os.environ, PERITUS_REPLAY_LINK=str(link),
                                   PERITUS_REPLAY_TARGET=str(target))
                subprocess.run([
                    "powershell.exe", "-NoProfile", "-NonInteractive", "-Command",
                    "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction "
                    "-Path $env:PERITUS_REPLAY_LINK -Target $env:PERITUS_REPLAY_TARGET | Out-Null",
                ], env=environment, check=True, capture_output=True, text=True)
                if kind == "dangling":
                    target.rmdir()
            for extension in (".zip", ".tar.gz"):
                destination = fixture / ("result" + extension)
                result = subprocess.run([sys.executable, str(script), str(source),
                                         str(destination), "1700000000"],
                                        capture_output=True, text=True, timeout=30)
                if kind == "ordinary":
                    passed = result.returncode == 0 and destination.is_file()
                    if passed:
                        if extension == ".zip":
                            with zipfile.ZipFile(destination) as archive:
                                passed = (archive.namelist() == ["package/", "package/payload.txt"]
                                          and archive.read("package/payload.txt")
                                          == b"ordinary package payload\n")
                        else:
                            with tarfile.open(destination) as archive:
                                passed = (archive.getnames() == ["package", "package/payload.txt"]
                                          and archive.extractfile("package/payload.txt").read()
                                          == b"ordinary package payload\n")
                else:
                    # The documented contract rejects links before creating an archive.
                    passed = (result.returncode != 0 and not destination.exists()
                              and "ValueError:" in result.stderr)
                print(f"{'PASS' if passed else 'FAIL'}: {kind} {extension}")
                failures += not passed
    return int(failures != 0)


if __name__ == "__main__":
    sys.exit(main())
