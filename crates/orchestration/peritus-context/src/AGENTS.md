# crates/orchestration/peritus-context/src — navigation

## Responsibility

Provenance-aware context planning for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Authority classes are independent from the source provenance label. |
| [budget.rs](budget.rs) | Checked context-window reservation and input-token accounting. |
| [compaction.rs](compaction.rs) | Checked compaction proposals that preserve source ranges and complete lineage. |
| [content.rs](content.rs) | Bounded content bytes and semantic content classes. |
| [content_binding.rs](content_binding.rs) | Bind bounded context bytes to their exact SHA-256 digest before constructing ContextContent. |
| [error.rs](error.rs) | Stable, structured failures for context construction and planning. |
| [graph.rs](graph.rs) | Canonically ordered, bounded context dependency graph. |
| [identity.rs](identity.rs) | Caller-supplied stable identities used by context plans and compaction. |
| [lib.rs](lib.rs) | Provenance-aware context planning for Peritus. |
| [node.rs](node.rs) | Checked context nodes and canonical role/dependency metadata. |
| [plan.rs](plan.rs) | Immutable deterministic selection plans and omission explanations. |
| [precedence.rs](precedence.rs) | Deterministic comparison rules shared by selection and rendering. |
| [provenance.rs](provenance.rs) | Provenance and its authority/trust compatibility ceiling. |
| [render.rs](render.rs) | Provider-neutral typed render segments preserving every authority boundary. |
| [reuse.rs](reuse.rs) | Digest-checked context selections for role-specific run-knowledge deltas. |
| [selection.rs](selection.rs) | Deterministic required-first selection with atomic optional closure admission. |
| [trust.rs](trust.rs) | Trust labels record validation confidence without granting instruction authority. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `plan_is_visible`, `plan_dependencies_complete`, `plan_is_exact`, `token_accounting_is_bounded`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [compaction/](compaction/AGENTS.md) | Implementation pieces for `compaction.rs`: Checked compaction proposals that preserve source ranges and complete lineage. |
| [graph/](graph/AGENTS.md) | Implementation pieces for `graph.rs`: Canonically ordered, bounded context dependency graph. |
| [node/](node/AGENTS.md) | Implementation pieces for `node.rs`: Checked context nodes and canonical role/dependency metadata. |
| [reuse/](reuse/AGENTS.md) | Implementation pieces for `reuse.rs`: Digest-checked context selections for role-specific run-knowledge deltas. |
| [selection/](selection/AGENTS.md) | Implementation pieces for `selection.rs`: Deterministic required-first selection with atomic optional closure admission. |
| [working/](working/AGENTS.md) | Task-local, non-authoritative working state and deterministic reducers. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
