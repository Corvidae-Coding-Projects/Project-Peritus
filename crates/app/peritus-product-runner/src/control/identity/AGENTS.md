# crates/app/peritus-product-runner/src/control/identity — navigation

## Responsibility

Implementation pieces for `identity.rs`: Nominal nonzero control identities, validated during construction and deserialization.

## Files in this folder

| File | Role |
| --- | --- |
| [checkpoint.rs](checkpoint.rs) | Stable recoverable checkpoint identity, distinct from candidate qualification evidence. |
| [conversation.rs](conversation.rs) | Stable user conversation identity, distinct from a run or transport session. |
| [input.rs](input.rs) | Stable accepted input identity across superseding content revisions. |
| [invocation.rs](invocation.rs) | Exact request invocation identity that owns an immutable input binding. |
| [operation.rs](operation.rs) | Reconnect-stable idempotency identity for one exact control operation. |
| [restore.rs](restore.rs) | Stable identity for one preview-bound rewind operation and its recovery receipt. |

Owning module: [identity.rs](../identity.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
