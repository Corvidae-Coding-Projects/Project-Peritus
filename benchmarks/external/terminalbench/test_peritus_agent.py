"""Focused regressions for the thin Harbor-to-Peritus boundary."""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from harbor.environments.base import ExecResult

from benchmarks.external.terminalbench.peritus_agent import (
    _file_sha256,
    _parse_protocol,
    _parse_report,
    _report_identity,
    _runtime_path,
    _workspace_path,
)


class _Environment:
    def __init__(self, result: ExecResult) -> None:
        self.result = result
        self.commands: list[str] = []

    async def exec(self, command: str) -> ExecResult:
        self.commands.append(command)
        return self.result


class WorkspacePathTests(unittest.IsolatedAsyncioTestCase):
    async def test_uses_the_container_workdir_instead_of_assuming_app(self) -> None:
        environment = _Environment(ExecResult(stdout="/workspace\n", return_code=0))

        path = await _workspace_path(environment)  # type: ignore[arg-type]

        self.assertEqual(str(path), "/workspace")
        self.assertEqual(environment.commands, ["pwd -P"])

    async def test_rejects_root_relative_and_multiline_results(self) -> None:
        for value in ("/\n", "workspace\n", "/workspace\n/other\n"):
            with self.subTest(value=value):
                environment = _Environment(ExecResult(stdout=value, return_code=0))
                with self.assertRaisesRegex(RuntimeError, "safe absolute path"):
                    await _workspace_path(environment)  # type: ignore[arg-type]

    async def test_preserves_environment_exec_failure(self) -> None:
        environment = _Environment(
            ExecResult(stderr="container unavailable", return_code=125)
        )

        with self.assertRaisesRegex(RuntimeError, "resolve task working directory failed"):
            await _workspace_path(environment)  # type: ignore[arg-type]


class RuntimePathTests(unittest.IsolatedAsyncioTestCase):
    async def test_prepends_peritus_without_discarding_container_paths(self) -> None:
        environment = _Environment(
            ExecResult(
                stdout="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\n",
                return_code=0,
            )
        )

        path = await _runtime_path(environment)  # type: ignore[arg-type]

        self.assertEqual(
            path,
            "/opt/peritus/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )

    async def test_deduplicates_existing_peritus_directory(self) -> None:
        environment = _Environment(
            ExecResult(stdout="/usr/bin:/opt/peritus/bin:/bin\n", return_code=0)
        )

        path = await _runtime_path(environment)  # type: ignore[arg-type]

        self.assertEqual(path, "/opt/peritus/bin:/usr/bin:/bin")

    async def test_rejects_empty_relative_and_multiline_paths(self) -> None:
        for value in ("\n", "/usr/bin:bin\n", "/usr/bin\n/sbin\n"):
            with self.subTest(value=value):
                environment = _Environment(ExecResult(stdout=value, return_code=0))
                with self.assertRaisesRegex(RuntimeError, "safe absolute path list"):
                    await _runtime_path(environment)  # type: ignore[arg-type]

    async def test_preserves_environment_exec_failure(self) -> None:
        environment = _Environment(
            ExecResult(stderr="container unavailable", return_code=125)
        )

        with self.assertRaisesRegex(RuntimeError, "resolve task executable path failed"):
            await _runtime_path(environment)  # type: ignore[arg-type]


class ReportIdentityTests(unittest.TestCase):
    def test_parses_schema_two_after_progress_output(self) -> None:
        report = _parse_report('progress\n{"schema_version":2,"success":true}\n')

        self.assertTrue(report["success"])

    def test_rejects_legacy_terminal_report(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "unsupported Terminal-Bench report"):
            _parse_report('{"schema_version":1,"success":true}\n')

    def test_hashes_and_matches_the_uploaded_binary_identity(self) -> None:
        with TemporaryDirectory() as raw:
            binary = Path(raw) / "agent"
            binary.write_bytes(b"native-agent")
            digest = _file_sha256(binary)
        report = {
            "agent_identity": {
                "package_version": "0.0.0",
                "source_revision": "0123456789abcdef0123456789abcdef01234567",
                "binary_sha256": digest,
            }
        }

        self.assertEqual(_report_identity(report, digest)["binary_sha256"], digest)

    def test_protocol_binds_adapter_schema_and_executable_identity(self) -> None:
        digest = "a" * 64
        protocol = {
            "schema_version": 1,
            "terminalbench_report_schema_version": 2,
            "agent_identity": {
                "package_version": "0.0.0",
                "source_revision": "0123456789abcdef0123456789abcdef01234567",
                "binary_sha256": digest,
            },
        }

        parsed = _parse_protocol(json.dumps(protocol), digest)

        self.assertEqual(parsed["terminalbench_report_schema_version"], 2)

    def test_protocol_rejects_stale_report_schema_before_a_trial(self) -> None:
        protocol = {
            "schema_version": 1,
            "terminalbench_report_schema_version": 1,
            "agent_identity": {},
        }

        with self.assertRaisesRegex(RuntimeError, "incompatible"):
            _parse_protocol(json.dumps(protocol), "a" * 64)

    def test_rejects_a_report_from_different_executable_bytes(self) -> None:
        report = {
            "agent_identity": {
                "package_version": "0.0.0",
                "source_revision": "0123456789abcdef0123456789abcdef01234567",
                "binary_sha256": "0" * 64,
            }
        }

        with self.assertRaisesRegex(RuntimeError, "does not match the uploaded binary"):
            _report_identity(report, "1" * 64)


if __name__ == "__main__":
    unittest.main()
