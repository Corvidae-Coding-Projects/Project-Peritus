"""Fail-closed compiler selection and clean-build comparison tests."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import windows_release as release


class WindowsReleaseTests(unittest.TestCase):
    def test_compiler_requires_native_host_exact_version_and_msvc_abi(self):
        valid = "clang version 20.1.8\nTarget: x86_64-pc-windows-msvc\nThread model: posix"
        for system, machine, compiler, version, error in (
            ("Linux", "x86_64", "clang-cl", valid, "native x86-64 Windows"),
            ("Windows", "ARM64", "clang-cl", valid, "native x86-64 Windows"),
            ("Windows", "AMD64", None, valid, "install the pinned"),
            ("Windows", "AMD64", "clang-cl", valid.replace("20.1.8", "21.1.0"), "must be clang-cl"),
            ("Windows", "AMD64", "clang-cl", valid.replace("msvc", "gnu"), "MSVC ABI"),
        ):
            with self.subTest(system=system, machine=machine, compiler=compiler, version=version), \
                    patch.object(release.platform, "system", return_value=system), \
                    patch.object(release.platform, "machine", return_value=machine), \
                    patch.object(release.shutil, "which", return_value=compiler), \
                    patch.object(release.subprocess, "check_output", return_value=version):
                with self.assertRaisesRegex(ValueError, error):
                    release.compiler_environment()
        with patch.object(release.platform, "system", return_value="Windows"), \
                patch.object(release.platform, "machine", return_value="AMD64"), \
                patch.object(release.shutil, "which", return_value="native/clang-cl.exe"), \
                patch.object(release.subprocess, "check_output", return_value=valid), \
                patch.object(release, "digest", return_value="a" * 64):
            environment, record = release.compiler_environment()
        self.assertEqual(environment["CC_x86_64_pc_windows_msvc"], "native/clang-cl.exe")
        self.assertEqual(record["version"], valid)
        self.assertEqual(record["sha256"], "a" * 64)

    def test_binary_build_preserves_locked_release_and_deterministic_linker(self):
        for binary, package in release.BINARIES.items():
            with self.subTest(binary=binary), \
                    patch.object(release, "compiler_environment", return_value=({"CC": "fixture"}, {})), \
                    patch.object(release.subprocess, "run") as run:
                release.build(binary)
            self.assertEqual(run.call_args.args[0], [
                "cargo", "rustc", "--release", "--locked", "--package", package,
                "--bin", binary, "--", "-C", "link-arg=/Brepro"])
            self.assertEqual(run.call_args.kwargs, {"cwd": release.ROOT,
                                                    "env": {"CC": "fixture"}, "check": True})

    def test_comparison_requires_two_distinct_clean_builds_and_exact_object_bytes(self):
        for identical in (True, False):
            with self.subTest(identical=identical), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                destinations = []

                def compile_fixture(arguments, **options):
                    self.assertEqual(arguments[:7], ["cargo", "build", "--release", "--locked",
                                                     "--package", "peritus-journal", "--target-dir"])
                    target = Path(arguments[-1])
                    self.assertFalse(target.exists())
                    destinations.append(target)
                    output = target / "release/build/libsqlite3-sys-fixture/out"
                    output.mkdir(parents=True)
                    content = b"test-only native object"
                    if not identical and len(destinations) == 2:
                        content += b"different"
                    (output / "fixture-sqlite3.o").write_bytes(content)

                with patch.object(release, "ROOT", root), \
                        patch.object(release, "compiler_environment", return_value=({}, {"test": True})), \
                        patch.object(release.subprocess, "run", side_effect=compile_fixture):
                    if identical:
                        release.check_sqlite()
                    else:
                        with self.assertRaisesRegex(ValueError, "compilations differ"):
                            release.check_sqlite()
                self.assertEqual(len(set(destinations)), 2)
                report = json.loads((root / "target/windows-sqlite-rebuild.json").read_text())
                self.assertIs(report["byte_identical"], identical)
                self.assertEqual(len(report["objects"]), 2)

    def test_object_selection_rejects_missing_or_ambiguous_outputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, "exactly one"):
                release.sqlite_object(root)
            output = root / "release/build/libsqlite3-sys-fixture/out"
            output.mkdir(parents=True)
            (output / "first-sqlite3.o").write_bytes(b"test-only object")
            self.assertEqual(release.sqlite_object(root), output / "first-sqlite3.o")
            (output / "second-sqlite3.o").write_bytes(b"test-only object")
            with self.assertRaisesRegex(ValueError, "exactly one"):
                release.sqlite_object(root)


if __name__ == "__main__":
    unittest.main()
