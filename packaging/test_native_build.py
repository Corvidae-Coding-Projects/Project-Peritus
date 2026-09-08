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


class NativeBuildFixture(unittest.TestCase):
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
        self.assertEqual(arguments[:5], ["cargo", "build", "--release", "--locked", "--package"])
        target = self.root / "target/native-daemon/release"
        if arguments[6:] == ["--lib"]:
            self.assertIn(arguments[5], ("peritus-daemon", "peritus-cli"))
            crate = arguments[5].replace("-", "_")
            (target / "deps").mkdir(parents=True, exist_ok=True)
            (target / f"lib{crate}.rlib").write_bytes(b"fixture library")
            (target / f"deps/lib{crate}-fixture.rlib").write_bytes(b"fixture library")
        else:
            binary = arguments[7]
            self.assertIn(binary, ("peritusd", "peritus"))
            self.assertEqual(arguments[5:], [transport.binary_package(binary), "--bin", binary])
            self.assertEqual((target / "libperitus_daemon.rlib").read_bytes(), b"fixture library")
            product = target / binary
            product.write_bytes(b"explicit non-executable product fixture: " + binary.encode())
            product.chmod(0o755)

    def prepare(self):
        with patch.object(build.subprocess, "run", side_effect=self.fake_cargo):
            build.library()
        shutil.move(self.root / "target/native-daemon", self.root / "target/retained-producer")

    def prepare_cli(self):
        self.prepare()
        self.binding.return_value = dict(self.bound, package="peritus-cli", binary="peritus")
        with patch.object(build.subprocess, "run", side_effect=self.fake_cargo) as cargo:
            build.library("peritus")
        self.assertEqual(cargo.call_count, 1)
        self.assertFalse((self.root / "target/native-daemon/release/peritus").exists())
        shutil.move(self.root / "target/native-daemon", self.root / "target/retained-cli-producer")


@unittest.skipUnless(os.name == "posix", "Unix executable-mode fixtures require a Unix filesystem")
class NativeBuildTests(NativeBuildFixture):
    def test_binary_is_actually_compiled_and_its_distinct_observation_binds_the_bytes(self):
        self.prepare()
        with patch.object(build.subprocess, "run", side_effect=self.fake_cargo) as cargo:
            build.binary()
        self.assertEqual(cargo.call_count, 1)
        product = self.root / "target/release/peritusd"
        record = json.loads((self.root / "target/native-peritusd-build.json").read_bytes())
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
        self.assertFalse((self.root / "target/native-peritusd-build.json").exists())
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

    def test_cli_compilation_retains_its_own_command_and_original_same_role_library(self):
        self.prepare_cli()
        consumer = dict(self.bound, package="peritus-cli", binary="peritus")
        self.binding.return_value = consumer
        with patch.object(build.subprocess, "run", side_effect=self.fake_cargo) as cargo:
            build.binary("peritus")
        self.assertEqual(cargo.call_count, 1)
        product = self.root / "target/release/peritus"
        record = json.loads((self.root / "target/native-peritus-build.json").read_bytes())
        inputs.validate_binary_record(record, self.bound["candidate"], "primary", product, "peritus")
        self.assertEqual(record["binding"], consumer)
        self.assertEqual(record["library"]["binding"], consumer)
        self.assertEqual(record["library"]["previous_library"]["binding"], self.bound)
        self.assertEqual(record["observation"]["command"],
                         ["cargo", "build", "--release", "--locked", "--package", "peritus-cli", "--bin", "peritus"])
        self.assertFalse((self.root / "target/release/peritusd").exists())
        with self.assertRaises(ValueError):
            inputs.validate_binary_record(record, self.bound["candidate"], "primary", product, "peritusd")
        for fault in ("package", "consumer", "command", "library-role", "library-package", "schema"):
            altered = copy.deepcopy(record)
            if fault == "package":
                altered["binding"]["package"] = "peritus-daemon"
            elif fault == "consumer":
                altered["binding"]["binary"] = "peritusd"
            elif fault == "command":
                altered["observation"]["command"] = build.cargo_arguments("binary", "peritusd")
            elif fault == "library-role":
                altered["library"]["binding"]["role"] = "independent"
            elif fault == "library-package":
                altered["library"]["binding"]["package"] = "peritus-daemon"
            else:
                altered["schema_version"] = 1
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                inputs.validate_binary_record(altered, self.bound["candidate"], "primary", product, "peritus")

    def test_failed_cli_compilation_cannot_create_a_product_or_success_record(self):
        self.prepare_cli()
        self.binding.return_value = dict(self.bound, package="peritus-cli", binary="peritus")
        failure = subprocess.CalledProcessError(1, ["fixture cargo"])
        with patch.object(build.subprocess, "run", side_effect=failure), self.assertRaises(subprocess.CalledProcessError):
            build.binary("peritus")
        self.assertFalse((self.root / "target/native-peritus-build.json").exists())
        self.assertFalse((self.root / "target/release/peritus").exists())

    def test_cli_parent_identity_order_and_attempt_are_mandatory_before_final_cargo(self):
        self.prepare_cli()
        path = self.root / "target/native-cli-libraries/libraries.json"
        original = json.loads(path.read_bytes())
        for fault in ("missing", "role", "attempt", "recursive", "order", "invocation", "old-schema"):
            changed = copy.deepcopy(original)
            parent = changed["previous_library"]
            if fault == "missing":
                changed["previous_library"] = None
            elif fault == "role":
                parent["binding"]["role"] = "independent"
            elif fault == "attempt":
                parent["binding"]["workflow"] = {"GITHUB_RUN_ATTEMPT": "2"}
            elif fault == "recursive":
                parent["previous_library"] = copy.deepcopy(parent)
            elif fault == "order":
                changed["observation"]["started_unix_nanos"] = 1
            elif fault == "invocation":
                changed["observation"]["invocation"] = parent["observation"]["invocation"]
            else:
                changed["schema_version"] = 1
            path.write_text(json.dumps(changed))
            with self.subTest(fault=fault), patch.object(build.subprocess, "run") as cargo, \
                    self.assertRaises(ValueError):
                build.binary("peritus")
            cargo.assert_not_called()
            self.assertFalse((self.root / "target/native-daemon").exists())
            self.assertFalse((self.root / "target/native-peritus-build.json").exists())
        path.write_text(json.dumps(original))


class WindowsNativeBuildTests(NativeBuildFixture):
    def test_windows_daemon_uses_pinned_compiler_and_brepro_in_a_distinct_final_invocation(self):
        self.bound["environment"].update(system="Windows", cc={"path": "fixture-clang-cl"})
        self.prepare()

        def windows_cargo(arguments, **kwargs):
            self.assertEqual(arguments, transport.cargo_arguments("binary", "peritusd", "Windows"))
            self.assertEqual(kwargs["env"]["CC_x86_64_pc_windows_msvc"], "fixture-clang-cl")
            self.assertTrue(kwargs["check"])
            (self.root / "target/native-daemon/release/peritusd.exe").write_bytes(b"MZ explicit fixture")

        with patch.object(build.subprocess, "run", side_effect=windows_cargo) as cargo:
            build.binary()
        cargo.assert_called_once()
        product = self.root / "target/release/peritusd.exe"
        record = json.loads((self.root / "target/native-peritusd-build.json").read_bytes())
        inputs.validate_binary_record(record, self.bound["candidate"], "primary", product)
        self.assertEqual(record["library"]["binding"]["environment"]["cc"], {"path": "fixture-clang-cl"})
        self.assertFalse((self.root / "target/release/peritusd").exists())
        changed = copy.deepcopy(record)
        changed["observation"]["command"] = transport.cargo_arguments("binary", "peritusd")
        with self.assertRaises(ValueError):
            inputs.validate_binary_record(changed, self.bound["candidate"], "primary", product)

    def test_windows_non_executable_output_cannot_emit_a_success_record(self):
        self.bound["environment"].update(system="Windows", cc={"path": "fixture-clang-cl"})
        self.prepare()

        def invalid_cargo(*_args, **_kwargs):
            (self.root / "target/native-daemon/release/peritusd.exe").write_bytes(b"not a PE image")

        with patch.object(build.subprocess, "run", side_effect=invalid_cargo), self.assertRaises(ValueError):
            build.binary()
        self.assertFalse((self.root / "target/release/peritusd.exe").exists())
        self.assertFalse((self.root / "target/native-peritusd-build.json").exists())


class NativeInputTests(unittest.TestCase):
    def test_windows_environment_records_verified_native_compiler_instead_of_ambient_cc(self):
        compiler = {"path": "fixture-clang-cl", "version": "fixture-version", "sha256": "f" * 64}
        with patch.object(inputs.platform, "system", return_value="Windows"), \
                patch.object(inputs.platform, "machine", return_value="AMD64"), \
                patch.object(inputs.windows_release, "compiler_environment", return_value=({}, compiler)), \
                patch.object(inputs, "command", return_value="fixture") as command, \
                patch.dict(os.environ, {}, clear=True):
            observed = inputs.environment()
        self.assertEqual(observed["cc"], compiler)
        self.assertEqual(observed["system"], "Windows")
        self.assertNotIn(unittest.mock.call("cc", "--version"), command.call_args_list)

    def test_consumer_mapping_is_closed_before_source_or_environment_commands(self):
        for binary in ("", "unknown", "../peritus", "peritus; exit 0", None, []):
            with self.subTest(binary=binary), patch.object(inputs, "command") as command:
                with self.assertRaises(ValueError):
                    inputs.binding(binary)
                command.assert_not_called()
                with self.assertRaises(ValueError):
                    build.cargo_arguments("binary", binary)
                with self.assertRaises(ValueError):
                    transport.record_filename(binary)
        self.assertEqual(build.cargo_arguments("library", "peritus")[-3:], ["--package", "peritus-cli", "--lib"])
        with patch.object(inputs, "candidate", return_value={"fixture": True}), \
                patch.object(inputs, "environment", return_value={"fixture": True}), \
                patch.dict(os.environ, {inputs.ROLE_ENV: "primary"}, clear=True):
            self.assertEqual(inputs.binding("peritus")["package"], "peritus-cli")
            self.assertEqual(inputs.library_binding(inputs.binding("peritus")), inputs.binding("peritus"))
            self.assertEqual(inputs.daemon_binding(inputs.binding("peritus")), inputs.binding("peritusd"))

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
