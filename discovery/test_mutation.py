import os
from unittest import TestCase, main, mock

from mutation.run import (
    DEFAULT_FILES,
    compiler_env,
    configure_test_packages,
    matches_file,
    selected_files,
    selected_mutation_packages,
    selected_test_packages,
    staged_lock_command,
)


class MutationControllerTests(TestCase):
    def test_explicit_files_replace_production_defaults(self):
        requested = ["crates/model/peritus-model-protocol/src/schema.rs"]
        self.assertEqual(selected_files(requested), tuple(requested))
        self.assertEqual(selected_files([]), DEFAULT_FILES)

    def test_file_selection_uses_full_paths_or_basenames(self):
        path = "crates/model/peritus-model-protocol/src/schema.rs"
        self.assertTrue(matches_file(path, [path]))
        self.assertTrue(matches_file(path, ["schema.rs"]))
        self.assertTrue(matches_file(path, ["crates/**/*.rs"]))
        self.assertFalse(matches_file(path, ["crates/**/bounds.rs"]))

    def test_recursive_glob_matches_zero_or_more_directories(self):
        pattern = ["xtask/src/**/*.rs"]

        self.assertTrue(matches_file("xtask/src/cli.rs", pattern))
        self.assertTrue(matches_file("xtask/src/package/linux.rs", pattern))
        self.assertFalse(matches_file("xtask/tests/cli.rs", pattern))
        self.assertFalse(matches_file("xtask/src/fixture.toml", pattern))

    def test_mutated_packages_are_included_once_ahead_of_discovery_tests(self):
        packages = selected_test_packages(["peritus-model-protocol", "peritus-mutation-tests"])

        self.assertEqual(packages[0], "peritus-model-protocol")
        self.assertEqual(packages.count("peritus-mutation-tests"), 1)
        self.assertIn("peritus-stage8-policy-properties", packages)

    def test_workspace_run_uses_every_package_with_selected_mutants(self):
        counts = {"z-package": 1, "a-package": 2}

        self.assertEqual(
            selected_mutation_packages([], counts),
            ("a-package", "z-package"),
        )
        self.assertEqual(
            selected_mutation_packages(["explicit-package"], counts),
            ("explicit-package",),
        )

    def test_disposable_config_replaces_exactly_one_test_package_setting(self):
        configured = configure_test_packages(
            'test_package = ["old"]\ncopy_vcs = true\n',
            ("peritus-model-protocol", "peritus-mutation-tests"),
        )

        self.assertEqual(
            configured,
            'test_package = ["peritus-model-protocol", "peritus-mutation-tests"]\ncopy_vcs = true\n',
        )
        with self.assertRaises(RuntimeError):
            configure_test_packages("copy_vcs = true\n", ("peritus-model-protocol",))

    def test_compiler_environment_disables_ambient_caches(self):
        inherited = {
            "RUSTC_WRAPPER": "sccache",
            "RUSTC_WORKSPACE_WRAPPER": "wrapper",
            "CARGO_TARGET_DIR": "/ambient/target",
            "CCACHE_DISABLE": "0",
            "SCCACHE_DISABLE": "0",
        }
        with mock.patch.dict(os.environ, inherited, clear=True):
            environment = compiler_env()

        self.assertNotIn("RUSTC_WRAPPER", environment)
        self.assertNotIn("RUSTC_WORKSPACE_WRAPPER", environment)
        self.assertNotIn("CARGO_TARGET_DIR", environment)
        self.assertEqual(environment["CCACHE_DISABLE"], "1")
        self.assertEqual(environment["SCCACHE_DISABLE"], "1")

    def test_staged_lock_resolution_supports_clean_cargo_caches(self):
        command = staged_lock_command()

        self.assertEqual(command, ["cargo", "metadata", "--format-version=1"])
        self.assertNotIn("--offline", command)


if __name__ == "__main__":
    main()
