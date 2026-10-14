# crates/app/peritus-product-runner/src/candidate/managed/transaction/validation — navigation

## Responsibility

Implementation pieces for `validation.rs`: Validate recorded transitions before retrying any source, index, HEAD, or archive effect.

## Files in this folder

| File | Role |
| --- | --- |
| [directories.rs](directories.rs) | A retained archive proves the otherwise ambiguous gap between directory moves. |
| [paths.rs](paths.rs) | Source and index fences preserve edits outside the explicitly owned path set. |

Owning module: [validation.rs](../validation.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
