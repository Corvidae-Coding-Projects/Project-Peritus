# crates/tools/peritus-tool-protocol/src/schema — navigation

## Responsibility

Implementation pieces for `schema.rs`: Deterministic bounded JSON values and the supported schema subset.

## Files in this folder

| File | Role |
| --- | --- |
| [canonical.rs](canonical.rs) | Canonical JSON parsing and rendering. |
| [compatibility.rs](compatibility.rs) | Same-major schema compatibility decisions. |
| [duplicates.rs](duplicates.rs) | Recursive duplicate-key detection before map materialization. |
| [validate.rs](validate.rs) | Complete recursive schema and cardinality validation. |

Owning module: [schema.rs](../schema.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
