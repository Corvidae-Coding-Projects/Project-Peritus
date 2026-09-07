"""Build genuine Debian source/binary packages or source/binary RPMs."""

import json
from email.utils import formatdate
import os
from pathlib import Path
import re
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
        source, epoch = prepare_native_source(kind, build_root)
        run_native_build(kind, build_root, source, epoch, jobs)
        retain_packages(kind, build_root, source, out, jobs, started)
    print(f"Built {kind} packages in {out}")


def prepare_native_source(kind, build_root):
    """Prepare the same complete vendored sources for full or staged native builds."""
    source, archive, epoch = prepare(build_root)
    if kind == "deb":
        debian_metadata(source, epoch)
        shutil.copyfile(archive, build_root / f"peritus_{version()}.orig.tar.gz")
    else:
        for name in ("SOURCES", "SPECS", "RPMS", "SRPMS", "BUILD", "BUILDROOT"):
            (build_root / name).mkdir()
        shutil.copyfile(archive, build_root / "SOURCES" / archive.name)
        shutil.copyfile(source / "packaging/rpm/peritus.spec", build_root / "SPECS/peritus.spec")
    return source, epoch


def run_native_build(kind, build_root, source, epoch, jobs, stage="full"):
    """Continue native recipes without short-circuiting any packaging or test stage."""
    if kind not in FORMATS:
        raise ValueError("native package format must be deb or rpm")
    if stage not in ("full", "compile", "compile-checks", "package"):
        raise ValueError("unknown native build stage")
    if stage == "compile-checks" and kind != "rpm":
        raise ValueError("separate check compilation is only supported for RPM")
    if kind == "deb":
        options = ["--rules-target=override_dh_auto_build"] if stage == "compile" else ["--build=full"]
        if stage == "package":
            options.append("--no-pre-clean")
        container(kind, [(build_root, "/build", False)],
                  "dpkg-buildpackage", *options, "--no-sign", f"--jobs={jobs}",
                  "--", f"/build/{source.name}", build_jobs=jobs)
    else:
        date = formatdate(epoch, usegmt=True).split()
        changelog_date = f"{date[0].rstrip(',')} {date[2]} {date[1]} {date[3]}"
        options = ["-bc", "--noclean"] if stage in ("compile", "compile-checks") else ["-ba"]
        if stage in ("package", "compile-checks"):
            options.append("--noprep")
        if stage == "compile-checks":
            options.extend(["--define", "peritus_compile_checks 1"])
        container(kind, [(build_root, "/build", False)],
                  "rpmbuild", *options, "--define", "_topdir /build",
                  *rpm_reproducibility_arguments(),
                  "--define", f"peritus_version {version()}",
                  "--define", f"peritus_packager {maintainer()}",
                  "--define", f"peritus_changelog_date {changelog_date}",
                  "/build/SPECS/peritus.spec",
                  environment=[f"SOURCE_DATE_EPOCH={epoch}"], build_jobs=jobs)


def image_identity(kind):
    observed = run(engine(), "image", "inspect", image_name(kind), "--format", "{{.Id}}", capture=True)
    if not re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", observed):
        raise ValueError("native builder did not return a SHA-256 image identity")
    return "sha256:" + observed.removeprefix("sha256:")


def build_observation(build_root, jobs, started):
    return {"host": socket.gethostname(), "invocation": build_root.name,
            "cargo_build_jobs": jobs, "started_unix_nanos": started,
            "finished_unix_nanos": time.time_ns(),
            "workflow_job": os.environ.get("GITHUB_JOB"),
            "workflow_run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT")}


def retain_packages(kind, build_root, source, out, jobs, started, compilation=None, observation_root=None):
    if kind == "deb":
        suffixes = (".deb", ".dsc", ".changes", ".buildinfo", ".orig.tar.gz", ".debian.tar.xz")
        files = [p for p in build_root.iterdir() if p.name.endswith(suffixes)]
    else:
        files = list((build_root / "RPMS").rglob("*.rpm"))
        files += list((build_root / "SRPMS").glob("*.rpm"))
    if not any(p.suffix == (".deb" if kind == "deb" else ".rpm") for p in files):
        raise ValueError("package builder produced no installable packages")
    for path in files:
        shutil.copyfile(path, out / path.name)
    provenance = json.loads((source / "PACKAGE-SOURCE.json").read_text())
    provenance["container_image"] = image_identity(kind)
    provenance["format"] = kind
    provenance["architecture"] = architecture()
    provenance["build_observation"] = build_observation(observation_root or build_root, jobs, started)
    if compilation is not None:
        provenance["compilation"] = compilation
    provenance["unsigned_files_sha256"] = {p.name: digest(p) for p in sorted(out.iterdir())}
    (out / f"peritus-{kind}-build.json").write_text(json.dumps(provenance, indent=2) + "\n")


def rpm_reproducibility_arguments():
    """Normalize package metadata, not the separately retained real build observations."""
    return ["--define", "_buildhost peritus-reproducible",
            "--define", "use_source_date_epoch_as_buildtime 1",
            "--define", "build_mtime_policy clamp_to_source_date_epoch"]
