"""Explicit non-release fixtures for native independent-rebuild admission."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import rebuild


class NativeRebuildTests(unittest.TestCase):
    def fixture(self, directory, role, payload=b"non-release archive fixture", windows=False):
        directory.mkdir()
        name = "peritus-windows-x86_64.zip" if windows else "peritus-linux-x86_64.tar.gz"
        (directory / name).write_bytes(payload)
        (directory / (name + ".sha256")).write_bytes(
            hashlib.sha256(payload).hexdigest().encode() + b"\n")
        prefix = "dist\\" if windows else "dist/"
        package = {"schema_version": 1, "archive": prefix + name,
                   "checksum": prefix + name + ".sha256",
                   "build_started_unix": 1, "build_finished_unix": 2}
        (directory / rebuild.PACKAGE_RECORD).write_text(json.dumps(package))
        with patch.object(rebuild, "candidate", return_value={"explicit_test_fixture": True}), \
                patch.object(rebuild, "command", return_value="fixture rustc identity"):
            rebuild.record(directory, role)
        return directory

    def test_complete_native_inventory_matches_despite_real_distinct_observations(self):
        for windows in (False, True):
            with self.subTest(windows=windows), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                primary = self.fixture(root / "primary", "primary", windows=windows)
                independent = self.fixture(root / "independent", "independent", windows=windows)
                report = root / "comparison.json"
                rebuild.compare(primary, independent, report)
                result = json.loads(report.read_bytes())
                self.assertTrue(result["reproducible"])
                self.assertTrue(result["compatible"])
                self.assertTrue(result["byte_identical"])
                self.assertEqual(len(result["primary"]["artifacts"]), 2)
                self.assertNotEqual(result["primary"]["invocation"], result["independent"]["invocation"])
                self.assertNotEqual(result["primary"]["observed_unix_nanos"],
                                    result["independent"]["observed_unix_nanos"])
                with self.assertRaises(FileExistsError):
                    rebuild.compare(primary, independent, report)

    def test_different_archive_and_checksum_retain_a_failing_comparison(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            primary = self.fixture(root / "primary", "primary")
            independent = self.fixture(root / "independent", "independent", b"changed fixture")
            report = root / "comparison.json"
            with self.assertRaisesRegex(ValueError, "rebuild differs"):
                rebuild.compare(primary, independent, report)
            result = json.loads(report.read_bytes())
            self.assertFalse(result["reproducible"])
            self.assertFalse(result["byte_identical"])
            self.assertTrue(result["compatible"])
            self.assertEqual(len(result["differences"]), 2)

    def test_input_drift_is_not_misreported_as_different_artifact_bytes(self):
        for field in ("candidate", "environment"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                primary = self.fixture(root / "primary", "primary")
                independent = self.fixture(root / "independent", "independent")
                path = independent / rebuild.OBSERVATION
                observation = json.loads(path.read_bytes())
                observation[field]["changed_fixture"] = True
                path.write_text(json.dumps(observation))
                report = root / "comparison.json"
                with self.assertRaises(ValueError):
                    rebuild.compare(primary, independent, report)
                result = json.loads(report.read_bytes())
                self.assertFalse(result["reproducible"])
                self.assertFalse(result["compatible"])
                self.assertTrue(result["byte_identical"])

    def test_reusing_a_directory_or_invocation_is_not_an_independent_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            primary = self.fixture(root / "primary", "primary")
            independent = self.fixture(root / "independent", "independent")
            with self.assertRaisesRegex(ValueError, "different output directory"):
                rebuild.compare(primary, primary, root / "comparison.json")
            path = independent / rebuild.OBSERVATION
            observation = json.loads(path.read_bytes())
            observation["invocation"] = json.loads((primary / rebuild.OBSERVATION).read_bytes())["invocation"]
            path.write_text(json.dumps(observation))
            with self.assertRaisesRegex(ValueError, "different invocation"):
                rebuild.compare(primary, independent, root / "comparison.json")

    def test_tampered_unaccounted_missing_or_noncanonical_outputs_are_rejected(self):
        for fault in ("tamper", "extra", "missing", "path", "clock", "observation"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                primary = self.fixture(root / "primary", "primary")
                independent = self.fixture(root / "independent", "independent")
                if fault == "tamper":
                    (independent / "peritus-linux-x86_64.tar.gz").write_bytes(b"tampered fixture")
                elif fault == "extra":
                    (independent / "unaccounted.tar.gz").write_bytes(b"extra fixture")
                elif fault == "missing":
                    (independent / "peritus-linux-x86_64.tar.gz").unlink()
                else:
                    name = rebuild.OBSERVATION if fault == "observation" else rebuild.PACKAGE_RECORD
                    path = independent / name
                    value = json.loads(path.read_bytes())
                    if fault == "path":
                        value["archive"] = "../peritus-linux-x86_64.tar.gz"
                    elif fault == "clock":
                        value["build_finished_unix"] = 0
                    else:
                        value["artifacts"][0]["sha256"] = "0" * 64
                    path.write_text(json.dumps(value))
                with self.assertRaises((ValueError, FileNotFoundError)):
                    rebuild.compare(primary, independent, root / "comparison.json")
                self.assertFalse((root / "comparison.json").exists())


if __name__ == "__main__":
    unittest.main()
