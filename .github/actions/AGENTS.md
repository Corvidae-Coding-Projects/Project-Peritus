# .github/actions — navigation

## Responsibility

Repository-owned reusable action definitions used by hosted workflows.

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [setup-rust/](setup-rust/AGENTS.md) | Composite action installing the pinned Rust toolchain with bounded retries after transient setup failures. |
| [upload-artifact/](upload-artifact/AGENTS.md) | Composite action uploading required release artifacts with bounded retries and unique attempt names. |

## Working in this area

Trace each workflow or composite action from its event trigger through permissions, inputs, called scripts, and produced artifacts. Repository YAML describes intended automation only; runner availability, secrets, branch protection, and current enablement remain live GitHub state.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
