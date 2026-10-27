# crates/app/peritus-product-runner/src/local_context/assembly — navigation

## Responsibility

Model-view construction: keep current host policy and governing input, select working state/evidence, restore current exchanges, and fit the provider transport.

## Files in this folder

| File | Role |
| --- | --- |
| [continuation.rs](continuation.rs) | Role-owned recent prose and cached settled-action receipts; preserves decisions and next steps across compaction and invocation rollover. |
| [evidence.rs](evidence.rs) | Current-permission and content-validated reusable evidence; exact source pages remain local and retrievable. |
| [exchanges.rs](exchanges.rs) | Append-only exchange topology and estimates; preserve failed calls and their current plan, while successful reads require current source admission and stale-read receipts. |
| [retrieval.rs](retrieval.rs) | Final submitted review-page ranges persist across invocation segments without granting correctness or acceptance credit. |
| [transport.rs](transport.rs) | Fit the actual adapter envelope while preserving typed file pages and exact continuations. |
| [validation.rs](validation.rs) | Exact-view accounting; savings compare with this invocation's unreduced complete exchanges. |
| [working.rs](working.rs) | Relevant current working entries; resolved, superseded and stale records stay archived, with provenance-aware duplicate suppression. |

Owning module: [assembly.rs](../assembly.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
