"""Bounded build scheduling changes must preserve native package recipe execution."""

import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import build
import common


class CapacityTests(unittest.TestCase):
    def test_default_and_explicit_package_build_jobs_are_bounded(self):
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(common.package_build_jobs(), 2)
        for value in ("1", "2", "3", "4"):
            with patch.dict(os.environ, PERITUS_PACKAGE_BUILD_JOBS=value):
                self.assertEqual(common.package_build_jobs(), int(value))
        for value in ("", "0", "5", "-1", "04", "4 ", "4; exit 0", "1.0"):
            with self.subTest(value=value), \
                    patch.dict(os.environ, PERITUS_PACKAGE_BUILD_JOBS=value):
                with self.assertRaisesRegex(ValueError, "PERITUS_PACKAGE_BUILD_JOBS"):
                    common.package_build_jobs()

    def test_container_forwards_only_the_explicit_bounded_cargo_job_count(self):
        with patch.object(common, "run") as run, \
                patch.dict(os.environ, PERITUS_CONTAINER_ENGINE="docker",
                           CARGO_BUILD_JOBS="1000", PERITUS_PACKAGE_BUILD_JOBS="4"):
            for jobs in (1, 2, 3, 4):
                common.container("deb", [], "native-recipe", build_jobs=jobs)
                arguments = run.call_args.args
                self.assertIn(f"CARGO_BUILD_JOBS={jobs}", arguments)
                self.assertEqual(sum(str(arg).startswith("CARGO_BUILD_JOBS=")
                                     for arg in arguments), 1)
                self.assertEqual(arguments[arguments.index("--network") + 1], "none")
                self.assertEqual(arguments[-1], "native-recipe")
            common.container("deb", [], "signing-recipe")
            self.assertIn("CARGO_BUILD_JOBS=2", run.call_args.args)
            for invalid in (0, 5, True, "4"):
                run.reset_mock()
                with self.assertRaises(ValueError):
                    common.container("deb", [], "native-recipe", build_jobs=invalid)
                run.assert_not_called()

    def test_both_recipes_and_observations_use_the_selected_capacity(self):
        for kind in ("deb", "rpm"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "output"
                output.mkdir()

                def prepare(build_root):
                    source = build_root / "peritus-1.2.3"
                    spec = source / "packaging/rpm/peritus.spec"
                    spec.parent.mkdir(parents=True)
                    spec.write_text("fixture only")
                    (source / "PACKAGE-SOURCE.json").write_text("{}")
                    archive = build_root / "peritus-1.2.3.tar.gz"
                    archive.write_bytes(b"fixture only")
                    return source, archive, 1234567890

                def compile_fixture(actual_kind, mounts, *arguments, **options):
                    self.assertEqual(actual_kind, kind)
                    self.assertEqual(options["build_jobs"], 4)
                    self.assertNotIn("--nocheck", arguments)
                    if kind == "deb":
                        self.assertEqual(arguments[:4],
                                         ("dpkg-buildpackage", "--build=full", "--no-sign", "--jobs=4"))
                        destination = mounts[0][0] / "peritus.deb"
                    else:
                        self.assertEqual(arguments[:2], ("rpmbuild", "-ba"))
                        destination = mounts[0][0] / "RPMS/peritus.rpm"
                    destination.write_bytes(b"fixture output")

                with patch.object(build, "ROOT", root), \
                        patch.object(build, "package_format", return_value=kind), \
                        patch.object(build, "maintainer", return_value="A <a@example.invalid>"), \
                        patch.object(build, "version", return_value="1.2.3"), \
                        patch.object(build, "output_directory", return_value=output), \
                        patch.object(build, "prepare", side_effect=prepare), \
                        patch.object(build, "debian_metadata"), \
                        patch.object(build, "container", side_effect=compile_fixture), \
                        patch.object(build, "run", return_value="sha256:fixture"), \
                        patch.dict(os.environ, PERITUS_PACKAGE_BUILD_JOBS="4"):
                    build.build()
                record = json.loads((output / f"peritus-{kind}-build.json").read_text())
                self.assertEqual(record["build_observation"]["cargo_build_jobs"], 4)

    def test_invalid_capacity_is_rejected_before_creating_output(self):
        with patch.dict(os.environ, PERITUS_PACKAGE_BUILD_JOBS="0"), \
                patch.object(build, "package_format", return_value="deb"), \
                patch.object(build, "maintainer", return_value="A <a@example.invalid>"), \
                patch.object(build, "output_directory", side_effect=AssertionError("created output")) as output:
            with self.assertRaisesRegex(ValueError, "PERITUS_PACKAGE_BUILD_JOBS"):
                build.build()
            output.assert_not_called()
