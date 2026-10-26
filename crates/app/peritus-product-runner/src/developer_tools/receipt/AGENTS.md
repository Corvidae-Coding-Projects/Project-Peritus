# crates/app/peritus-product-runner/src/developer_tools/receipt — navigation

## Responsibility

Implementation pieces for `receipt.rs`: Durable receipts around mutating developer-tool effects.

## Files in this folder

| File | Role |
| --- | --- |
| [codec.rs](codec.rs) | Explicit effect-receipt JSON boundary without derive macros. |
| [inspection.rs](inspection.rs) | Read-only uncertain-effect projection and explicit human review. |
| [owner.rs](owner.rs) | Durable effect-receipt command-owner identity and reconciliation. |
| [replay.rs](replay.rs) | Read-only completed-receipt lookup before mutation-specific checkpoint preflight. |
| [storage.rs](storage.rs) | Durable frame loading, truncated-tail recovery, and append-only persistence. |
| [uncertainty.rs](uncertainty.rs) | Unknown command inspection, acknowledgement, and cross-invocation fencing. |

Owning module: [receipt.rs](../receipt.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
