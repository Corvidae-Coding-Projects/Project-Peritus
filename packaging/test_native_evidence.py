"""Retained native compilations must describe both actual archived product binaries."""

import copy
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
import warnings
from unittest.mock import patch
import zipfile

import archive
import native_inputs
import native_transport
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
        self.compiler = {"path": "fixture-clang-cl", "version": "fixture-version", "sha256": "f" * 64}
        self.resources.enter_context(patch.object(native_inputs.windows_release, "compiler_environment",
                                                return_value=({}, self.compiler)))

    def fixture(self, payload=b"non-release daemon fixture", windows=False):
        directory = self.root / "dist"
        package_name = "peritus-windows-x86_64" if windows else "peritus-macos-x86_64"
        loose = directory / package_name
        (loose / "bin").mkdir(parents=True)
        payloads = {"peritusd": payload, "peritus": b"non-release CLI fixture"}
        for binary, content in payloads.items():
            (loose / "bin" / (binary + (".exe" if windows else ""))).write_bytes(content)
        name = package_name + (".zip" if windows else ".tar.gz")
        archive.archive_tree(loose, directory / name, 1)
        (directory / (name + ".sha256")).write_bytes(
            rebuild.digest(directory / name).encode("ascii") + b"\n")
        package = {"schema_version": 1, "archive": "dist/" + name,
                   "checksum": "dist/" + name + ".sha256",
                   "build_started_unix": 1, "build_finished_unix": 2}
        (directory / rebuild.PACKAGE_RECORD).write_text(json.dumps(package))
        bound = {"candidate": self.candidate, "role": "primary", "workflow": None,
                 "package": "peritus-daemon", "binary": "peritusd",
                 "environment": {"system": "Darwin", "machine": "x86_64",
                                 "rustc": "fixture rustc", "image_version": "fixture-image"}}
        record = {"schema_version": 3, "kind": "native-release-binary-compilation", "binding": bound,
                  "library": {"schema_version": 2, "kind": "native-release-library-compilation",
                              "previous_library": None,
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
        evidence, records = {}, {}
        if windows:
            bound["environment"].update(system="Windows", machine="AMD64", cc=self.compiler)
            record["library"]["binding"] = copy.deepcopy(bound)
        for binary, content in payloads.items():
            if windows and binary != "peritusd":
                continue
            observed = copy.deepcopy(record)
            observed["binding"].update(binary=binary, package=native_transport.binary_package(binary))
            if binary == "peritus":
                observed["library"] = dict(copy.deepcopy(record["library"]),
                    binding=copy.deepcopy(observed["binding"]), previous_library=copy.deepcopy(record["library"]),
                    observation={"host": "fixture", "invocation": "fixture-cli-library",
                                 "started_unix_nanos": 3, "finished_unix_nanos": 4,
                                 "command": native_transport.cargo_arguments("library", binary)})
                observed["observation"].update(started_unix_nanos=5, finished_unix_nanos=6)
            observed["observation"]["command"] = native_transport.cargo_arguments(
                "binary", binary, "Windows" if windows else "Darwin")
            observed["observation"]["invocation"] = f"fixture-binary-{binary}"
            observed["binary"] = {"sha256": hashlib.sha256(content).hexdigest(), "byte_length": len(content)}
            path = self.root / "target/native-compile-record" / native_transport.record_filename(binary)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(observed))
            evidence[binary], records[binary] = path, observed
        return directory, evidence, records

    def test_windows_zip_bytes_and_pinned_compiler_are_bound_to_retained_daemon_stages(self):
        directory, _, records = self.fixture(windows=True)
        rebuild.record(directory, "primary")
        self.assertEqual(rebuild.load(directory, "primary")["binary_compilations"], records)
        loose = directory / "peritus-windows-x86_64/bin/peritusd.exe"
        loose.write_bytes(b"loose file is not the shipped binary")
        self.assertEqual(rebuild.load(directory, "primary")["binary_compilations"], records)
        compiler = dict(self.compiler, sha256="0" * 64)
        with patch.object(native_inputs.windows_release, "compiler_environment", return_value=({}, compiler)), \
                self.assertRaisesRegex(ValueError, "C compiler"):
            rebuild.load(directory, "primary")

    def test_windows_missing_record_wrong_bytes_attempt_or_command_fails_closed(self):
        directory, evidence, records = self.fixture(windows=True)
        original = records["peritusd"]
        path = evidence["peritusd"]
        for fault in ("missing", "bytes", "attempt", "command", "schema"):
            changed = copy.deepcopy(original)
            if fault == "bytes":
                changed["binary"]["sha256"] = "0" * 64
            elif fault == "attempt":
                changed["library"]["binding"]["workflow"] = {"GITHUB_RUN_ATTEMPT": "2"}
            elif fault == "command":
                changed["observation"]["command"] = native_transport.cargo_arguments("binary")
            elif fault == "schema":
                changed["schema_version"] = 2
            path.write_text(json.dumps(changed))
            if fault == "missing":
                path.unlink()
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                rebuild.record(directory, "primary")
            self.assertFalse((directory / rebuild.OBSERVATION).exists())
        path.write_text(json.dumps(original))

    def test_windows_zip_duplicate_link_empty_and_missing_daemon_are_rejected(self):
        target = self.root / "bad.zip"
        wanted = "peritus-windows-x86_64/bin/peritusd.exe"
        for fault in ("duplicate", "link", "empty", "missing"):
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with zipfile.ZipFile(target, "w") as output:
                    if fault == "duplicate":
                        output.writestr(wanted, b"fixture")
                        output.writestr(wanted, b"fixture")
                    elif fault == "link":
                        info = zipfile.ZipInfo(wanted)
                        info.external_attr = (stat.S_IFLNK | 0o777) << 16
                        output.writestr(info, b"somewhere")
                    elif fault == "empty":
                        output.writestr(wanted, b"")
                    else:
                        output.writestr("peritusd.exe", b"wrong path")
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                rebuild.archived_binaries(target, windows=True)

    def test_checksum_fixture_is_byte_exact_under_windows_text_translation(self):
        write_text = Path.write_text

        def windows_text(path, data, **options):
            options.setdefault("newline", "\r\n")
            return write_text(path, data, **options)

        with patch.object(Path, "write_text", windows_text):
            directory, _, _ = self.fixture()
        name = "peritus-macos-x86_64.tar.gz"
        checksum = directory / (name + ".sha256")
        expected = rebuild.digest(directory / name).encode("ascii") + b"\n"
        self.assertEqual(checksum.read_bytes(), expected)
        rebuild.outputs(directory)
        checksum.write_bytes(expected[:-1] + b"\r\n")
        with self.assertRaisesRegex(ValueError, "native output checksum mismatch"):
            rebuild.outputs(directory)

    def test_real_compilation_observations_are_retained_and_revalidated(self):
        directory, _, records = self.fixture()
        rebuild.record(directory, "primary")
        retained = rebuild.load(directory, "primary")
        self.assertEqual(retained["schema_version"], 4)
        self.assertEqual(retained["binary_compilations"], records)

    def test_a_loose_projection_cannot_substitute_for_the_archived_binary(self):
        directory, _, records = self.fixture()
        for binary in records:
            loose = directory / "peritus-macos-x86_64/bin" / binary
            loose.write_bytes(b"changed loose projection fixture")
        rebuild.record(directory, "primary")
        self.assertEqual(rebuild.load(directory, "primary")["binary_compilations"], records)
        observed = rebuild.archived_binaries(directory / "peritus-macos-x86_64.tar.gz")
        for binary, record in records.items():
            self.assertEqual(observed[binary], record["binary"])
            loose = directory / "peritus-macos-x86_64/bin" / binary
            self.assertNotEqual(observed[binary]["sha256"], rebuild.digest(loose))

    def test_missing_cross_role_changed_environment_or_wrong_bytes_cannot_be_recorded(self):
        directory, evidence, records = self.fixture()
        package, _ = rebuild.outputs(directory)
        self.assertEqual(rebuild.binary_compilations(directory, package, "primary"), records)
        for binary, record in records.items():
            for fault in ("missing", "role", "environment", "bytes", "workflow", "consumer", "schema"):
                changed = copy.deepcopy(record)
                if fault == "role":
                    changed["binding"]["role"] = "independent"
                elif fault == "environment":
                    changed["binding"]["environment"]["image_version"] = "other-image"
                    changed["library"]["binding"] = native_inputs.library_binding(changed["binding"])
                elif fault == "bytes":
                    changed["binary"]["sha256"] = "0" * 64
                elif fault == "consumer":
                    changed["binding"]["binary"] = "peritus" if binary == "peritusd" else "peritusd"
                elif fault == "schema":
                    changed["schema_version"] = 1
                evidence[binary].write_text(json.dumps(changed))
                if fault == "missing":
                    evidence[binary].unlink()
                variables = {"GITHUB_ACTIONS": "true"} if fault == "workflow" else {}
                with self.subTest(binary=binary, fault=fault), patch.dict(os.environ, variables), \
                        self.assertRaises((ValueError, FileNotFoundError)):
                    rebuild.record(directory, "primary")
                self.assertFalse((directory / rebuild.OBSERVATION).exists())
                evidence[binary].write_text(json.dumps(record))

    def test_duplicate_link_or_missing_archived_binary_is_rejected(self):
        directory, _, _ = self.fixture()
        target = directory / "bad-fixture.tar.gz"
        for binary in ("peritusd", "peritus"):
            for fault in ("duplicate", "link", "missing", "noncanonical"):
                with tarfile.open(target, "w:gz") as output:
                    for name in ("peritusd", "peritus"):
                        source = directory / "peritus-macos-x86_64/bin" / name
                        entry = f"peritus-macos-x86_64/bin/{name}"
                        if name != binary:
                            output.add(source, arcname=entry)
                        elif fault == "link":
                            info = tarfile.TarInfo(entry)
                            info.type = tarfile.SYMTYPE
                            info.linkname = "elsewhere"
                            output.addfile(info)
                        elif fault == "duplicate":
                            output.add(source, arcname=entry)
                            output.add(source, arcname=entry)
                        elif fault == "noncanonical":
                            output.add(source, arcname=name)
                with self.subTest(binary=binary, fault=fault), self.assertRaises(ValueError):
                    rebuild.archived_binaries(target)

    def test_extra_swapped_or_unpaired_compilation_records_are_rejected(self):
        directory, evidence, records = self.fixture()
        package, _ = rebuild.outputs(directory)
        for fault in ("extra", "swap", "missing-cli", "library", "invocation"):
            altered = copy.deepcopy(records)
            if fault == "extra":
                altered["peritus-tui"] = altered["peritus"]
            elif fault == "swap":
                altered["peritus"], altered["peritusd"] = altered["peritusd"], altered["peritus"]
            elif fault == "missing-cli":
                del altered["peritus"]
            elif fault == "library":
                altered["peritus"]["library"]["previous_library"]["observation"]["invocation"] = "another-library"
            else:
                altered["peritus"]["observation"]["invocation"] = altered["peritusd"]["observation"]["invocation"]
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                rebuild.validate_binary_compilations(altered, directory, package, "primary")
        (evidence["peritus"].parent / "unaccounted.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "exactly this target's staged binary"):
            rebuild.record(directory, "primary")
        self.assertFalse((directory / rebuild.OBSERVATION).exists())

    def test_loaded_assembly_rechecks_the_cli_and_rejects_old_schemas(self):
        directory, _, _ = self.fixture()
        rebuild.record(directory, "primary")
        path = directory / rebuild.OBSERVATION
        original = json.loads(path.read_bytes())
        for fault in ("bytes", "missing", "missing-map", "schema"):
            altered = copy.deepcopy(original)
            if fault == "bytes":
                altered["binary_compilations"]["peritus"]["binary"]["sha256"] = "0" * 64
            elif fault == "missing":
                del altered["binary_compilations"]["peritus"]
            elif fault == "missing-map":
                del altered["binary_compilations"]
            else:
                altered["schema_version"] = 2
            path.write_text(json.dumps(altered))
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                rebuild.load(directory, "primary")


if __name__ == "__main__":
    unittest.main()
