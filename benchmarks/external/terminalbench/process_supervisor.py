"""Cancellation-safe process supervision for the Harbor adapter."""

from __future__ import annotations

import asyncio
import shlex
from pathlib import PurePosixPath

from harbor.environments.base import BaseEnvironment, ExecResult


async def exec_supervised(
    environment: BaseEnvironment,
    command: str,
    *,
    pid_file: PurePosixPath,
    cwd: str,
    env: dict[str, str],
) -> ExecResult:
    """Run a command and reap its in-container process tree when cancelled."""
    try:
        return await environment.exec(
            _supervised_command(command, pid_file),
            cwd=cwd,
            env=env,
        )
    except asyncio.CancelledError:
        cleanup = asyncio.create_task(_terminate_process_tree(environment, pid_file))
        await asyncio.shield(cleanup)
        raise


def _supervised_command(command: str, pid_file: PurePosixPath) -> str:
    quoted_pid_file = shlex.quote(str(pid_file))
    return "\n".join(
        (
            "set -eu",
            f"pid_file={quoted_pid_file}",
            "cleanup() { rm -f -- \"$pid_file\"; }",
            "trap cleanup EXIT",
            f"{command} &",
            "child=$!",
            "printf '%s\\n' \"$child\" > \"$pid_file\"",
            "wait \"$child\"",
        )
    )


async def _terminate_process_tree(
    environment: BaseEnvironment,
    pid_file: PurePosixPath,
) -> None:
    result = await environment.exec(
        _termination_command(pid_file),
        user="root",
        timeout_sec=20,
    )
    if result.return_code != 0:
        detail = (result.stderr or result.stdout or "no diagnostic output").strip()
        raise RuntimeError(
            "terminate cancelled Peritus process tree failed with "
            f"exit {result.return_code}: {detail[:4000]}"
        )


def _termination_command(pid_file: PurePosixPath) -> str:
    quoted_pid_file = shlex.quote(str(pid_file))
    return f"""set -eu
pid_file={quoted_pid_file}
[ -f "$pid_file" ] || exit 0
root=$(cat "$pid_file")
case "$root" in (*[!0-9]*|'') echo 'invalid supervised PID' >&2; exit 1;; esac
collect_tree() {{
  current=$1
  [ -d "/proc/$current" ] || return 0
  children=$(cat "/proc/$current/task/$current/children" 2>/dev/null || true)
  for child in $children; do collect_tree "$child"; done
  printf '%s\\n' "$current"
}}
tree=$(collect_tree "$root")
[ -n "$tree" ] || {{ rm -f -- "$pid_file"; exit 0; }}
kill -TERM $tree 2>/dev/null || true
for delay in 1 2 3; do
  [ ! -d "/proc/$root" ] && break
  sleep 1
done
survivors=''
for process in $tree; do
  [ ! -d "/proc/$process" ] || survivors="$survivors $process"
done
[ -z "$survivors" ] || kill -KILL $survivors 2>/dev/null || true
for delay in 1 2 3 4 5; do
  survivors=''
  for process in $tree; do
    [ ! -d "/proc/$process" ] || survivors="$survivors $process"
  done
  [ -z "$survivors" ] && break
  sleep 1
done
rm -f -- "$pid_file"
[ -z "$survivors" ] || {{ echo "supervised processes survived:$survivors" >&2; exit 1; }}
"""
