from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
CONTAINER_IMAGE = "docker.io/library/alpine:3.22"


class PosixUninstallFailureTests(unittest.TestCase):
    def setUp(self):
        if shutil.which("podman") is None:
            self.skipTest("rootless podman is required for disposable natural-profile isolation")
        if subprocess.run(
            ["podman", "image", "exists", CONTAINER_IMAGE],
            check=False,
            timeout=10,
        ).returncode != 0:
            self.skipTest(f"preloaded {CONTAINER_IMAGE} is required; tests never pull images")
        self.temporary = tempfile.TemporaryDirectory(prefix="peritus-posix-lifecycle-")
        self.root = Path(self.temporary.name)
        self.home = self.root / "profile"
        self.commands = self.root / "commands"
        self.calls = self.root / "calls"
        self.commands.mkdir()
        self.sibling = self.root / "sibling-canary"
        self.sibling.write_text("unrelated installation\n", encoding="utf-8")

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

    def _run(self, script: Path, *, fault: str) -> subprocess.CompletedProcess[str]:
        relative_script = script.relative_to(ROOT)
        return subprocess.run(
            [
                "podman",
                "run",
                "--rm",
                "--network=none",
                "--read-only",
                "--security-opt=label=disable",
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
                CONTAINER_IMAGE,
                "/bin/sh",
                f"/repo/{relative_script}",
            ],
            text=True,
            capture_output=True,
            timeout=30,
            check=False,
        )

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
            self.skipTest("id is required by the macOS lifecycle script")
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
    unittest.main()
