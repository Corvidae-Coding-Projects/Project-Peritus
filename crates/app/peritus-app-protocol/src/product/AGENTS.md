# crates/app/peritus-app-protocol/src/product — navigation

## Responsibility

Implementation pieces for `product.rs`: Product-level coding-run messages exposed to interactive clients.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Explicit run-to-conversation discovery without inferring identities from run identifiers. |
| [control.rs](control.rs) | Explicit control operations for daemon-owned product runs. |
| [effort.rs](effort.rs) | Explicit user-selected reasoning effort, independent of a provider's model identifier. |
| [error.rs](error.rs) | Product-run message construction failures. |
| [interaction.rs](interaction.rs) | Additive conversation-first execution requests and honest user-input lifecycle observations. |
| [models.rs](models.rs) | Explicit model choices and provider-catalog observations for interactive clients. |
| [observation.rs](observation.rs) | Mixed active and settled run observations without inventing terminal evidence. |
| [operation.rs](operation.rs) | Authoritative operation knowledge and legal controls projected with every product run. |
| [phase.rs](phase.rs) | Stable user-facing product-run phases. |
| [query.rs](query.rs) | Exact interaction lookup for a durable product run. |
| [request.rs](request.rs) | Checked requests and provider-role selection for product runs. |
| [settlement.rs](settlement.rs) | Product-facing projection of a verified terminal run settlement. |
| [snapshot.rs](snapshot.rs) | Bounded product-run observation snapshots. |

Owning module: [product.rs](../product.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
