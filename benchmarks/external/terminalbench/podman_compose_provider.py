#!/usr/bin/env python3
"""Make podman-compose satisfy Harbor's Compose-V2 preflight probe."""

from __future__ import annotations

import os
import shutil
import sys


def main() -> None:
    if sys.argv[1:] == ["ls"]:
        return
    provider = shutil.which("podman-compose")
    if provider is None:
        raise SystemExit("podman-compose is not available on PATH")
    arguments, project_directory = _podman_compose_arguments(sys.argv[1:])
    if project_directory is not None:
        os.chdir(project_directory)
    os.execv(provider, [provider, *arguments])


def _podman_compose_arguments(arguments: list[str]) -> tuple[list[str], str | None]:
    """Translate the Compose-V2 global options emitted by Harbor."""
    translated: list[str] = []
    project_directory: str | None = None
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        if argument in ("--project-name", "--project-directory"):
            if index + 1 >= len(arguments):
                raise SystemExit(f"missing value for {argument}")
            value = arguments[index + 1]
            if argument == "--project-name":
                translated.extend(("-p", value))
            else:
                project_directory = value
            index += 2
            continue
        translated.append(argument)
        index += 1
    return translated, project_directory


if __name__ == "__main__":
    main()
