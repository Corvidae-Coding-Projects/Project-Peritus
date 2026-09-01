"""Regressions for Harbor deadline propagation into the native product run."""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from benchmarks.external.terminalbench.deadline import product_budget_seconds

_DIGEST = "a" * 64


class DeadlineTests(unittest.TestCase):
    def test_reserves_ten_percent_of_an_ordinary_task_deadline(self) -> None:
        with TemporaryDirectory() as raw:
            logs, cache = _fixture(Path(raw), task_timeout=900)
            self.assertEqual(product_budget_seconds(logs, cache), 810)

    def test_matches_harbor_override_cap_and_multiplier_order(self) -> None:
        with TemporaryDirectory() as raw:
            logs, cache = _fixture(
                Path(raw),
                task_timeout=900,
                agent={"override_timeout_sec": 1200, "max_timeout_sec": 1000},
                lock={"timeout_multiplier": 3, "agent_timeout_multiplier": 2},
            )
            self.assertEqual(product_budget_seconds(logs, cache), 1800)

    def test_caps_product_work_at_the_eight_hour_hard_horizon(self) -> None:
        with TemporaryDirectory() as raw:
            logs, cache = _fixture(Path(raw), task_timeout=60 * 60 * 12)
            self.assertEqual(product_budget_seconds(logs, cache), 8 * 60 * 60)

    def test_rejects_missing_retained_deadline_evidence(self) -> None:
        with TemporaryDirectory() as raw:
            logs = Path(raw) / "trial" / "agent"
            logs.mkdir(parents=True)
            with self.assertRaisesRegex(RuntimeError, "trial lock"):
                product_budget_seconds(logs, Path(raw) / "cache")


def _fixture(
    root: Path,
    *,
    task_timeout: float,
    agent: dict[str, object] | None = None,
    lock: dict[str, object] | None = None,
) -> tuple[Path, Path]:
    logs = root / "trial" / "agent"
    logs.mkdir(parents=True)
    lock_value: dict[str, object] = {
        "task": {
            "name": "terminal-bench/example",
            "digest": f"sha256:{_DIGEST}",
        },
        "agent": agent or {},
        "timeout_multiplier": 1.0,
    }
    lock_value.update(lock or {})
    (logs.parent / "lock.json").write_text(json.dumps(lock_value), encoding="utf-8")
    cache = root / "cache"
    task_dir = cache / "terminal-bench" / "example" / _DIGEST
    task_dir.mkdir(parents=True)
    (task_dir / "task.toml").write_text(
        f"[agent]\ntimeout_sec = {task_timeout}\n",
        encoding="utf-8",
    )
    return logs, cache


if __name__ == "__main__":
    unittest.main()
