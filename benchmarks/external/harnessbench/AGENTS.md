# benchmarks/external/harnessbench — navigation

## Responsibility

HarnessBench API/runner adapters and example configuration for invoking native Peritus against upstream tasks.

## Files in this folder

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [app.example.json](app.example.json) | Structured app.example data with fields `data_dir`, `tasks_dir`, `results_dir`, `work_root`, `default_timeout_sec`. |
| [harness.json](harness.json) | Structured harness data with fields `models`. |
| [oracle-requirements.txt](oracle-requirements.txt) | Checked-in data/record for oracle requirements. |
| [pin.toml](pin.toml) | Configuration/contract data for pin. Sections: `schema_version`, `repository`, `revision`, `revision_date`, `task_count`. |
| [rubric_server.py](rubric_server.py) | Local HTTP boundary from HarnessBench rubrics to the native Peritus account router. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
