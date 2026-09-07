"""Staged archive admission, exact-byte retention, and extraction regressions."""

import hashlib
import io
import os
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
import zipfile

from archive import archive_tree
from qualification import prepare


class StagedQualificationTests(unittest.TestCase):
    def fixture(self, base, platform="linux"):
        package = f"peritus-{platform}-x86_64"
        tree = base / "tree" / package
        tree.mkdir(parents=True)
        (tree / "bin").mkdir()
        (tree / "bin/peritus").write_bytes(b"release fixture, not an executable")
        (tree / "bin/peritus").chmod(0o755)
        (tree / "manifest.toml").write_bytes(b"schema_version = 1\n")
        source = base / "source"
        source.mkdir()
        extension = "zip" if platform == "windows" else "tar.gz"
        archive = source / f"{package}.{extension}"
        archive_tree(tree, archive, 1788740000)
        self.checksum(archive)
        root = base / "consumer"
        root.mkdir()
        return package, source, archive, root

    def checksum(self, archive):
        path = archive.with_name(archive.name + ".sha256")
        path.write_bytes(hashlib.sha256(archive.read_bytes()).hexdigest().encode() + b"\n")

    def test_restores_exact_archive_and_executable_modes_not_loose_download(self):
        for platform in ("linux", "windows"):
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temporary:
                package, source, archive, root = self.fixture(Path(temporary), platform)
                loose = source / package
                loose.mkdir()
                (loose / "manifest.toml").write_bytes(b"wrong loose tree")
                prepare(source, root, package)
                self.assertEqual((root / "dist" / archive.name).read_bytes(), archive.read_bytes())
                self.assertEqual((root / "dist" / package / "manifest.toml").read_bytes(),
                                 b"schema_version = 1\n")
                if platform == "linux" and os.name != "nt":
                    self.assertEqual((root / "dist" / package / "bin/peritus").stat().st_mode
                                     & 0o777, 0o755)
                with self.assertRaisesRegex(ValueError, "fresh"):
                    prepare(source, root, package)

    def test_checksum_mismatch_has_no_materialization(self):
        with tempfile.TemporaryDirectory() as temporary:
            package, source, archive, root = self.fixture(Path(temporary))
            archive.write_bytes(archive.read_bytes() + b"tampered")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                prepare(source, root, package)
            self.assertFalse((root / "dist").exists())

    def test_tar_rejects_traversal_links_duplicates_and_missing_manifest(self):
        for invalid in ("traversal", "symlink", "hardlink", "duplicate", "manifest", "mode"):
            with self.subTest(invalid=invalid), tempfile.TemporaryDirectory() as temporary:
                package, source, archive, root = self.fixture(Path(temporary))
                with tarfile.open(archive, "w:gz") as output:
                    directory = tarfile.TarInfo(package)
                    directory.type, directory.mode = tarfile.DIRTYPE, 0o755
                    output.addfile(directory)
                    member = tarfile.TarInfo(f"{package}/manifest.toml")
                    member.mode = 0o644
                    if invalid == "traversal":
                        member.name = f"{package}/../../escaped"
                    elif invalid in ("symlink", "hardlink"):
                        member.type = tarfile.SYMTYPE if invalid == "symlink" else tarfile.LNKTYPE
                        member.linkname = "../../escaped"
                    elif invalid == "manifest":
                        member.name = f"{package}/not-the-manifest"
                    elif invalid == "mode":
                        member.mode = 0o4755
                    output.addfile(member, io.BytesIO())
                    if invalid == "duplicate":
                        output.addfile(member, io.BytesIO())
                self.checksum(archive)
                with self.assertRaises(ValueError):
                    prepare(source, root, package)
                self.assertFalse((root / "dist").exists())
                self.assertEqual(list((root / "target").iterdir()), [])

    def test_zip_rejects_nonregular_members(self):
        with tempfile.TemporaryDirectory() as temporary:
            package, source, archive, root = self.fixture(Path(temporary), "windows")
            with zipfile.ZipFile(archive, "w") as output:
                directory = zipfile.ZipInfo(package + "/")
                directory.external_attr = (stat.S_IFDIR | 0o755) << 16
                output.writestr(directory, b"")
                member = zipfile.ZipInfo(f"{package}/manifest.toml")
                member.external_attr = (stat.S_IFLNK | 0o777) << 16
                output.writestr(member, b"../../escaped")
            self.checksum(archive)
            with self.assertRaisesRegex(ValueError, "non-regular"):
                prepare(source, root, package)
            self.assertFalse((root / "dist").exists())


if __name__ == "__main__":
    unittest.main()
