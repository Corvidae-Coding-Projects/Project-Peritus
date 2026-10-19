# crates/app/peritus-product-runner/src/local_context/assembly — navigation

## Responsibility

Model-view construction: keep current host policy and governing input, select working state/evidence, restore current exchanges, and fit the provider transport.

## Files in this folder

| File | Role |
| --- | --- |
| [evidence.rs](evidence.rs) | Current-permission and content-validated reusable evidence; exact source pages remain local and retrievable. |
| [exchanges.rs](exchanges.rs) | Complete exchange grouping with stale-read receipts that preserve unrelated settled results. |
| [retrieval.rs](retrieval.rs) | Host-observed immutable evidence ranges survive omission of their retrieval receipts. |
| [transport.rs](transport.rs) | Fit a known adapter envelope using retrievable prefixes of already durable observations. |
| [validation.rs](validation.rs) | Exact-view accounting; savings compare with this invocation's unreduced complete exchanges. |
| [working.rs](working.rs) | Relevant current working entries; resolved, superseded and stale records stay archived, with provenance-aware duplicate suppression. |

Owning module: [assembly.rs](../assembly.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
