# crates/analysis/peritus-eval/src — navigation

## Responsibility

Durable reproducible harness evaluation for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Stable redaction-safe E3 failure vocabulary. |
| [identity.rs](identity.rs) | E3-owned validated identities and digest roles. |
| [lib.rs](lib.rs) | Durable reproducible harness evaluation for Peritus. |
| [limits.rs](limits.rs) | Independent compiled and caller-selected E3 bounds. |
| [projection.rs](projection.rs) | Rebuildable read-only campaign progress projection. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `accounting_conserved`, `pass_at_k_preconditions`, `terminal_dominates`, `FrozenProfileFacts`, `new`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [accounting/](accounting/AGENTS.md) | Complete rollout attempt, terminal, and resource accounting. |
| [aggregate/](aggregate/AGENTS.md) | Pure durable evaluation campaign aggregate. |
| [dataset/](dataset/AGENTS.md) | Immutable dataset manifests and candidate/evaluator isolation. |
| [durability/](durability/AGENTS.md) | C0 aggregate binding, outbox directives, atomic commits, and replay. |
| [execution/](execution/AGENTS.md) | Isolated two-stage rollout execution contracts. |
| [plan/](plan/AGENTS.md) | Deterministic paired rollout planning and D3 work binding. |
| [profile/](profile/AGENTS.md) | Frozen evaluation bindings and metric/failure policies. |
| [report/](report/AGENTS.md) | Deterministic analysis and immutable inert evaluation reports. |
| [runtime/](runtime/AGENTS.md) | Narrow effect orchestration over existing C0 owners. |
| [statistics/](statistics/AGENTS.md) | Portable bounded E3 statistical calculations. |
| [wire/](wire/AGENTS.md) | Canonical inert B3 schema-v1 families 85, 86, and 87. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
