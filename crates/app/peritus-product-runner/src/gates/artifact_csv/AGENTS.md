# crates/app/peritus-product-runner/src/gates/artifact_csv — navigation

## Responsibility

Implementation pieces for `artifact_csv.rs`: Contract-selected, incremental structural validation for changed CSV artifacts.

## Files in this folder

| File | Role |
| --- | --- |
| [parser.rs](parser.rs) | CSV decoding/parser state and structural summaries for artifact checks. |

Owning module: [artifact_csv.rs](../artifact_csv.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
