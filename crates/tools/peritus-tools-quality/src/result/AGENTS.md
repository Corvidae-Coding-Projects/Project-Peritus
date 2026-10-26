# crates/tools/peritus-tools-quality/src/result — navigation

## Responsibility

Implementation pieces for `result.rs`: Strict typed decoding of `quality.run` terminal envelopes.

## Files in this folder

| File | Role |
| --- | --- |
| [contract.rs](contract.rs) | Closed consistency rules for quality terminal status and typed failure metadata. |
| [structured.rs](structured.rs) | Strict decoding of the quality-owned structured terminal projection. |

Owning module: [result.rs](../result.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
