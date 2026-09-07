"""Native archive determinism and extraction-boundary regression tests."""

import os
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
import zipfile

from archive import archive_tree


class NativeArchiveTests(unittest.TestCase):
    def test_independent_trees_have_identical_bytes_and_preserve_payloads(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for suffix in ("tar.gz", "zip"):
                outputs = []
                for index, order in enumerate((('z', 'a'), ('a', 'z'))):
                    source = root / f"builder-{suffix}-{index}" / "peritus-native"
                    source.mkdir(parents=True)
                    for name in order:
                        (source / name).write_bytes(name.encode())
                        (source / name).chmod(0o700 if name == "a" else 0o600)
                        os.utime(source / name, (1_000_000_000 + index, 1_000_000_000 + index))
                    (source / ".hidden").write_bytes(b"retained")
                    os.utime(source, (1_000_000_000 + index, 1_000_000_000 + index))
                    output = root / f"build-{index}.{suffix}"
                    archive_tree(source, output, 1_234_567_890)
                    outputs.append(output)
                self.assertEqual(outputs[0].read_bytes(), outputs[1].read_bytes())
                self.check_payloads(outputs[0], suffix)

    def check_payloads(self, path, suffix):
        expected = ["peritus-native", "peritus-native/.hidden", "peritus-native/a", "peritus-native/z"]
        if suffix == "tar.gz":
            with tarfile.open(path) as archive:
                self.assertEqual(archive.getnames(), expected)
                self.assertEqual(archive.extractfile(expected[2]).read(), b"a")
                for member in archive.getmembers():
                    self.assertEqual((member.uid, member.gid, member.uname, member.gname), (0, 0, "root", "root"))
                    self.assertEqual(member.mtime, 1_234_567_890)
                if os.name != "nt":
                    self.assertEqual(archive.getmember(expected[2]).mode, 0o755)
        else:
            with zipfile.ZipFile(path) as archive:
                self.assertEqual([name.rstrip("/") for name in archive.namelist()], expected)
                self.assertEqual(archive.read(expected[2]), b"a")
                for member in archive.infolist():
                    self.assertEqual(member.date_time, (2009, 2, 13, 23, 31, 30))
                    self.assertEqual(member.create_system, 3)
                if os.name != "nt":
                    self.assertEqual(stat.S_IMODE(archive.getinfo(expected[2]).external_attr >> 16), 0o755)

    def test_changed_content_changes_both_archives(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            for suffix in ("tar.gz", "zip"):
                first, second = root / f"first.{suffix}", root / f"second.{suffix}"
                (source / "payload").write_bytes(b"first")
                archive_tree(source, first, 1_234_567_890)
                (source / "payload").write_bytes(b"second")
                archive_tree(source, second, 1_234_567_890)
                self.assertNotEqual(first.read_bytes(), second.read_bytes())

    def test_rejects_existing_outputs_invalid_epochs_and_nested_destinations(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            output = root / "retained.zip"
            output.write_bytes(b"retain")
            with self.assertRaises(FileExistsError):
                archive_tree(source, output, 0)
            self.assertEqual(output.read_bytes(), b"retain")
            for epoch in (-1, 0x100000000):
                with self.assertRaises(ValueError):
                    archive_tree(source, root / "invalid.zip", epoch)
            with self.assertRaises(ValueError):
                archive_tree(source, source / "nested.zip", 0)

    @unittest.skipIf(os.name == "nt", "Windows symlink creation requires a host privilege")
    def test_rejects_links_and_special_files_before_creating_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            link = source / "escape"
            link.symlink_to(root, target_is_directory=True)
            for suffix in ("tar.gz", "zip"):
                output = root / f"invalid.{suffix}"
                with self.assertRaises(ValueError):
                    archive_tree(source, output, 0)
                self.assertFalse(output.exists())
            link.unlink()
            os.mkfifo(source / "pipe")
            with self.assertRaises(ValueError):
                archive_tree(source, root / "pipe.zip", 0)


if __name__ == "__main__":
    unittest.main()
