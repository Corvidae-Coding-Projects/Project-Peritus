"""Non-release fixtures for the native daemon library handoff boundary."""

import copy
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import native_transport as transport


class NativeTransportTests(unittest.TestCase):
    def fixture(self, root):
        tree = root / "producer" / "native-daemon"
        (tree / "release/deps").mkdir(parents=True)
        (tree / "release/libperitus_daemon.rlib").write_bytes(b"fixture library")
        (tree / "release/deps/libperitus_daemon-fixture.rlib").write_bytes(b"fixture library")
        binding = {"candidate": {"fixture": True}, "role": "primary", "environment": {"fixture": True},
                   "binary": "peritusd", "package": "peritus-daemon", "workflow": {"GITHUB_RUN_ATTEMPT": "1"}}
        observation = {"host": "fixture-host", "invocation": "fixture-library",
                       "started_unix_nanos": 1, "finished_unix_nanos": 2,
                       "command": ["cargo", "build", "--release", "--locked", "--package",
                                   "peritus-daemon", "--lib"]}
        bundle = root / "bundle"
        record = transport.save(tree, bundle, binding, observation)
        return tree, bundle, binding, record

    def test_round_trip_preserves_bytes_modes_and_mtime_without_a_product_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tree, bundle, binding, record = self.fixture(root)
            destination = root / "consumer" / "native-daemon"
            destination.parent.mkdir()
            self.assertEqual(transport.restore(bundle, destination, binding), record)
            for path in tree.rglob("*"):
                restored = destination / path.relative_to(tree)
                self.assertEqual(path.stat().st_mode, restored.stat().st_mode)
                self.assertAlmostEqual(path.stat().st_mtime, restored.stat().st_mtime, places=5)
                if path.is_file():
                    self.assertEqual(path.read_bytes(), restored.read_bytes())
            self.assertFalse((destination / "release/peritusd").exists())

    def test_changed_candidate_role_or_environment_is_rejected_before_extraction(self):
        for field in ("candidate", "role", "environment", "workflow", "binary", "package"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                _, bundle, binding, _ = self.fixture(root)
                expected = copy.deepcopy(binding)
                expected[field] = "changed fixture"
                destination = root / "consumer" / "native-daemon"
                destination.parent.mkdir()
                with self.assertRaisesRegex(ValueError, "binding"):
                    transport.restore(bundle, destination, expected)
                self.assertFalse(destination.exists())

    def test_archive_record_inventory_and_observation_tampering_fail_closed(self):
        for fault in ("hash", "length", "kind", "schema", "clock", "invocation", "command", "extra", "missing"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                _, bundle, binding, record = self.fixture(root)
                if fault == "extra":
                    (bundle / "unaccounted").write_bytes(b"extra fixture")
                elif fault == "missing":
                    (bundle / transport.ARCHIVE).unlink()
                else:
                    if fault == "hash":
                        record["archive_sha256"] = "0" * 64
                    elif fault == "length":
                        record["archive_byte_length"] += 1
                    elif fault in ("kind", "schema"):
                        record["kind" if fault == "kind" else "schema_version"] = "invalid"
                    elif fault == "clock":
                        record["observation"]["finished_unix_nanos"] = 0
                    elif fault == "command":
                        record["observation"]["command"] = ["cargo", "build"]
                    else:
                        record["observation"]["invocation"] = ""
                    (bundle / transport.RECORD).write_text(json.dumps(record))
                destination = root / "consumer" / "native-daemon"
                destination.parent.mkdir()
                with self.assertRaises((ValueError, FileNotFoundError)):
                    transport.restore(bundle, destination, binding)
                self.assertFalse(destination.exists())

    @unittest.skipIf(os.name == "nt", "Windows symlink creation requires a host privilege")
    def test_symbolic_links_cannot_enter_a_native_library_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tree, _, binding, record = self.fixture(root)
            (tree / "release/link").symlink_to("libperitus_daemon.rlib")
            with self.assertRaises(ValueError):
                transport.save(tree, root / "second-bundle", binding, record["observation"])
            self.assertFalse((root / "second-bundle").exists())

    def test_missing_libraries_or_prebuilt_product_are_not_a_library_stage(self):
        for fault in ("library", "binary", "windows-binary", "binary-dependency", "binary-fingerprint"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                tree, _, binding, record = self.fixture(root)
                if fault == "library":
                    (tree / "release/libperitus_daemon.rlib").unlink()
                elif fault == "binary":
                    (tree / "release/peritusd").write_bytes(b"fixture binary")
                elif fault == "windows-binary":
                    (tree / "release/peritusd.exe").write_bytes(b"fixture binary")
                elif fault == "binary-dependency":
                    (tree / "release/deps/peritusd-fixture").write_bytes(b"fixture binary")
                else:
                    path = tree / "release/.fingerprint/peritus-daemon-fixture/bin-peritusd"
                    path.parent.mkdir(parents=True)
                    path.write_bytes(b"fixture fingerprint")
                with self.assertRaises(ValueError):
                    transport.save(tree, root / "second-bundle", binding, record["observation"])
                self.assertFalse((root / "second-bundle").exists())

    def test_prebuilt_cli_outputs_are_rejected_before_creating_a_library_bundle(self):
        for name in ("release/peritus", "release/peritus.exe", "release/PERITUS.EXE", "release/deps/peritus-fixture",
                     "release/.fingerprint/peritus-cli-fixture/bin-peritus"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                tree, _, binding, record = self.fixture(root)
                product = tree / name
                product.parent.mkdir(parents=True, exist_ok=True)
                product.write_bytes(b"prebuilt CLI fixture must not enter a library bundle")
                with self.assertRaises(ValueError):
                    transport.save(tree, root / "second-bundle", binding, record["observation"])
                self.assertFalse((root / "second-bundle").exists())

    def test_unsafe_tar_members_and_resource_overflow_are_rejected(self):
        for name in ("../escape", "/absolute", "native-daemon/../escape", "native-daemon//bad",
                     "native-daemon/back\\slash", "native-daemon/C:stream", "native-daemon/CON.txt",
                     "native-daemon/file.", "native-daemon/file ", "native-daemon/wild*card",
                     "another-root/file", "native-daemon/control\n"):
            with self.subTest(name=name):
                member = tarfile.TarInfo(name)
                with self.assertRaises(ValueError):
                    transport.members([member])
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.CHRTYPE, tarfile.FIFOTYPE):
            member = tarfile.TarInfo("native-daemon/file")
            member.type = kind
            with self.assertRaises(ValueError):
                transport.members([member])
        member = tarfile.TarInfo("native-daemon/file")
        with self.assertRaises(ValueError):
            transport.members([member, member])
        with self.assertRaises(ValueError):
            transport.members([member, tarfile.TarInfo("native-daemon/FILE")])
        for value in (-1, float("inf")):
            member.mtime = value
            with self.assertRaises(ValueError):
                transport.members([member])
        member.mtime = 0
        member.mode = 0o4755
        with self.assertRaises(ValueError):
            transport.members([member])
        member.mode = 0o644
        member.size = 2
        with patch.object(transport, "MAX_EXPANDED_BYTES", 1), self.assertRaises(ValueError):
            transport.members([member])
        with patch.object(transport, "MAX_MEMBERS", 0), self.assertRaises(ValueError):
            transport.members([member])

    def test_admitted_hash_does_not_make_an_unsafe_archive_safe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, bundle, binding, record = self.fixture(root)
            archive = bundle / transport.ARCHIVE
            with tarfile.open(archive, "w:gz") as tar:
                entry = tarfile.TarInfo("../escape")
                entry.size = 1
                tar.addfile(entry, io.BytesIO(b"x"))
            record["archive_sha256"] = transport.digest(archive)
            record["archive_byte_length"] = archive.stat().st_size
            (bundle / transport.RECORD).write_text(json.dumps(record))
            destination = root / "consumer" / "native-daemon"
            destination.parent.mkdir()
            with self.assertRaises(ValueError):
                transport.restore(bundle, destination, binding)
            self.assertFalse((root / "escape").exists())

    def test_existing_destination_or_output_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tree, bundle, binding, record = self.fixture(root)
            with self.assertRaises(FileExistsError):
                transport.save(tree, bundle, binding, record["observation"])
            destination = root / "consumer" / "native-daemon"
            destination.mkdir(parents=True)
            sentinel = destination / "sentinel"
            sentinel.write_bytes(b"preserve fixture")
            with self.assertRaises(ValueError):
                transport.restore(bundle, destination, binding)
            self.assertEqual(sentinel.read_bytes(), b"preserve fixture")


if __name__ == "__main__":
    unittest.main()
