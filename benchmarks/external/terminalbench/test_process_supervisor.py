"""Regressions for cancellation-safe in-container process supervision."""

from __future__ import annotations

import asyncio
import os
import unittest
from pathlib import Path, PurePosixPath
from tempfile import TemporaryDirectory

from harbor.environments.base import ExecResult

from benchmarks.external.terminalbench.process_supervisor import (
    _supervised_command,
    exec_supervised,
)


class _CancellationEnvironment:
    def __init__(self) -> None:
        self.started = asyncio.Event()
        self.cleaned = asyncio.Event()
        self.calls: list[tuple[str, dict[str, object]]] = []

    async def exec(self, command: str, **kwargs: object) -> ExecResult:
        self.calls.append((command, kwargs))
        if len(self.calls) == 1:
            self.started.set()
            await asyncio.Event().wait()
            raise AssertionError("cancelled execution unexpectedly resumed")
        self.cleaned.set()
        return ExecResult(return_code=0)


class _LocalCancellationEnvironment:
    def __init__(self) -> None:
        self.processes: list[asyncio.subprocess.Process] = []

    async def exec(
        self,
        command: str,
        *,
        cwd: str | None = None,
        env: dict[str, str] | None = None,
        **_options: object,
    ) -> ExecResult:
        process = await asyncio.create_subprocess_shell(
            command,
            cwd=cwd,
            env={**os.environ, **(env or {})},
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        self.processes.append(process)
        stdout, stderr = await process.communicate()
        return ExecResult(
            stdout=stdout.decode(),
            stderr=stderr.decode(),
            return_code=process.returncode,
        )


class ProcessSupervisorTests(unittest.IsolatedAsyncioTestCase):
    def test_wrapper_records_and_waits_for_the_exact_child(self) -> None:
        command = _supervised_command(
            "peritus-benchmark-agent terminalbench --workspace /app",
            PurePosixPath("/tmp/peritus-home/run.pid"),
        )

        self.assertIn("peritus-benchmark-agent terminalbench --workspace /app &", command)
        self.assertIn("child=$!", command)
        self.assertIn("printf '%s\\n' \"$child\" > \"$pid_file\"", command)
        self.assertIn("wait \"$child\"", command)
        self.assertIn("trap cleanup EXIT", command)

    async def test_cancellation_reaps_remote_tree_before_propagating(self) -> None:
        environment = _CancellationEnvironment()
        task = asyncio.create_task(
            exec_supervised(
                environment,  # type: ignore[arg-type]
                "peritus-benchmark-agent terminalbench",
                pid_file=PurePosixPath("/tmp/peritus-home/run.pid"),
                cwd="/app",
                env={"HOME": "/tmp/peritus-home"},
            )
        )
        await environment.started.wait()

        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task

        self.assertTrue(environment.cleaned.is_set())
        self.assertEqual(len(environment.calls), 2)
        cleanup_command, cleanup_options = environment.calls[1]
        self.assertIn("/proc/$current/task/$current/children", cleanup_command)
        self.assertIn("kill -TERM $tree", cleanup_command)
        self.assertIn("kill -KILL $survivors", cleanup_command)
        self.assertEqual(cleanup_options["user"], "root")
        self.assertEqual(cleanup_options["timeout_sec"], 20)

    async def test_cancellation_reaps_a_live_linux_process_tree(self) -> None:
        environment = _LocalCancellationEnvironment()
        with TemporaryDirectory(prefix="peritus-supervisor-") as raw:
            pid_path = Path(raw) / "agent.pid"
            task = asyncio.create_task(
                exec_supervised(
                    environment,  # type: ignore[arg-type]
                    "sh -c 'sleep 60 & wait'",
                    pid_file=PurePosixPath(str(pid_path)),
                    cwd=raw,
                    env={},
                )
            )
            for _ in range(100):
                if pid_path.exists():
                    break
                await asyncio.sleep(0.01)
            self.assertTrue(pid_path.exists())
            child_pid = int(pid_path.read_text(encoding="utf-8").strip())

            task.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await task
            await asyncio.gather(
                *(process.wait() for process in environment.processes)
            )

            self.assertFalse(Path(f"/proc/{child_pid}").exists())
            self.assertFalse(pid_path.exists())


if __name__ == "__main__":
    unittest.main()
