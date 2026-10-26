# crates/analysis/peritus-debugger/src — navigation

## Responsibility

Durable evidence-linked trace diagnosis for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Checked immutable bindings for one production analysis subject. |
| [error.rs](error.rs) | Redaction-safe typed failures for debugger domain and effect boundaries. |
| [identity.rs](identity.rs) | Nominal E2 identities and content-address derivation. |
| [lib.rs](lib.rs) | Durable evidence-linked trace diagnosis for Peritus. |
| [limits.rs](limits.rs) | Frozen independently tighten-able resource limits for E2 analysis. |
| [projection.rs](projection.rs) | Rebuildable read-only debugger job projection. |
| [query.rs](query.rs) | Immutable content-addressed trace-selection queries. |
| [taxonomy.rs](taxonomy.rs) | Closed schema-v1 outcome and failure taxonomy. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `SelectionContainmentFacts`, `new`, `exact_subject`, `exact_trace_event`, `exact_journal_record`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [aggregate/](aggregate/AGENTS.md) | Pure durable debugger job state machine. |
| [binding/](binding/AGENTS.md) | Implementation pieces for `binding.rs`: Checked immutable bindings for one production analysis subject. |
| [causal/](causal/AGENTS.md) | Deterministic analyzer registry and bounded root-cause candidates. |
| [citation/](citation/AGENTS.md) | Manifest-contained source and ordinary-artifact citations. |
| [clustering/](clustering/AGENTS.md) | Deterministic exact-fingerprint and bounded agglomerative clustering. |
| [component/](component/AGENTS.md) | E1 component correlation and diagnostic-only harness health. |
| [durability/](durability/AGENTS.md) | C0 aggregate binding, atomic transition commits, outbox fencing, and replay. |
| [model/](model/AGENTS.md) | Provider-neutral, tool-free optional model analysis. |
| [report/](report/AGENTS.md) | Checked report claims, complete validation, and canonical bytes. |
| [runtime/](runtime/AGENTS.md) | Narrow effect orchestration over existing C0/C5/C6 owners. |
| [selection/](selection/AGENTS.md) | Deterministic immutable C7/C0 evidence selection. |
| [timeline/](timeline/AGENTS.md) | Deterministic per-subject causal timelines. |
| [wire/](wire/AGENTS.md) | Canonical inert B3 schema-v1 families 82, 83, and 84. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
