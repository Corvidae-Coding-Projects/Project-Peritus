from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parent.parent
CONTAINER_IMAGE = os.environ.get(
    "PERITUS_POSIX_LIFECYCLE_IMAGE", "docker.io/library/alpine:3.22"
)
CONTAINER_ENGINE = os.environ.get("PERITUS_CONTAINER_ENGINE", "podman")
REQUIRE_PREREQUISITES = os.environ.get("PERITUS_REQUIRE_POSIX_LIFECYCLE") == "1"


class PosixUninstallFailureTests(unittest.TestCase):
    def setUp(self):
        if CONTAINER_ENGINE not in ("docker", "podman"):
            self.fail("PERITUS_CONTAINER_ENGINE must be docker or podman")
        if shutil.which(CONTAINER_ENGINE) is None:
            self._unavailable(
                f"{CONTAINER_ENGINE} is required for disposable natural-profile isolation"
            )
        image_check = (
            [CONTAINER_ENGINE, "image", "exists", CONTAINER_IMAGE]
            if CONTAINER_ENGINE == "podman"
            else [CONTAINER_ENGINE, "image", "inspect", CONTAINER_IMAGE]
        )
        if subprocess.run(
            image_check,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=10,
        ).returncode != 0:
            self._unavailable(f"preloaded {CONTAINER_IMAGE} is required; tests never pull images")
        self.temporary = tempfile.TemporaryDirectory(prefix="peritus-posix-lifecycle-")
        self.root = Path(self.temporary.name)
        self.home = self.root / "profile"
        self.commands = self.root / "commands"
        self.calls = self.root / "calls"
        self.commands.mkdir()
        self.sibling = self.root / "sibling-canary"
        self.sibling.write_text("unrelated installation\n", encoding="utf-8")
        self.container_sequence = 0
        self.last_container_name = None

    def _unavailable(self, reason: str) -> None:
        if REQUIRE_PREREQUISITES:
            self.fail(reason)
        self.skipTest(reason)

    def tearDown(self):
        self.assertEqual(
            self.sibling.read_text(encoding="utf-8"),
            "unrelated installation\n",
            "uninstall crossed its owned profile boundary",
        )
        self.temporary.cleanup()

    def _command(self, name: str, body: str) -> None:
        path = self.commands / name
        path.write_text("#!/bin/sh\nset -eu\n" + body, encoding="utf-8")
        path.chmod(0o755)

    def _run(
        self, script: Path, *, fault: str, timeout: float = 30
    ) -> subprocess.CompletedProcess[str]:
        relative_script = script.relative_to(ROOT)
        self.container_sequence += 1
        container_name = f"{self.root.name}-{self.container_sequence}"
        self.last_container_name = container_name
        options = [
            f"--name={container_name}",
            "--network=none",
            "--read-only",
            "--pids-limit=32",
            "--memory=256m",
            "--tmpfs=/tmp:rw,size=32m",
            f"--volume={ROOT}:/repo:ro",
            f"--volume={self.home}:/root:rw",
            f"--volume={self.commands}:/commands:ro",
            f"--volume={self.root}:/campaign:rw",
            "--env=PATH=/commands:/usr/bin:/bin",
            "--env=PERITUS_TEST_CALLS=/campaign/calls",
            f"--env=PERITUS_TEST_FAULT={fault}",
        ]
        if CONTAINER_ENGINE == "podman":
            options.insert(1, "--security-opt=label=disable")
        command = [
            CONTAINER_ENGINE,
            "run",
            *options,
            CONTAINER_IMAGE,
            "/bin/sh",
            f"/repo/{relative_script}",
        ]
        completed = None
        timed_out = False
        try:
            completed = subprocess.run(
                command,
                text=True,
                capture_output=True,
                timeout=timeout,
                check=False,
            )
            return completed
        except subprocess.TimeoutExpired:
            timed_out = True
            raise
        finally:
            cleanup_command = [CONTAINER_ENGINE, "rm", "--force"]
            if CONTAINER_ENGINE == "podman":
                cleanup_command.extend(["--time", "0"])
            cleanup_command.append(container_name)
            cleanup = subprocess.run(
                cleanup_command,
                text=True,
                capture_output=True,
                timeout=10,
                check=False,
            )
            if cleanup.returncode != 0 and (
                timed_out or completed is None or completed.returncode != 125
            ):
                self.fail(
                    f"failed to remove owned container {container_name}: {cleanup.stderr}"
                )
            if cleanup.returncode == 0 and not self._wait_for_container_absent(container_name):
                self.fail(f"owned container {container_name} remained after forced cleanup")

    def _container_exists(self, name: str) -> bool:
        return (
            subprocess.run(
                [CONTAINER_ENGINE, "container", "inspect", name],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=10,
                check=False,
            ).returncode
            == 0
        )

    def _wait_for_container_absent(self, name: str) -> bool:
        deadline = time.monotonic() + 5
        while self._container_exists(name):
            if time.monotonic() >= deadline:
                return False
            time.sleep(0.05)
        return True

    def _linux_fixture(self) -> tuple[Path, Path]:
        unit = self.home / ".config/systemd/user/peritus.service"
        binary = self.home / ".local/bin/peritus"
        unit.parent.mkdir(parents=True, exist_ok=True)
        binary.parent.mkdir(parents=True, exist_ok=True)
        unit.write_text("fixture unit\n", encoding="utf-8")
        binary.write_text("fixture binary\n", encoding="utf-8")
        self._command(
            "systemctl",
            'printf "%s\\n" "$*" >> "$PERITUS_TEST_CALLS"\n'
            '[ "$PERITUS_TEST_FAULT" != query ] || exit 71\n'
            'if [ "$1 $2" = "--user show" ]; then printf "%s\\n" loaded; fi\n',
        )
        return unit, binary

    def test_linux_controller_failure_is_truthful_and_retryable(self):
        script = ROOT / "packaging/linux/Uninstall-Peritus.sh"
        for attempt in range(3):
            with self.subTest(reproduction=attempt + 1):
                unit, binary = self._linux_fixture()
                failed = self._run(script, fault="query")
                self.assertNotEqual(failed.returncode, 0)
                self.assertTrue(unit.exists(), "failed stop removed supervisor registration")
                self.assertTrue(binary.exists(), "failed stop removed package files")

                recovered = self._run(script, fault="none")
                self.assertEqual(recovered.returncode, 0, recovered.stderr)
                self.assertFalse(unit.exists())
                self.assertFalse(binary.exists())

        calls = self.calls.read_text(encoding="utf-8").splitlines()
        self.assertEqual(calls.count("--user show peritus.service --property=LoadState --value"), 6)
        self.assertEqual(calls.count("--user disable --now peritus.service"), 3)
        self.assertEqual(calls.count("--user daemon-reload"), 3)

    def test_timed_out_scenario_force_removes_its_exact_container(self):
        script = ROOT / "packaging/linux/Uninstall-Peritus.sh"
        self._linux_fixture()
        marker = self.root / "hang-reached"
        self._command(
            "systemctl",
            "touch /campaign/hang-reached\nexec sleep 300\n",
        )

        with self.assertRaises(subprocess.TimeoutExpired):
            self._run(script, fault="none", timeout=5)

        self.assertTrue(marker.exists(), "controlled hang did not reach the fault boundary")
        self.assertIsNotNone(self.last_container_name)
        self.assertFalse(
            self._container_exists(self.last_container_name),
            "timed-out scenario container survived exact-name cleanup",
        )

    def test_linux_absent_registration_is_idempotent(self):
        binary = self.home / ".local/bin/peritus"
        binary.parent.mkdir(parents=True)
        binary.write_text("fixture binary\n", encoding="utf-8")
        self._command(
            "systemctl",
            'if [ "$1 $2" = "--user show" ]; then printf "%s\\n" not-found; exit 0; fi\n'
            'exit 72\n',
        )

        result = self._run(ROOT / "packaging/linux/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(binary.exists())

    def test_linux_missing_controller_without_registration_removes_package(self):
        binary = self.home / ".local/bin/peritus"
        binary.parent.mkdir(parents=True)
        binary.write_text("fixture binary\n", encoding="utf-8")

        result = self._run(ROOT / "packaging/linux/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(binary.exists())

    def test_linux_missing_controller_preserves_registration_and_package(self):
        unit = self.home / ".config/systemd/user/peritus.service"
        binary = self.home / ".local/bin/peritus"
        unit.parent.mkdir(parents=True)
        binary.parent.mkdir(parents=True)
        unit.write_text("fixture unit\n", encoding="utf-8")
        binary.write_text("fixture binary\n", encoding="utf-8")

        result = self._run(ROOT / "packaging/linux/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 127)
        self.assertTrue(unit.exists())
        self.assertTrue(binary.exists())

    def test_linux_loaded_registration_without_unit_file_is_reconciled(self):
        binary = self.home / ".local/bin/peritus"
        binary.parent.mkdir(parents=True)
        binary.write_text("fixture binary\n", encoding="utf-8")
        self._command(
            "systemctl",
            'printf "%s\\n" "$*" >> "$PERITUS_TEST_CALLS"\n'
            'if [ "$1 $2" = "--user show" ]; then printf "%s\\n" loaded; fi\n',
        )

        result = self._run(ROOT / "packaging/linux/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(binary.exists())
        self.assertEqual(
            self.calls.read_text(encoding="utf-8").splitlines(),
            [
                "--user show peritus.service --property=LoadState --value",
                "--user disable --now peritus.service",
                "--user daemon-reload",
            ],
        )

    def test_linux_reload_failure_retries_controller_reconciliation(self):
        unit, binary = self._linux_fixture()
        marker = "/campaign/reload-failed"
        self._command(
            "systemctl",
            'printf "%s\\n" "$*" >> "$PERITUS_TEST_CALLS"\n'
            'if [ "$1 $2" = "--user show" ]; then printf "%s\\n" loaded; exit 0; fi\n'
            f'if [ "$2" = daemon-reload ] && [ ! -f "{marker}" ]; then '
            f'touch "{marker}"; exit 74; fi\n',
        )
        failed = self._run(ROOT / "packaging/linux/Uninstall-Peritus.sh", fault="none")
        self.assertNotEqual(failed.returncode, 0)
        self.assertFalse(unit.exists(), "reload failure occurs after unit removal")
        self.assertTrue(binary.exists(), "reload failure must stop package deletion")

        recovered = self._run(ROOT / "packaging/linux/Uninstall-Peritus.sh", fault="none")
        self.assertEqual(recovered.returncode, 0, recovered.stderr)
        self.assertFalse(binary.exists())

    def test_macos_controller_access_failure_is_truthful_and_retryable(self):
        if shutil.which("id") is None:
            self._unavailable("id is required by the macOS lifecycle script")
        script = ROOT / "packaging/macos/Uninstall-Peritus.sh"
        agent = self.home / "Library/LaunchAgents/com.corvidae.peritus.plist"
        binary = self.home / "Library/Application Support/Peritus/bin/peritus"
        self._command(
            "launchctl",
            'printf "%s\\n" "$*" >> "$PERITUS_TEST_CALLS"\n'
            'if [ "$1" = print ] && [ "$2" != "gui/$(id -u)" ]; then exit 113; fi\n'
            '[ "$PERITUS_TEST_FAULT" != query ] || exit 73\n',
        )
        for attempt in range(3):
            with self.subTest(reproduction=attempt + 1):
                agent.parent.mkdir(parents=True, exist_ok=True)
                binary.parent.mkdir(parents=True, exist_ok=True)
                agent.write_text("fixture agent\n", encoding="utf-8")
                binary.write_text("fixture binary\n", encoding="utf-8")
                failed = self._run(script, fault="query")
                self.assertNotEqual(failed.returncode, 0)
                self.assertTrue(agent.exists())
                self.assertTrue(binary.exists())

                recovered = self._run(script, fault="none")
                self.assertEqual(recovered.returncode, 0, recovered.stderr)
                self.assertFalse(agent.exists())
                self.assertFalse(binary.exists())

    def test_macos_loaded_job_without_agent_file_is_reconciled(self):
        binary = self.home / "Library/Application Support/Peritus/bin/peritus"
        binary.parent.mkdir(parents=True)
        binary.write_text("fixture binary\n", encoding="utf-8")
        self._command(
            "launchctl",
            'printf "%s\\n" "$*" >> "$PERITUS_TEST_CALLS"\n'
            'exit 0\n',
        )

        result = self._run(ROOT / "packaging/macos/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(binary.exists())
        calls = self.calls.read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(calls), 3)
        self.assertTrue(calls[2].startswith("bootout gui/"))

    def test_macos_missing_controller_without_registration_removes_package(self):
        binary = self.home / "Library/Application Support/Peritus/bin/peritus"
        binary.parent.mkdir(parents=True)
        binary.write_text("fixture binary\n", encoding="utf-8")

        result = self._run(ROOT / "packaging/macos/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(binary.exists())

    def test_macos_missing_controller_preserves_registration_and_package(self):
        agent = self.home / "Library/LaunchAgents/com.corvidae.peritus.plist"
        binary = self.home / "Library/Application Support/Peritus/bin/peritus"
        agent.parent.mkdir(parents=True)
        binary.parent.mkdir(parents=True)
        agent.write_text("fixture agent\n", encoding="utf-8")
        binary.write_text("fixture binary\n", encoding="utf-8")

        result = self._run(ROOT / "packaging/macos/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 127)
        self.assertTrue(agent.exists())
        self.assertTrue(binary.exists())

    def test_macos_generic_job_query_failure_is_not_absence(self):
        binary = self.home / "Library/Application Support/Peritus/bin/peritus"
        binary.parent.mkdir(parents=True)
        binary.write_text("fixture binary\n", encoding="utf-8")
        self._command(
            "launchctl",
            'if [ "$1" = print ] && [ "$2" != "gui/$(id -u)" ]; then exit 75; fi\n'
            'exit 0\n',
        )

        result = self._run(ROOT / "packaging/macos/Uninstall-Peritus.sh", fault="none")

        self.assertEqual(result.returncode, 75)
        self.assertTrue(binary.exists())


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(PosixUninstallFailureTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    executed = result.testsRun - len(result.skipped)
    strict_failure = REQUIRE_PREREQUISITES and (executed == 0 or bool(result.skipped))
    sys.exit(0 if result.wasSuccessful() and not strict_failure else 1)
