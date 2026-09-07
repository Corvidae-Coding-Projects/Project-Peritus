"""Cryptographic and real package-manager lifecycle checks; only runs in a container."""

import json
import os
from pathlib import Path
import pty
import re
import shutil
import subprocess
import sys
import tempfile

from common import digest, run


def must_fail(*args):
    result = subprocess.run([str(arg) for arg in args], stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, check=False)
    if result.returncode == 0:
        raise ValueError(f"negative qualification unexpectedly succeeded: {args[0]}")
    return result.stdout


def terminal_failure(*args):
    """Exercise the public CLI with the terminal required by its command contract."""
    output = bytearray()

    def read(descriptor):
        data = os.read(descriptor, 65536)
        output.extend(data)
        return data

    status = pty.spawn([str(arg) for arg in args], master_read=read)
    if os.waitstatus_to_exitcode(status) == 0:
        raise ValueError("terminal negative qualification unexpectedly succeeded")
    return output.decode("utf-8", errors="replace")


def rpm_signature_verified(output, key):
    """RPM 6 prints lowercase 'signature' and the complete signing fingerprint."""
    pattern = rf"^\s*Header .*\bsignature, key fingerprint: {re.escape(key)}: OK$"
    return re.search(pattern, output, re.IGNORECASE | re.MULTILINE) is not None


def sums(root, keyring):
    run("gpgv", "--keyring", keyring, root / "SHA256SUMS.asc", root / "SHA256SUMS")
    names = set()
    for line in (root / "SHA256SUMS").read_text().splitlines():
        expected, name = line.split("  ", 1)
        if Path(name).name != name or name in names:
            raise ValueError("unsafe or duplicate checksum entry")
        names.add(name)
        path = root / name
        if path.is_symlink() or not path.is_file() or digest(path) != expected:
            raise ValueError(f"package asset checksum mismatch: {name}")
    actual = {p.name for p in root.iterdir()} - {"SHA256SUMS", "SHA256SUMS.asc"}
    if names != actual:
        raise ValueError("signed inventory does not exactly cover the package set")


def debian_policy(directory, key, keyring):
    identity = key[-16:]
    policies = directory / "policies" / identity
    keyrings = directory / "keyrings" / identity
    policies.mkdir(parents=True)
    keyrings.mkdir(parents=True)
    shutil.copyfile(keyring, keyrings / "peritus.pgp")
    (policies / "peritus.pol").write_text(
        '<?xml version="1.0"?>\n'
        '<!DOCTYPE Policy SYSTEM "https://www.debian.org/debsig/1.0/policy.dtd">\n'
        '<Policy xmlns="https://www.debian.org/debsig/1.0/">\n'
        f'<Origin Name="Peritus" id="{identity}" Description="Peritus releases"/>\n'
        f'<Selection><Required Type="origin" File="peritus.pgp" id="{key}"/></Selection>\n'
        f'<Verification><Required Type="origin" File="peritus.pgp" id="{key}"/></Verification>\n'
        '</Policy>\n'
    )
    return ["debsig-verify", "--policies-dir", directory / "policies",
            "--keyrings-dir", directory / "keyrings"]


def verify_debian_metadata(root, keyring):
    for suffix in ("dsc", "buildinfo", "changes"):
        files = sorted(root.glob(f"*.{suffix}"))
        if not files:
            raise ValueError(f"missing signed Debian {suffix}")
        for path in files:
            run("gpgv", "--keyring", keyring, path)
            section = None
            hashes = 0
            for line in path.read_text().splitlines():
                if line and not line.startswith(" "):
                    section = line.partition(":")[0]
                elif line.startswith(" ") and section == "Checksums-Sha256":
                    expected, size, name = line.split()
                    if Path(name).name != name:
                        raise ValueError("unsafe Debian source member")
                    payload = root / name
                    if digest(payload) != expected or payload.stat().st_size != int(size):
                        raise ValueError(f"Debian signed metadata mismatch: {path.name}/{name}")
                    hashes += 1
            if hashes == 0:
                raise ValueError(f"missing SHA-256 closure in {path.name}")


def tamper(package, target):
    shutil.copyfile(package, target)
    with target.open("r+b") as stream:
        if package.suffix == ".deb":
            if stream.read(8) != b"!<arch>\n":
                raise ValueError("Debian package is not an ar archive")
            while header := stream.read(60):
                size = int(header[48:58])
                if header[:16].rstrip(b" /").startswith(b"data.tar"):
                    stream.seek(size // 2, os.SEEK_CUR)
                    break
                stream.seek(size + size % 2, os.SEEK_CUR)
            else:
                raise ValueError("Debian package has no data archive")
        else:
            stream.seek(-1, os.SEEK_END)
        value = stream.read(1)
        stream.seek(-1, os.SEEK_CUR)
        stream.write(bytes([value[0] ^ 1]))


def installed(version):
    expected = {
        "peritus": "peritus", "peritusd": "peritusd",
        "peritus-tui": None, "peritus-linux-sandbox-helper": None,
    }
    if Path("/usr/bin/peritus").resolve() != Path("/usr/lib/peritus/peritus"):
        raise ValueError("public command does not resolve to the private sibling-binary directory")
    for name, prefix in expected.items():
        binary = Path("/usr/lib/peritus") / name
        metadata = binary.stat()
        if (binary.is_symlink() or metadata.st_mode & 0o7777 != 0o755
                or metadata.st_uid != 0 or metadata.st_gid != 0):
            raise ValueError(f"package binary must be regular, root-owned, and mode 0755: {name}")
        if prefix and run(binary, "--version", capture=True) != f"{prefix} {version}":
            raise ValueError(f"installed binary version mismatch: {name}")
    if "--endpoint" not in run("/usr/lib/peritus/peritus-tui", "--help", capture=True):
        raise ValueError("installed terminal client did not expose its native command interface")
    helper_output = must_fail("/usr/lib/peritus/peritus-linux-sandbox-helper")
    if "helper invocation is invalid" not in helper_output:
        raise ValueError(f"installed sandbox helper did not reach its protocol entry point: {helper_output}")
    output = terminal_failure("/usr/bin/peritus", "update")
    if "apt or dnf" not in output:
        raise ValueError(f"package update ownership check failed: {output.strip()}")


def main():
    kind, key = sys.argv[1:]
    root = Path("/packages")
    with tempfile.TemporaryDirectory(prefix="peritus-verify-") as temporary:
        directory = Path(temporary)
        keyring = directory / "release.pgp"
        run("gpg", "--batch", "--output", keyring, "--dearmor", "/release-key.asc")
        sums(root, keyring)
        record = json.loads((root / f"peritus-{kind}-build.json").read_text())
        archive = Path("/assets") / f"peritus-{kind}-{record['architecture']}.tar.gz"
        run("gpgv", "--keyring", keyring, archive.with_name(archive.name + ".asc"), archive)
        for path in Path("/assets").iterdir():
            if path.suffix in (".deb", ".rpm") and digest(path) != digest(root / path.name):
                raise ValueError("standalone package differs from the verified signed package set")
        if kind == "deb":
            command = debian_policy(directory, key, keyring)
            verify_debian_metadata(root, keyring)
            # Lintian's Debian-archive policy deliberately rejects all _extra
            # members, including debsigs' valid _gpgorigin extension. Lint the
            # exact hash-bound unsigned build, then verify signed bytes natively.
            run("runuser", "--user", "nobody", "--", "lintian", "--no-user-dirs", "--no-cfg",
                "--fail-on", "error", *sorted(Path("/unsigned").glob("*.changes")))
            packages = sorted(root.glob("*.deb"))
            for package in packages:
                run(*command, package)
            main_packages = [p for p in packages if p.name.startswith("peritus_")]
            if len(main_packages) != 1:
                raise ValueError("expected exactly one Peritus binary Debian package")
            package = main_packages[0]
            tampered = directory / "tampered.deb"
            tamper(package, tampered)
            must_fail(*command, tampered)
            unsigned = directory / "unsigned.deb"
            shutil.copyfile(package, unsigned)
            run("debsigs", "--delete=origin", unsigned)
            must_fail(*command, unsigned)
            # Debian slim deliberately excludes docs/manpages. Qualification must
            # install the entire package, not silently accept missing license files.
            Path("/etc/dpkg/dpkg.cfg.d/docker").unlink(missing_ok=True)
            run("apt-get", "install", "-y", "--no-install-recommends", package)
            installed(record["version"])
            differences = run("dpkg", "--verify", "peritus", capture=True)
            if differences:
                raise ValueError(f"installed Debian files differ from the package: {differences}")
            run("apt-get", "remove", "-y", "peritus")
        elif kind == "rpm":
            database = directory / "rpmdb"
            command = ["rpmkeys", "--dbpath", database, "--checksig", "--verbose"]
            run("rpm", "--dbpath", database, "--initdb")
            packages = sorted(root.glob("*.rpm"))
            for package in packages:
                must_fail(*command, package)
            run("rpm", "--dbpath", database, "--import", "/release-key.asc")
            for package in packages:
                output = run(*command, package, capture=True)
                if not rpm_signature_verified(output, key):
                    raise ValueError(f"RPM lacks a verified signature: {package.name}")
            main_packages = [p for p in packages if not p.name.endswith(".src.rpm")
                             and not p.name.startswith(("peritus-debuginfo-", "peritus-debugsource-"))]
            if len(main_packages) != 1:
                raise ValueError("expected exactly one Peritus binary RPM")
            package = main_packages[0]
            tampered = directory / "tampered.rpm"
            tamper(package, tampered)
            must_fail(*command, tampered)
            unsigned = directory / "unsigned.rpm"
            shutil.copyfile(package, unsigned)
            run("rpmsign", "--delsign", unsigned)
            unsigned_output = run(*command, unsigned, capture=True)
            if "signature" in unsigned_output.lower():
                raise ValueError("RPM signature removal test did not remove the signature")
            run("rpm", "--import", "/release-key.asc")
            must_fail("dnf", "--disablerepo=*", "--setopt=localpkg_gpgcheck=1", "install", "-y", unsigned)
            run("dnf", "--disablerepo=*", "--setopt=localpkg_gpgcheck=1", "install", "-y", package)
            installed(record["version"])
            run("rpm", "--verify", "peritus")
            run("dnf", "--disablerepo=*", "remove", "-y", "peritus")
        else:
            raise ValueError("unsupported format")
        if Path("/usr/bin/peritus").exists() or Path("/usr/lib/peritus").exists():
            raise ValueError("package removal left product binaries installed")
    print(f"PASS: {kind} signatures, metadata, tamper rejection, install, version, ownership, removal")


if __name__ == "__main__":
    main()
