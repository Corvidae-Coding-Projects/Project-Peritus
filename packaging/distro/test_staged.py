"""Staged packaging admits only complete, same-candidate native build trees."""

import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import build
import compiled


class NativeRecipeTests(unittest.TestCase):
    def test_rpm_check_compilation_uses_the_native_build_environment_without_installing(self):
        with patch.object(build, "container") as container, \
                patch.object(build, "version", return_value="1.2.3"), \
                patch.object(build, "maintainer", return_value="A <a@example.invalid>"):
            build.run_native_build("rpm", Path("root"), Path("peritus-1.2.3"), 123, 4, "compile-checks")
            args = container.call_args.args
            self.assertEqual(args[2:4], ("rpmbuild", "-bc"))
            self.assertIn("--noprep", args)
            self.assertIn("--noclean", args)
            self.assertIn("peritus_compile_checks 1", args)
            for forbidden in ("--nocheck", "--short-circuit", "--nodebuginfo", "--nodeps", "-bi"):
                self.assertNotIn(forbidden, args)
            container.reset_mock()
            with self.assertRaises(ValueError):
                build.run_native_build("deb", Path("root"), Path("source"), 123, 4, "compile-checks")
            container.assert_not_called()

    def test_rpm_compiles_and_runs_the_same_check_command(self):
        spec = (build.ROOT / "packaging/rpm/peritus.spec").read_text()
        self.assertIn("%global peritus_check cargo --config .cargo/vendor.toml test --frozen -p peritus-launcher --features system-package", spec)
        self.assertIn("%if 0%{?peritus_compile_checks}\n%{peritus_check} --no-run\n%endif", spec)
        self.assertIn("%{peritus_check} update::tests", spec.split("%check\n", 1)[1])

    def test_direct_debian_compilation_exports_the_full_recipe_build_flags(self):
        rules = (build.ROOT / "packaging/debian/rules").read_text()
        hardening = rules.index("export DEB_BUILD_MAINT_OPTIONS = hardening=+all")
        export_flags = rules.index("DPKG_EXPORT_BUILDFLAGS = 1")
        include_flags = rules.index("include /usr/share/dpkg/default.mk")
        rust_flags = rules.index("export RUSTFLAGS =")
        self.assertLess(hardening, export_flags)
        self.assertLess(export_flags, include_flags)
        self.assertLess(include_flags, rust_flags)

    def test_stages_preserve_full_native_recipes_and_tests(self):
        with patch.object(build, "container") as container, \
                patch.object(build, "version", return_value="1.2.3"), \
                patch.object(build, "maintainer", return_value="A <a@example.invalid>"):
            for kind in ("deb", "rpm"):
                for stage in ("full", "compile", "package"):
                    build.run_native_build(kind, Path("root"), Path("peritus-1.2.3"), 1234567890, 4, stage)
                    args = container.call_args.args
                    self.assertEqual(args[:2], (kind, [(Path("root"), "/build", False)]))
                    self.assertEqual(container.call_args.kwargs["build_jobs"], 4)
                    for forbidden in ("--nocheck", "--short-circuit", "--nodebuginfo", "--nodeps"):
                        self.assertNotIn(forbidden, args)
                    if kind == "deb":
                        self.assertEqual(args[2], "dpkg-buildpackage")
                        self.assertIn("--jobs=4", args)
                        self.assertEqual("--rules-target=override_dh_auto_build" in args, stage == "compile")
                        self.assertEqual("--build=full" in args, stage != "compile")
                        self.assertEqual("--no-pre-clean" in args, stage == "package")
                    else:
                        self.assertEqual(args[2:4], ("rpmbuild", "-bc" if stage == "compile" else "-ba"))
                        self.assertEqual("--noprep" in args, stage == "package")
                        self.assertEqual("--noclean" in args, stage == "compile")
                        self.assertEqual(container.call_args.kwargs["environment"],
                                         ["SOURCE_DATE_EPOCH=1234567890"])
            container.reset_mock()
            with self.assertRaises(ValueError):
                build.run_native_build("rpm", Path("root"), Path("source"), 1, 4, "skip-tests")
            container.assert_not_called()


class TransferTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.tree = self.root / "original"
        self.tree.mkdir()
        self.source = self.tree / "peritus-1.2.3"
        self.source.mkdir()
        payload = self.source / "Cargo.toml"
        payload.write_bytes(b"fixture source")
        self.binding = {"format": "deb", "architecture": "x86_64", "container_image": "sha256:fixture",
                        "maintainer": "A <a@example.invalid>", "cargo_build_jobs": 4,
                        "workflow": None,
                        "source": {"git_commit": "a" * 40, "version": "1.2.3", "source_date_epoch": 123,
                                   "source_files_sha256": {"Cargo.toml": hashlib.sha256(payload.read_bytes()).hexdigest()}}}
        (self.source / "PACKAGE-SOURCE.json").write_text(json.dumps(self.binding["source"]))
        target = self.source / "target/release"
        target.mkdir(parents=True)
        for name in compiled.BINARIES:
            binary = target / name
            binary.write_bytes(f"fixture {name}".encode())
            binary.chmod(0o755)
            os.utime(binary, ns=(1234567890123456789, 1234567890123456789))
        self.observation = {"host": "builder-one", "invocation": "compile-one", "cargo_build_jobs": 4,
                            "started_unix_nanos": 10, "finished_unix_nanos": 20}
        self.bundle = self.root / "bundle"
        self.record = compiled.save_bundle(self.tree, self.bundle, self.binding, self.observation)
        self.destination = self.root / "fresh/build"
        self.destination.parent.mkdir()

    def test_transfer_preserves_source_binary_bytes_permissions_and_timestamp_order(self):
        record = compiled.restore_bundle(self.bundle, self.destination, self.binding)
        self.assertEqual(record, self.record)
        compiled.verify_source_trees(self.destination, self.binding)
        for original in self.tree.rglob("*"):
            restored = self.destination / original.relative_to(self.tree)
            if original.is_file():
                self.assertEqual(original.read_bytes(), restored.read_bytes())
                self.assertEqual(original.stat().st_mode & 0o777, restored.stat().st_mode & 0o777)
                self.assertLessEqual(abs(original.stat().st_mtime_ns - restored.stat().st_mtime_ns), 1000)

    def test_changed_candidate_format_architecture_builder_or_run_is_rejected(self):
        for field in self.binding:
            changed = copy.deepcopy(self.binding)
            changed[field] = "different"
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "binding"):
                compiled.restore_bundle(self.bundle, self.destination, changed)
            self.assertFalse(self.destination.exists())

    def test_changed_archive_is_rejected_before_extraction(self):
        archive = self.bundle / compiled.ARCHIVE
        with archive.open("ab") as output:
            output.write(b"tamper")
        with self.assertRaisesRegex(ValueError, "digest"):
            compiled.restore_bundle(self.bundle, self.destination, self.binding)
        self.assertFalse(self.destination.exists())

    def test_cargo_hard_links_are_transferred_as_regular_file_bytes(self):
        original = self.source / "target/release/peritus"
        os.link(original, self.source / "target/release/cargo-hard-link")
        bundle = self.root / "hard-linked-bundle"
        compiled.save_bundle(self.tree, bundle, self.binding, self.observation)
        compiled.restore_bundle(bundle, self.destination, self.binding)
        restored = self.destination / "peritus-1.2.3/target/release/cargo-hard-link"
        self.assertEqual(restored.read_bytes(), original.read_bytes())
        with tarfile.open(bundle / compiled.ARCHIVE) as archive:
            self.assertFalse(any(member.issym() or member.islnk() for member in archive))

    def test_symbolic_links_are_not_dereferenced_by_the_producer(self):
        (self.source / "escape").symlink_to(self.root)
        bundle = self.root / "unsafe-bundle"
        with self.assertRaisesRegex(ValueError, "link or special file"):
            compiled.save_bundle(self.tree, bundle, self.binding, self.observation)
        self.assertFalse(bundle.exists())

    def test_extra_transport_files_and_invalid_observations_are_rejected(self):
        extra = self.bundle / "unexpected"
        extra.touch()
        with self.assertRaisesRegex(ValueError, "inventory"):
            compiled.restore_bundle(self.bundle, self.destination, self.binding)
        extra.unlink()
        for mutation in ({"host": ""}, {"started_unix_nanos": True},
                         {"finished_unix_nanos": 1}, {"cargo_build_jobs": 2}):
            record = copy.deepcopy(self.record)
            record["compile_observation"].update(mutation)
            (self.bundle / compiled.RECORD).write_text(json.dumps(record))
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, "observation"):
                compiled.restore_bundle(self.bundle, self.destination, self.binding)
            self.assertFalse(self.destination.exists())

    def test_a_correct_digest_does_not_authorize_unsafe_archive_paths(self):
        archive = self.bundle / compiled.ARCHIVE
        with tarfile.open(archive, "w:gz") as transport:
            transport.addfile(tarfile.TarInfo("build/../../outside"))
        record = copy.deepcopy(self.record)
        record["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
        record["archive_byte_length"] = archive.stat().st_size
        (self.bundle / compiled.RECORD).write_text(json.dumps(record))
        with self.assertRaisesRegex(ValueError, "unsafe"):
            compiled.restore_bundle(self.bundle, self.destination, self.binding)
        self.assertFalse(self.destination.exists())
        self.assertFalse((self.root / "outside").exists())

    def test_existing_input_or_destination_is_never_overwritten(self):
        with self.assertRaises(FileExistsError):
            compiled.save_bundle(self.tree, self.bundle, self.binding, self.observation)
        self.destination.mkdir()
        retained = self.destination / "retained"
        retained.write_text("keep")
        with self.assertRaises(ValueError):
            compiled.restore_bundle(self.bundle, self.destination, self.binding)
        self.assertEqual(retained.read_text(), "keep")

    def test_modified_source_or_missing_compiled_binary_cannot_continue(self):
        compiled.verify_source_trees(self.tree, self.binding)
        source_file = self.source / "Cargo.toml"
        source_file.write_bytes(b"changed source")
        with self.assertRaisesRegex(ValueError, "source"):
            compiled.verify_source_trees(self.tree, self.binding)
        source_file.write_bytes(b"fixture source")
        (self.source / "target/release/peritusd").unlink()
        with self.assertRaisesRegex(ValueError, "compiled binary"):
            compiled.verify_source_trees(self.tree, self.binding)

    def test_rpm_requires_one_compiled_source_tree_in_addition_to_original_source(self):
        binding = copy.deepcopy(self.binding)
        binding["format"] = "rpm"
        with self.assertRaisesRegex(ValueError, "one compiled RPM source"):
            compiled.verify_source_trees(self.tree, binding)

    def rpm_check_bundle(self):
        binding = copy.deepcopy(self.binding)
        binding["format"] = "rpm"
        native_source = self.tree / "BUILD/peritus-1.2.3-build/peritus-1.2.3"
        shutil.copytree(self.source, native_source)
        previous = compiled.save_bundle(self.tree, self.root / "rpm-release", binding, self.observation)
        checks = native_source / "target/debug/deps/peritus_launcher-0123456789abcdef"
        checks.parent.mkdir(parents=True)
        checks.write_bytes(b"fixture check executable")
        checks.chmod(0o755)
        observation = {**self.observation, "invocation": "checks-two",
                       "started_unix_nanos": 30, "finished_unix_nanos": 40}
        bundle = self.root / "rpm-checks"
        record = compiled.save_bundle(self.tree, bundle, binding, observation, previous)
        return binding, bundle, record

    def test_rpm_check_transfer_retains_both_phase_observations_and_requires_the_checked_stage(self):
        binding, bundle, record = self.rpm_check_bundle()
        self.assertEqual(record["previous_compilation"]["compile_observation"], self.observation)
        self.assertEqual(record["stage"], "checks")
        with self.assertRaisesRegex(ValueError, "stage"):
            compiled.restore_bundle(bundle, self.destination, binding)
        self.assertFalse(self.destination.exists())
        restored = compiled.restore_bundle(bundle, self.destination, binding, "checks")
        self.assertEqual(restored, record)

    def test_check_transfer_rejects_missing_parent_changed_binding_or_reused_observations(self):
        binding, bundle, record = self.rpm_check_bundle()
        for mutation in ("missing-parent", "binding", "stage", "time", "invocation"):
            changed = copy.deepcopy(record)
            if mutation == "missing-parent":
                changed["previous_compilation"] = None
            elif mutation == "binding":
                changed["previous_compilation"]["binding"]["architecture"] = "other"
            elif mutation == "stage":
                changed["previous_compilation"]["stage"] = "checks"
            elif mutation == "time":
                changed["compile_observation"]["started_unix_nanos"] = 15
            else:
                changed["compile_observation"]["invocation"] = self.observation["invocation"]
            (bundle / compiled.RECORD).write_text(json.dumps(changed))
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                compiled.restore_bundle(bundle, self.destination, binding, "checks")
            self.assertFalse(self.destination.exists())

    def test_checked_stage_requires_exactly_one_executable_check_harness(self):
        binding, _, record = self.rpm_check_bundle()
        native = self.tree / "BUILD/peritus-1.2.3-build/peritus-1.2.3/target/debug/deps"
        (native / "peritus_launcher-0123456789abcdef").unlink()
        with self.assertRaisesRegex(ValueError, "check executable"):
            compiled.save_bundle(self.tree, self.root / "missing-checks", binding,
                                 record["compile_observation"], record["previous_compilation"])


class ArchiveAdmissionTests(unittest.TestCase):
    def test_traversal_duplicates_links_and_special_files_are_rejected(self):
        for name in ("../outside", "/build/absolute", "build/../outside", "build//file", "build\\file"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                compiled.validate_members([tarfile.TarInfo(name)])
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.CHRTYPE, tarfile.FIFOTYPE):
            entry = tarfile.TarInfo("build/unsafe")
            entry.type = kind
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                compiled.validate_members([entry])
        with self.assertRaises(ValueError):
            compiled.validate_members([tarfile.TarInfo("build/file"), tarfile.TarInfo("build/file")])
        privileged = tarfile.TarInfo("build/unsafe")
        privileged.mode = 0o4755
        with self.assertRaises(ValueError):
            compiled.validate_members([privileged])

    def test_archive_inventory_and_expanded_bytes_are_bounded(self):
        large = tarfile.TarInfo("build/large")
        large.size = compiled.MAX_EXPANDED_BYTES + 1
        with self.assertRaises(ValueError):
            compiled.validate_members([large])
        with patch.object(compiled, "MAX_MEMBERS", 1), self.assertRaises(ValueError):
            compiled.validate_members([tarfile.TarInfo("build/a"), tarfile.TarInfo("build/b")])
