# crates/orchestration/peritus-memory/src — navigation

## Responsibility

Scoped derived-memory lifecycle and retrieval for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [claim.rs](claim.rs) | Typed derived claims, source provenance, and provider-neutral retrieval features. |
| [confidence.rs](confidence.rs) | Checked bounded integer scores used by validation and deterministic ranking. |
| [error.rs](error.rs) | Stable, actionable memory validation and planning failures. |
| [evidence.rs](evidence.rs) | Canonical bounded source-event and evidence identifier sets. |
| [feedback.rs](feedback.rs) | Explicit bounded positive and negative retrieval feedback. |
| [identity.rs](identity.rs) | Caller-supplied fixed-width identities used by memory records and indexes. |
| [index.rs](index.rs) | Canonical replay-derived memory index. |
| [lib.rs](lib.rs) | Scoped derived-memory lifecycle and retrieval for Peritus. |
| [lifecycle.rs](lifecycle.rs) | Logical observations and explicit immutable memory lifecycle states. |
| [record.rs](record.rs) | Immutable memory records and checked lifecycle revisions. |
| [retrieval.rs](retrieval.rs) | Deterministic filter, rank, and budget retrieval planning. |
| [scope.rs](scope.rs) | Checked durable memory scopes and explicit compatibility policy. |
| [tombstone.rs](tombstone.rs) | Deletion tombstones that dominate replay without retaining deleted content. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `memory_is_non_authority`, `lifecycle_advanced`, `deletion_dominates`, `retrieval_is_bounded`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [index/](index/AGENTS.md) | Implementation pieces for `index.rs`: Canonical replay-derived memory index. |
| [record/](record/AGENTS.md) | Implementation pieces for `record.rs`: Immutable memory records and checked lifecycle revisions. |
| [retrieval/](retrieval/AGENTS.md) | Implementation pieces for `retrieval.rs`: Deterministic filter, rank, and budget retrieval planning. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
