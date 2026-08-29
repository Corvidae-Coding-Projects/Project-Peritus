"""Focused regressions for the thin Harbor-to-Peritus boundary."""

from __future__ import annotations

import unittest

from harbor.environments.base import ExecResult

from benchmarks.external.terminalbench.peritus_agent import (
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


if __name__ == "__main__":
    unittest.main()
