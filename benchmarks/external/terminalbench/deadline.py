"""Resolve Harbor's retained agent deadline into a Peritus work horizon."""

from __future__ import annotations

import json
import math
import tomllib
from pathlib import Path
from typing import Any

from harbor.constants import PACKAGE_CACHE_DIR

_MAX_PRODUCT_SECONDS = 8 * 60 * 60


def product_budget_seconds(
    agent_logs_dir: Path,
    package_cache_dir: Path = PACKAGE_CACHE_DIR,
) -> int:
    """Return a bounded work horizon with time left for report and process settlement."""

    lock = _read_json(agent_logs_dir.parent / "lock.json", "Harbor trial lock")
    task = _mapping(lock.get("task"), "Harbor trial task")
    agent = _mapping(lock.get("agent"), "Harbor trial agent")
    task_name = _task_name(task.get("name"))
    digest = _digest(task.get("digest"))
    task_path = package_cache_dir.joinpath(*task_name.split("/"), digest, "task.toml")
    try:
        task_config = tomllib.loads(task_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise RuntimeError(f"cannot read Harbor task deadline from {task_path}") from error
    task_agent = _mapping(task_config.get("agent"), "Harbor task agent")

    base = _optional_positive(agent.get("override_timeout_sec"), "agent override timeout")
    if base is None:
        base = _positive(task_agent.get("timeout_sec"), "task agent timeout")
    maximum = _optional_positive(agent.get("max_timeout_sec"), "agent maximum timeout")
    multiplier_value = lock.get("agent_timeout_multiplier")
    if multiplier_value is None:
        multiplier_value = lock.get("timeout_multiplier", 1.0)
    multiplier = _positive(multiplier_value, "agent timeout multiplier")
    outer_seconds = min(base, maximum if maximum is not None else math.inf) * multiplier

    desired_reserve = min(300.0, max(90.0, outer_seconds * 0.10))
    reserve = min(desired_reserve, outer_seconds / 2.0)
    budget = math.floor(min(float(_MAX_PRODUCT_SECONDS), outer_seconds - reserve))
    if budget < 1:
        raise RuntimeError("Harbor agent deadline leaves no positive Peritus work horizon")
    return budget


def _read_json(path: Path, label: str) -> dict[str, Any]:
    try:
        return _mapping(json.loads(path.read_text(encoding="utf-8")), label)
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"cannot read {label} from {path}") from error


def _mapping(value: object, label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise RuntimeError(f"{label} is not an object")
    return value


def _task_name(value: object) -> str:
    if not isinstance(value, str) or not value or any(
        part in ("", ".", "..") for part in value.split("/")
    ):
        raise RuntimeError("Harbor trial task name is invalid")
    return value


def _digest(value: object) -> str:
    if not isinstance(value, str) or not value.startswith("sha256:"):
        raise RuntimeError("Harbor trial task digest is invalid")
    digest = value.removeprefix("sha256:")
    if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
        raise RuntimeError("Harbor trial task digest is invalid")
    return digest


def _positive(value: object, label: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise RuntimeError(f"{label} is missing or invalid")
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise RuntimeError(f"{label} is missing or invalid")
    return number


def _optional_positive(value: object, label: str) -> float | None:
    return None if value is None else _positive(value, label)
