"""Cancellation-safe process supervision for the Harbor adapter."""

from __future__ import annotations

import asyncio
import shlex
from pathlib import PurePosixPath

from harbor.environments.base import BaseEnvironment, ExecResult

_SUPERVISOR_MARKER = "PERITUS_SUPERVISOR_TOKEN"


async def exec_supervised(
    environment: BaseEnvironment,
    command: str,
    *,
    pid_file: PurePosixPath,
    cwd: str,
    env: dict[str, str],
) -> ExecResult:
    """Run a command and reap its in-container process family when cancelled."""
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
    quoted_marker = shlex.quote(_marker(pid_file))
    return "\n".join(
        (
            "set -eu",
            f"pid_file={quoted_pid_file}",
            f"{_SUPERVISOR_MARKER}={quoted_marker}",
            f"export {_SUPERVISOR_MARKER}",
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
    quoted_marker = shlex.quote(_marker(pid_file))
    return f"""set -eu
pid_file={quoted_pid_file}
[ -f "$pid_file" ] || exit 0
IFS= read -r root < "$pid_file"
case "$root" in (*[!0-9]*|'') echo 'invalid supervised PID' >&2; exit 1;; esac
marker={quoted_marker}
collect_tree() {{
  current=$1
  [ -d "/proc/$current" ] || return 0
  children=$(cat "/proc/$current/task/$current/children" 2>/dev/null || true)
  for child in $children; do collect_tree "$child"; done
  printf '%s\\n' "$current"
}}
collect_marked() {{
  for environment in /proc/[0-9]*/environ; do
    [ -r "$environment" ] || continue
    if tr '\\000' '\\n' < "$environment" 2>/dev/null | \
      grep -Fqx -- "{_SUPERVISOR_MARKER}=$marker"; then
      process=${{environment#/proc/}}
      printf '%s\\n' "${{process%/environ}}"
    fi
  done
}}
collect_targets() {{
  collect_tree "$root"
  collect_marked
}}
for pass in 1 2 3; do
  targets=$(collect_targets)
  [ -n "$targets" ] || break
  kill -STOP $targets 2>/dev/null || true
  sleep 1
done
targets=$(collect_targets)
[ -z "$targets" ] || kill -KILL $targets 2>/dev/null || true
for pass in 1 2 3 4 5; do
  survivors=$(collect_targets)
  [ -z "$survivors" ] && break
  sleep 1
done
rm -f -- "$pid_file"
[ -z "$survivors" ] || {{ echo "supervised processes survived:$survivors" >&2; exit 1; }}
"""


def _marker(pid_file: PurePosixPath) -> str:
    return f"peritus-supervised:{pid_file}"
