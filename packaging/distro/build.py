"""Build genuine Debian source/binary packages or source/binary RPMs."""

import json
from email.utils import formatdate
import os
from pathlib import Path
import shutil
import socket
import tempfile
import time
import tomllib

from common import (ROOT, FORMATS, architecture, container, digest, engine, image_name,
                    maintainer, output_directory, package_build_jobs, package_format, run, version)
from source import debian_metadata, prepare


def build_image():
    kind = package_format()
    rust = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    run(engine(), "build", "--tag", image_name(kind), "--build-arg", f"RUST_VERSION={rust}",
        "--file", ROOT / "packaging" / "containers" / f"{FORMATS[kind]}.Dockerfile",
        ROOT / "packaging" / "containers")


def save_image():
    (ROOT / "target").mkdir(exist_ok=True)
    run(engine(), "image", "save", "--output", ROOT / "target/distro-builder.tar",
        image_name(package_format()))


def restore_image():
    run(engine(), "image", "load", "--input", ROOT / "target/distro-builder.tar")
    run(engine(), "image", "inspect", image_name(package_format()))


def build():
    kind = package_format()
    maintainer()
    jobs = package_build_jobs()
    out = output_directory(kind)
    started = time.time_ns()
    (ROOT / "target").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"package-{kind}-", dir=ROOT / "target", delete=False) as temporary:
        build_root = Path(temporary)
        source, archive, epoch = prepare(build_root)
        if kind == "deb":
            debian_metadata(source, epoch)
            shutil.copyfile(archive, build_root / f"peritus_{version()}.orig.tar.gz")
            container(kind, [(build_root, "/build", False)],
                      "dpkg-buildpackage", "--build=full", "--no-sign", f"--jobs={jobs}",
                      "--", f"/build/{source.name}", build_jobs=jobs)
            suffixes = (".deb", ".dsc", ".changes", ".buildinfo", ".orig.tar.gz", ".debian.tar.xz")
            files = [p for p in build_root.iterdir() if p.name.endswith(suffixes)]
        else:
            date = formatdate(epoch, usegmt=True).split()
            changelog_date = f"{date[0].rstrip(',')} {date[2]} {date[1]} {date[3]}"
            for name in ("SOURCES", "SPECS", "RPMS", "SRPMS", "BUILD", "BUILDROOT"):
                (build_root / name).mkdir()
            shutil.copyfile(archive, build_root / "SOURCES" / archive.name)
            shutil.copyfile(source / "packaging/rpm/peritus.spec", build_root / "SPECS/peritus.spec")
            container(kind, [(build_root, "/build", False)],
                      "rpmbuild", "-ba", "--define", "_topdir /build",
                      *rpm_reproducibility_arguments(),
                      "--define", f"peritus_version {version()}",
                      "--define", f"peritus_packager {maintainer()}",
                      "--define", f"peritus_changelog_date {changelog_date}",
                      "/build/SPECS/peritus.spec",
                      environment=[f"SOURCE_DATE_EPOCH={epoch}"], build_jobs=jobs)
            files = list((build_root / "RPMS").rglob("*.rpm"))
            files += list((build_root / "SRPMS").glob("*.rpm"))
        if not any(p.suffix == (".deb" if kind == "deb" else ".rpm") for p in files):
            raise ValueError("package builder produced no installable packages")
        for path in files:
            shutil.copyfile(path, out / path.name)
        provenance = json.loads((source / "PACKAGE-SOURCE.json").read_text())
        provenance["container_image"] = run(engine(), "image", "inspect", image_name(kind),
                                            "--format", "{{.Id}}", capture=True)
        provenance["format"] = kind
        provenance["architecture"] = architecture()
        provenance["build_observation"] = {
            "host": socket.gethostname(),
            "invocation": build_root.name,
            "cargo_build_jobs": jobs,
            "started_unix_nanos": started,
            "finished_unix_nanos": time.time_ns(),
        }
        provenance["unsigned_files_sha256"] = {p.name: digest(p) for p in sorted(out.iterdir())}
        (out / f"peritus-{kind}-build.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(f"Built {kind} packages in {out}")


def rpm_reproducibility_arguments():
    """Normalize package metadata, not the separately retained real build observations."""
    return ["--define", "_buildhost peritus-reproducible",
            "--define", "use_source_date_epoch_as_buildtime 1",
            "--define", "build_mtime_policy clamp_to_source_date_epoch"]
