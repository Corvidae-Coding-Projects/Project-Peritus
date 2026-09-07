"""The retained native compilation must describe the actual archive's daemon bytes."""

import copy
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import archive
import native_inputs
import rebuild


class NativeEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.resources = ExitStack()
        self.addCleanup(self.resources.close)
        self.root = Path(self.resources.enter_context(tempfile.TemporaryDirectory()))
        self.resources.enter_context(patch.object(rebuild, "ROOT", self.root))
        self.resources.enter_context(patch.object(rebuild, "command", return_value="fixture rustc"))
        self.candidate = {"explicit_non_release_fixture": True}
        self.resources.enter_context(patch.object(rebuild, "candidate", return_value=self.candidate))
        self.resources.enter_context(patch.object(native_inputs, "candidate", return_value=self.candidate))
        self.resources.enter_context(patch.dict(os.environ, {"ImageVersion": "fixture-image"}, clear=True))

    def fixture(self, payload=b"non-release daemon fixture"):
        directory = self.root / "dist"
        loose = directory / "peritus-macos-x86_64"
        (loose / "bin").mkdir(parents=True)
        binary = loose / "bin/peritusd"
        binary.write_bytes(payload)
        name = "peritus-macos-x86_64.tar.gz"
        archive.archive_tree(loose, directory / name, 1)
        (directory / (name + ".sha256")).write_text(rebuild.digest(directory / name) + "\n")
        package = {"schema_version": 1, "archive": "dist/" + name,
                   "checksum": "dist/" + name + ".sha256",
                   "build_started_unix": 1, "build_finished_unix": 2}
        (directory / rebuild.PACKAGE_RECORD).write_text(json.dumps(package))
        bound = {"candidate": self.candidate, "role": "primary", "workflow": None,
                 "package": "peritus-daemon", "binary": "peritusd",
                 "environment": {"system": "Darwin", "machine": "x86_64",
                                 "rustc": "fixture rustc", "image_version": "fixture-image"}}
        record = {"schema_version": 1, "kind": "native-daemon-binary-compilation", "binding": bound,
                  "library": {"schema_version": 1, "kind": "native-daemon-library-compilation",
                              "binding": copy.deepcopy(bound), "archive_byte_length": 1,
                              "archive_sha256": "0" * 64,
                              "observation": {"host": "fixture", "invocation": "fixture-library",
                                              "started_unix_nanos": 1, "finished_unix_nanos": 2,
                                              "command": ["cargo", "build", "--release", "--locked",
                                                          "--package", "peritus-daemon", "--lib"]}},
                  "observation": {"host": "fixture", "invocation": "fixture-binary",
                                  "started_unix_nanos": 3, "finished_unix_nanos": 4,
                                  "command": ["cargo", "build", "--release", "--locked",
                                              "--package", "peritus-daemon", "--bin", "peritusd"]},
                  "binary": {"sha256": hashlib.sha256(payload).hexdigest(), "byte_length": len(payload)}}
        evidence = self.root / "target/native-compile-record/native-daemon-build.json"
        evidence.parent.mkdir(parents=True)
        evidence.write_text(json.dumps(record))
        return directory, evidence, record

    def test_real_compilation_observations_are_retained_and_revalidated(self):
        directory, _, record = self.fixture()
        rebuild.record(directory, "primary")
        retained = rebuild.load(directory, "primary")
        self.assertEqual(retained["schema_version"], 2)
        self.assertEqual(retained["daemon_compilation"], record)

    def test_a_loose_projection_cannot_substitute_for_the_archived_binary(self):
        directory, _, record = self.fixture()
        loose = directory / "peritus-macos-x86_64/bin/peritusd"
        loose.write_bytes(b"changed loose projection fixture")
        rebuild.record(directory, "primary")
        self.assertEqual(rebuild.load(directory, "primary")["daemon_compilation"], record)
        observed = rebuild.archived_daemon(directory / "peritus-macos-x86_64.tar.gz")
        self.assertEqual(observed, record["binary"])
        self.assertNotEqual(observed["sha256"], rebuild.digest(loose))

    def test_missing_cross_role_changed_environment_or_wrong_bytes_cannot_be_recorded(self):
        directory, evidence, record = self.fixture()
        for fault in ("missing", "role", "environment", "bytes", "workflow"):
            changed = copy.deepcopy(record)
            if fault == "role":
                changed["binding"]["role"] = "independent"
            elif fault == "environment":
                changed["binding"]["environment"]["image_version"] = "other-image"
                changed["library"]["binding"] = copy.deepcopy(changed["binding"])
            elif fault == "bytes":
                changed["binary"]["sha256"] = "0" * 64
            evidence.write_text(json.dumps(changed))
            if fault == "missing":
                evidence.unlink()
            variables = {"GITHUB_ACTIONS": "true"} if fault == "workflow" else {}
            with self.subTest(fault=fault), patch.dict(os.environ, variables), \
                    self.assertRaises((ValueError, FileNotFoundError)):
                rebuild.record(directory, "primary")
            self.assertFalse((directory / rebuild.OBSERVATION).exists())

    def test_duplicate_link_or_missing_archived_daemon_is_rejected(self):
        directory, _, _ = self.fixture()
        target = directory / "bad-fixture.tar.gz"
        for fault in ("duplicate", "link", "missing"):
            with tarfile.open(target, "w:gz") as output:
                if fault != "missing":
                    source = directory / "peritus-macos-x86_64/bin/peritusd"
                    name = "peritus-macos-x86_64/bin/peritusd"
                    if fault == "link":
                        info = tarfile.TarInfo(name)
                        info.type = tarfile.SYMTYPE
                        info.linkname = "elsewhere"
                        output.addfile(info)
                    else:
                        output.add(source, arcname=name)
                        output.add(source, arcname=name)
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                rebuild.archived_daemon(target)


if __name__ == "__main__":
    unittest.main()
