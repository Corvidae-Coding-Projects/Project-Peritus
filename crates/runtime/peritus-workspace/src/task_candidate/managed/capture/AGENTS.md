# crates/runtime/peritus-workspace/src/task_candidate/managed/capture — navigation

## Responsibility

Implementation pieces for `capture.rs`: Raw-byte Git trees built with a private index; never runs clean/smudge filters.

## Files in this folder

| File | Role |
| --- | --- |
| [directories.rs](directories.rs) | Inspect ordinary directories still represented by a gitlink in the real index. |

Owning module: [capture.rs](../capture.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
