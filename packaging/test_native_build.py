"""Explicit fixture tests for native compilation ownership and observation."""

import copy
from contextlib import ExitStack
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import native_build as build
import native_inputs as inputs
import native_transport as transport


@unittest.skipUnless(os.name == "posix", "native daemon handoff executes only on Unix hosts")
class NativeBuildTests(unittest.TestCase):
    def setUp(self):
        self.resources = ExitStack()
        self.addCleanup(self.resources.close)
        self.root = Path(self.resources.enter_context(tempfile.TemporaryDirectory()))
        self.resources.enter_context(patch.object(build, "ROOT", self.root))
        self.bound = {"candidate": {"explicit_fixture": True}, "role": "primary",
                      "package": "peritus-daemon", "binary": "peritusd", "workflow": None,
                      "environment": {"cargo_build_jobs": 2}}
        self.binding = self.resources.enter_context(patch.object(inputs, "binding", return_value=self.bound))
        self.resources.enter_context(patch.object(inputs, "normalize_verified_sources"))

    def fake_cargo(self, arguments, **kwargs):
        self.assertEqual(kwargs["cwd"], self.root)
        self.assertTrue(kwargs["check"])
        self.assertEqual(kwargs["env"]["CARGO_INCREMENTAL"], "0")
        self.assertEqual(kwargs["env"]["CARGO_BUILD_JOBS"], "2")
        self.assertEqual(kwargs["env"]["CARGO_TARGET_DIR"], str(self.root / "target/native-daemon"))
        self.assertEqual(arguments[:6], ["cargo", "build", "--release", "--locked", "--package", "peritus-daemon"])
        target = self.root / "target/native-daemon/release"
        if arguments[6:] == ["--lib"]:
            (target / "deps").mkdir(parents=True)
            (target / "libperitus_daemon.rlib").write_bytes(b"fixture library")
            (target / "deps/libperitus_daemon-fixture.rlib").write_bytes(b"fixture library")
        else:
            self.assertEqual(arguments[6:], ["--bin", "peritusd"])
            self.assertEqual((target / "libperitus_daemon.rlib").read_bytes(), b"fixture library")
            product = target / "peritusd"
            product.write_bytes(b"explicit non-executable product fixture")
            product.chmod(0o755)

    def prepare(self):
        with patch.object(build.subprocess, "run", side_effect=self.fake_cargo):
            build.library()
        shutil.move(self.root / "target/native-daemon", self.root / "target/retained-producer")

    def test_binary_is_actually_compiled_and_its_distinct_observation_binds_the_bytes(self):
        self.prepare()
        with patch.object(build.subprocess, "run", side_effect=self.fake_cargo) as cargo:
            build.binary()
        self.assertEqual(cargo.call_count, 1)
        product = self.root / "target/release/peritusd"
        record = json.loads((self.root / "target/native-daemon-build.json").read_bytes())
        inputs.validate_binary_record(record, self.bound["candidate"], "primary", product)
        self.assertEqual(record["library"]["binding"], self.bound)
        self.assertNotEqual(record["library"]["observation"]["invocation"], record["observation"]["invocation"])
        self.assertEqual(record["observation"]["command"], build.cargo_arguments("binary"))
        self.assertEqual(record["library"]["observation"]["command"], build.cargo_arguments("library"))
        for fault in ("candidate", "role", "binary", "clock", "invocation", "command"):
            altered = copy.deepcopy(record)
            if fault == "candidate":
                altered["binding"]["candidate"] = {"changed_fixture": True}
            elif fault == "role":
                altered["binding"]["role"] = "independent"
            elif fault == "binary":
                altered["binary"]["sha256"] = "0" * 64
            elif fault == "clock":
                altered["observation"]["started_unix_nanos"] = 1
            elif fault == "command":
                altered["observation"]["command"] = build.cargo_arguments("library")
            else:
                altered["observation"]["invocation"] = altered["library"]["observation"]["invocation"]
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                inputs.validate_binary_record(altered, self.bound["candidate"], "primary", product)

    def test_failure_drift_or_missing_library_cannot_emit_a_success_record(self):
        self.prepare()
        failure = subprocess.CalledProcessError(1, ["fixture cargo"])
        with patch.object(build.subprocess, "run", side_effect=failure), self.assertRaises(subprocess.CalledProcessError):
            build.binary()
        self.assertFalse((self.root / "target/native-daemon-build.json").exists())
        self.assertFalse((self.root / "target/release/peritusd").exists())

    def test_library_input_drift_never_creates_a_transfer_artifact(self):
        self.binding.side_effect = [self.bound, {"changed_fixture": True}]
        with patch.object(build.subprocess, "run", side_effect=self.fake_cargo), self.assertRaises(ValueError):
            build.library()
        self.assertFalse((self.root / "target/native-daemon-libraries").exists())

    def test_existing_output_and_missing_or_cross_role_input_fail_before_cargo(self):
        self.prepare()
        altered = copy.deepcopy(self.bound)
        altered["role"] = "independent"
        self.binding.return_value = altered
        with patch.object(build.subprocess, "run") as cargo, self.assertRaises(ValueError):
            build.binary()
        cargo.assert_not_called()
        self.binding.return_value = self.bound
        product = self.root / "target/release/peritusd"
        product.parent.mkdir()
        product.write_bytes(b"preserve fixture")
        with patch.object(build.subprocess, "run") as cargo, self.assertRaises(ValueError):
            build.binary()
        cargo.assert_not_called()
        self.assertEqual(product.read_bytes(), b"preserve fixture")
        with self.assertRaises(ValueError):
            build.cargo_arguments("unknown")


class NativeInputTests(unittest.TestCase):
    def test_roles_and_complete_workflow_identity_are_required(self):
        with patch.object(inputs, "candidate", return_value={"fixture": True}), \
                patch.object(inputs, "environment", return_value={"fixture": True}):
            for role in ("", "other", "primary; exit 0"):
                with patch.dict(os.environ, {inputs.ROLE_ENV: role}, clear=True), self.assertRaises(ValueError):
                    inputs.binding()
            with patch.dict(os.environ, {inputs.ROLE_ENV: "primary", "GITHUB_ACTIONS": "true"}, clear=True), \
                    self.assertRaises(ValueError):
                inputs.binding()
            values = {inputs.ROLE_ENV: "independent", "GITHUB_ACTIONS": "true",
                      **{key: "explicit-fixture" for key in inputs.WORKFLOW_KEYS}}
            with patch.dict(os.environ, values, clear=True):
                self.assertEqual(inputs.binding()["workflow"], {key: "explicit-fixture" for key in inputs.WORKFLOW_KEYS})

    def test_profile_wrapper_cross_target_and_capacity_overrides_are_rejected(self):
        invalid = {"RUSTFLAGS": "-Copt-level=0", "CARGO_PROFILE_RELEASE_LTO": "false",
                   "RUSTC": "other-rustc", "CARGO_BUILD_RUSTC": "other-rustc",
                   "RUSTC_BOOTSTRAP": "1",
                   "RUSTC_WRAPPER": "fixture-wrapper", "CARGO_TARGET_DIR": "another-target",
                   "CARGO_BUILD_TARGET": "other-platform", "CARGO_INCREMENTAL": "1",
                   "CARGO_BUILD_JOBS": "8", "CARGO_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS": "changed"}
        for key, value in invalid.items():
            with self.subTest(key=key), patch.dict(os.environ, {key: value}, clear=True), \
                    patch.object(inputs, "command") as command, self.assertRaises(ValueError):
                inputs.environment()
            command.assert_not_called()

    def test_timestamp_preparation_requires_matching_source_bytes_first(self):
        expected = {"fixture": True}
        with patch.object(inputs, "candidate", return_value={"changed_fixture": True}), \
                patch.object(inputs.os, "utime") as timestamp, self.assertRaises(ValueError):
            inputs.normalize_verified_sources(expected)
        timestamp.assert_not_called()


if __name__ == "__main__":
    unittest.main()
