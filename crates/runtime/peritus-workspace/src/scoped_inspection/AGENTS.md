# crates/runtime/peritus-workspace/src/scoped_inspection — navigation

## Responsibility

Implementation pieces for `scoped_inspection.rs`: Handle-relative, bounded reads for explicit live-folder references, not mutation authority.

## Files in this folder

| File | Role |
| --- | --- |
| [read.rs](read.rs) | One streamed source scan with exact inclusion capture and observed-change rejection. |
| [selection.rs](selection.rs) | Explicit exact selections; line ranges resolve to byte ranges during one source scan. |

Owning module: [scoped_inspection.rs](../scoped_inspection.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
