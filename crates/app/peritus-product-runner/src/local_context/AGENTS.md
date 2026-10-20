# crates/app/peritus-product-runner/src/local_context — navigation

## Responsibility

Durable working-context host: journal/artifact storage, current-policy assembly, exact evidence retrieval, derived entries, compaction, and executor wrapper.

## Files in this folder

| File | Role |
| --- | --- |
| [pages.rs](pages.rs) | Content-addressed working/source pages, atomic checkpoint roots, and legacy snapshot restoration. |
| [assembly.rs](assembly.rs) | Semantic pinning, complete current exchanges, and local-only evidence selection. |
| [checkpoint_validation.rs](checkpoint_validation.rs) | Shared semantic validation for recovered and read-only inspected checkpoints. |
| [driver.rs](driver.rs) | Shared production invocation switch; legacy mode is explicit and never an error fallback. |
| [memory.rs](memory.rs) | Durable state owner shared by the loop port and its bounded memory-tool decorator. |
| [mod.rs](mod.rs) | Local working-memory host: C0 effects around C6 deterministic state. |
| [port.rs](port.rs) | Shared context port, including separate per-job/per-stage module lineages; locks do not cross task-provider requests or base-tool execution. |
| [record.rs](record.rs) | Version-one host journal payloads and rebuildable transcript/source projections. |
| [read_reuse.rs](read_reuse.rs) | Role-scoped version/range coverage and delivery reuse derived from the durable source archive. |
| [semantic.rs](semantic.rs) | Optional local inference proposes ordinary validated deltas; every failure stays local. |
| [storage.rs](storage.rs) | Single-owner C0 journal/artifact storage and atomic checkpoint-root publication. |
| [tools.rs](tools.rs) | Bounded memory tools; the underlying executor retains every authority and grounding gate. |
| [view_binding.rs](view_binding.rs) | Canonical binding for one persisted provider view and the policy/accounting that selected it. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [assembly/](assembly/AGENTS.md) | Model-view construction: keep current host policy and governing input, select working state/evidence, restore current exchanges, and fit the provider transport. |
| [memory/](memory/AGENTS.md) | Implementation pieces for `memory.rs`: Durable state owner shared by the loop port and its bounded memory-tool decorator. |
| [read_reuse/](read_reuse/AGENTS.md) | Exact archived source slicing and final provider-view coverage. |
| [storage/](storage/AGENTS.md) | Implementation pieces for `storage.rs`: Single-owner C0 journal/artifact storage and atomic checkpoint-root publication. |
| [tools/](tools/AGENTS.md) | Model-facing context read/update operations and schema. These retain evidence/working entries without granting workspace or process authority. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
