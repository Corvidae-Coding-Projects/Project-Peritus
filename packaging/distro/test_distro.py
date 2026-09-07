"""Focused deterministic package-tooling regressions; native builds are tested separately."""

import hashlib
import json
import os
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch

import common
import build
import ci_sign
import sign
import verify
from licenses import debian_copyright
from sign_inside import refresh_checksums
from source import archive_source, notices
from verify_inside import rpm_signature_verified, tamper, terminal_failure


class ConfigurationTests(unittest.TestCase):
    def test_rpm_metadata_uses_explicit_reproducibility_controls(self):
        arguments = build.rpm_reproducibility_arguments()
        self.assertEqual(arguments[::2], ["--define"] * 3)
        self.assertEqual(arguments[1::2], [
            "_buildhost peritus-reproducible",
            "use_source_date_epoch_as_buildtime 1",
            "build_mtime_policy clamp_to_source_date_epoch",
        ])

    def test_maintainer_requires_one_safe_identity(self):
        for value in ("", "A", "A <a@example.org>\nB", "A%{expand:x} <a@example.org>", "A <a b@c>"):
            with self.subTest(value=value), patch.dict(os.environ, PERITUS_PACKAGE_MAINTAINER=value):
                with self.assertRaises(ValueError):
                    common.maintainer()
        with patch.dict(os.environ, PERITUS_PACKAGE_MAINTAINER="A <a@example.org>"):
            self.assertEqual(common.maintainer(), "A <a@example.org>")

    def test_engine_cannot_be_an_arbitrary_command(self):
        for value in ("sh", "docker --privileged", "/tmp/docker"):
            with patch.dict(os.environ, PERITUS_CONTAINER_ENGINE=value):
                with self.assertRaises(ValueError):
                    common.engine()

    def test_existing_output_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(common, "ROOT", Path(temporary)):
            output = common.output_directory("deb")
            (output / "retained.deb").write_bytes(b"retained")
            with self.assertRaises(ValueError):
                common.output_directory("deb")
            self.assertEqual((output / "retained.deb").read_bytes(), b"retained")


class ArchiveTests(unittest.TestCase):
    def test_debian_common_license_conversion_preserves_upstream_copyright(self):
        for separator in ("\n", "\n\n", "\n   \n"):
            notice = ("Copyright retained before\n  Apache License" + separator
                      + "  Version 2.0, January 2004\n  TERMS AND CONDITIONS\n"
                      + "  END OF TERMS AND CONDITIONS\nCopyright retained after\n")
            converted = debian_copyright(notice)
            self.assertNotIn("TERMS AND CONDITIONS", converted)
            self.assertIn("/usr/share/common-licenses/Apache-2.0", converted)
            self.assertIn("Copyright retained before", converted)
            self.assertIn("Copyright retained after", converted)

    def test_source_archive_normalizes_time_and_owner(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "peritus-1.2.3"
            source.mkdir()
            (source / "Cargo.toml").write_text("source")
            first, second = root / "a.tar.gz", root / "b.tar.gz"
            archive_source(source, first, 1234567890)
            os.utime(source / "Cargo.toml", (2000000000, 2000000000))
            archive_source(source, second, 1234567890)
            self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_source_archive_rejects_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "peritus-1.2.3"
            source.mkdir()
            (source / "escape").symlink_to("/etc/passwd")
            with self.assertRaises(ValueError):
                archive_source(source, root / "source.tar.gz", 1234567890)

    def test_unrecognized_missing_license_is_a_hard_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            crate = root / "vendor" / "unreviewed-1.0.0"
            crate.mkdir(parents=True)
            license_root = root / "packaging" / "licenses"
            license_root.mkdir(parents=True)
            (license_root / "README.md").write_text("Provenance")
            (crate / "Cargo.toml").write_text('[package]\nname="unreviewed"\nversion="1.0.0"\nlicense="MIT"\n')
            with self.assertRaisesRegex(ValueError, "no redistributable license"):
                notices(root)


class BuildTests(unittest.TestCase):
    def test_rpm_build_passes_exact_source_epoch_and_retains_real_observations(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "output"
            output.mkdir()

            def prepare(build_root):
                source = build_root / "peritus-1.2.3"
                spec = source / "packaging/rpm/peritus.spec"
                spec.parent.mkdir(parents=True)
                spec.write_text("test-only spec")
                (source / "PACKAGE-SOURCE.json").write_text(
                    json.dumps({"source_date_epoch": 1234567890}))
                archive = build_root / "peritus-1.2.3.tar.gz"
                archive.write_bytes(b"test-only source")
                return source, archive, 1234567890

            def compile_fixture(kind, mounts, *arguments, **options):
                self.assertEqual(kind, "rpm")
                self.assertEqual(options, {"environment": ["SOURCE_DATE_EPOCH=1234567890"],
                                           "build_jobs": 2})
                self.assertIn("use_source_date_epoch_as_buildtime 1", arguments)
                self.assertIn("build_mtime_policy clamp_to_source_date_epoch", arguments)
                self.assertIn("_buildhost peritus-reproducible", arguments)
                (mounts[0][0] / "RPMS/peritus.rpm").write_bytes(b"test-only output")

            with patch.object(build, "ROOT", root), \
                    patch.object(build, "package_format", return_value="rpm"), \
                    patch.object(build, "maintainer", return_value="Fixture <fixture@example.invalid>"), \
                    patch.object(build, "version", return_value="1.2.3"), \
                    patch.object(build, "output_directory", return_value=output), \
                    patch.object(build, "prepare", side_effect=prepare), \
                    patch.object(build, "container", side_effect=compile_fixture), \
                    patch.object(build, "run", return_value="sha256:fixture-image"), \
                    patch.object(build.socket, "gethostname", return_value="actual-host"), \
                    patch.object(build.time, "time_ns", side_effect=[2000000000000, 3000000000000]), \
                    patch.dict(os.environ, PERITUS_PACKAGE_BUILD_JOBS="2"):
                build.build()
            record = json.loads((output / "peritus-rpm-build.json").read_text())
            observation = record["build_observation"]
            self.assertEqual(observation["host"], "actual-host")
            self.assertEqual(observation["cargo_build_jobs"], 2)
            self.assertEqual(observation["started_unix_nanos"], 2000000000000)
            self.assertEqual(observation["finished_unix_nanos"], 3000000000000)
            self.assertTrue((root / "target" / observation["invocation"]).is_dir())
            self.assertEqual(record["source_date_epoch"], 1234567890)
            self.assertEqual(record["unsigned_files_sha256"], {
                "peritus.rpm": hashlib.sha256(b"test-only output").hexdigest()})


class SigningTests(unittest.TestCase):
    def test_verifier_binds_the_retained_unsigned_build_to_signed_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            unsigned = root / "dist/packages/deb"
            signed = root / "dist/signed/deb/packages"
            unsigned.mkdir(parents=True)
            signed.mkdir(parents=True)
            package = unsigned / "peritus.deb"
            package.write_bytes(b"unsigned package")
            record = json.dumps({"format": "deb", "architecture": common.architecture(),
                                 "version": common.version(),
                                 "unsigned_files_sha256": {package.name: common.digest(package)}})
            (unsigned / "peritus-deb-build.json").write_text(record)
            signed_record = signed / "peritus-deb-build.json"
            signed_record.write_text(record)
            with patch.object(verify, "ROOT", root), \
                    patch.object(verify, "package_format", return_value="deb"), \
                    patch.object(verify, "fingerprint", return_value="A" * 40), \
                    patch.object(verify, "container") as container, \
                    patch.dict(os.environ, GITHUB_ACTIONS="false"):
                verify.verify()
                container.assert_called_once()
                self.assertIn((unsigned, "/unsigned", True), container.call_args.args[1])
                container.reset_mock()
                signed_record.write_text("{}")
                with self.assertRaisesRegex(ValueError, "does not bind"):
                    verify.verify()
                container.assert_not_called()

    def test_rpm_signature_check_requires_exact_fingerprint_and_success_not_only_digests(self):
        key = "C5FAA0061C56096C0DB57E93BC1152DDCE409792"
        signed = (f"    Header OpenPGP V4 RSA/SHA512 signature, key fingerprint: {key.lower()}: OK\n"
                  "    Header SHA256 digest: OK\n    Payload SHA256 digest: OK\n")
        self.assertTrue(rpm_signature_verified(signed, key))
        self.assertTrue(rpm_signature_verified(signed.upper(), key))
        self.assertFalse(rpm_signature_verified(signed, "0" * 40))
        self.assertFalse(rpm_signature_verified(signed.replace(": OK", ": NOKEY"), key))
        self.assertFalse(rpm_signature_verified("Header SHA256 digest: OK\nPayload SHA256 digest: OK", key))

    def test_ci_import_is_forbidden_locally_and_for_placeholder_or_wrong_tags(self):
        with patch.dict(os.environ, GITHUB_ACTIONS="false"):
            with self.assertRaisesRegex(ValueError, "only available in GitHub Actions"):
                ci_sign.sign_ci()
        for version, tag in (("0.0.0", "v0.0.0"), ("1.2.3", "v9.9.9")):
            with patch.dict(os.environ, GITHUB_ACTIONS="true", GITHUB_REF_NAME=tag), \
                    patch.object(ci_sign, "version", return_value=version), \
                    patch.object(ci_sign.subprocess, "run") as import_key:
                with self.assertRaisesRegex(ValueError, "non-placeholder release tag"):
                    ci_sign.sign_ci()
                import_key.assert_not_called()

    def test_unsigned_inventory_rejects_changed_added_and_wrong_architecture_assets(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "peritus.deb"
            package.write_bytes(b"unsigned package")
            record = {"format": "deb", "architecture": common.architecture(),
                      "version": common.version(),
                      "unsigned_files_sha256": {package.name: common.digest(package)}}
            manifest = root / "peritus-deb-build.json"
            manifest.write_text(json.dumps(record))
            with patch.dict(os.environ, GITHUB_ACTIONS="false"):
                sign.validate_unsigned(root, "deb")
                package.write_bytes(b"changed")
                with self.assertRaisesRegex(ValueError, "changed after build"):
                    sign.validate_unsigned(root, "deb")
                package.write_bytes(b"unsigned package")
                extra = root / "extra.deb"
                extra.touch()
                with self.assertRaisesRegex(ValueError, "exactly cover"):
                    sign.validate_unsigned(root, "deb")
                extra.unlink()
                record["architecture"] = "wrong-architecture"
                manifest.write_text(json.dumps(record))
                with self.assertRaisesRegex(ValueError, "does not match"):
                    sign.validate_unsigned(root, "deb")

    def test_terminal_check_exercises_a_real_tty(self):
        result = terminal_failure(sys.executable, "-c",
                                  "import os,sys; print(os.isatty(0), os.isatty(1)); sys.exit(1)")
        self.assertIn("True True", result)
        with self.assertRaisesRegex(ValueError, "unexpectedly succeeded"):
            terminal_failure(sys.executable, "-c", "pass")

    def test_all_debian_checksums_follow_signed_payload_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "peritus_1.2.3-1_amd64.deb"
            package.write_bytes(b"signed package bytes")
            changes = root / "peritus.changes"
            changes.write_text(
                f"Format: 1.8\nFiles:\n abc 1 devel optional {package.name}\n"
                f"Checksums-Sha1:\n abc 1 {package.name}\n"
                f"Checksums-Sha256:\n abc 1 {package.name}\n"
            )
            refresh_checksums(changes)
            content = changes.read_text()
            for algorithm in ("md5", "sha1", "sha256"):
                self.assertIn(hashlib.new(algorithm, package.read_bytes()).hexdigest(), content)
            self.assertEqual(content.count(str(package.stat().st_size)), 3)
            self.assertIn("devel optional", content)

    def test_checksum_metadata_cannot_escape_package_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            changes = Path(temporary) / "peritus.changes"
            changes.write_text("Checksums-Sha256:\n abc 1 ../outside\n")
            with self.assertRaises(ValueError):
                refresh_checksums(changes)

    def test_debian_negative_test_changes_payload_not_ar_padding(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package, changed = root / "good.deb", root / "bad.deb"
            payload = b"payload-data"
            header = (f"{'data.tar.xz/':<16}{0:<12}{0:<6}{0:<6}{100644:<8}"
                      f"{len(payload):<10}`\n").encode()
            package.write_bytes(b"!<arch>\n" + header + payload)
            tamper(package, changed)
            self.assertEqual(changed.read_bytes()[:68], package.read_bytes()[:68])
            self.assertNotEqual(changed.read_bytes()[68:], payload)


if __name__ == "__main__":
    unittest.main()
