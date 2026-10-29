# docs/formal-coverage-evidence/ci-snapshot — navigation

## Responsibility

Historical evidence for ci snapshot; recorded inputs, outputs and source identities belong to the earlier investigation.

## Files in this folder

| File | Role |
| --- | --- |
| [retention.json](retention.json) | Structured retention data with fields `schema`, `schema_version`, `reviewer`, `scope`, `original_hash_manifest`. |
| [review.json](review.json) | Structured review data with fields `schema`, `schema_version`, `reviewed_head`, `reviewed_tree`, `hash_manifest`. |
| [source-sha256.txt](source-sha256.txt) | Source identity digest inventory for the recorded run. |

## Working in this area

Treat this directory as retained evidence tied to the source identity and environment recorded by its owning report. Do not infer current behavior from an old log alone; follow the parent waypoint to the active implementation or replay and preserve original evidence bytes when adding newer observations.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
