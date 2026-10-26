# .chainlink — navigation

## Responsibility

Retained local issue/workflow configuration, rule documents, and tracking data.

## Files in this folder

| File | Role |
| --- | --- |
| [.gitignore](.gitignore) | Checked-in .gitignore asset/configuration. |
| [hook-config.json](hook-config.json) | Structured hook config data with fields `tracking_mode`, `blocked_git_commands`, `allowed_bash_prefixes`. |
| [issues.db](issues.db) | Retained database for issues; not a source-code entrypoint. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [rules/](rules/AGENTS.md) | Language/framework/workflow rule documents consumed by local hooks. Check hook configuration to understand how they are loaded. |

## Working in this area

Use the responsibility and tables above to choose the local entry point, then follow the parent waypoint to understand callers, ownership, and neighboring systems. Inspect the referenced file and current call sites before changing behavior, and update this map whenever files or responsibilities move.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
