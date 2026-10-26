# docs/testing/reproducers/rsi-production/evolution — navigation

## Responsibility

peritus-rsi-evolution-replay package.

Package `peritus-rsi-evolution-replay` is a standalone consumer/example package outside root workspace membership.

Local dependency waypoints: [peritus-daemon](../../../../../crates/app/peritus-daemon/AGENTS.md), [peritus-product-runner](../../../../../crates/app/peritus-product-runner/AGENTS.md), [peritus-harness](../../../../../crates/orchestration/peritus-harness/AGENTS.md), [peritus-evolution](../../../../../crates/analysis/peritus-evolution/AGENTS.md), [peritus-artifact-store](../../../../../crates/state/peritus-artifact-store/AGENTS.md), [peritus-journal](../../../../../crates/state/peritus-journal/AGENTS.md), [peritus-evidence](../../../../../crates/state/peritus-evidence/AGENTS.md), [peritus-git](../../../../../crates/runtime/peritus-git/AGENTS.md), [peritus-workspace](../../../../../crates/runtime/peritus-workspace/AGENTS.md), [peritus-patch](../../../../../crates/runtime/peritus-patch/AGENTS.md), [peritus-types](../../../../../crates/foundation/peritus-types/AGENTS.md), [peritus-review](../../../../../crates/orchestration/peritus-review/AGENTS.md), [peritus-codec](../../../../../crates/foundation/peritus-codec/AGENTS.md), [peritus-debugger](../../../../../crates/analysis/peritus-debugger/AGENTS.md), [peritus-eval](../../../../../crates/analysis/peritus-eval/AGENTS.md), [peritus-context](../../../../../crates/orchestration/peritus-context/AGENTS.md), [peritus-role](../../../../../crates/orchestration/peritus-role/AGENTS.md), [peritus-quality-policy](../../../../../crates/foundation/peritus-quality-policy/AGENTS.md), [peritus-approval](../../../../../crates/state/peritus-approval/AGENTS.md), [peritus-budget](../../../../../crates/foundation/peritus-budget/AGENTS.md), [peritus-kernel](../../../../../crates/foundation/peritus-kernel/AGENTS.md), [peritus-policy](../../../../../crates/foundation/peritus-policy/AGENTS.md), [peritus-protocol](../../../../../crates/foundation/peritus-protocol/AGENTS.md), [peritus-spec](../../../../../crates/foundation/peritus-spec/AGENTS.md), [peritus-trace](../../../../../crates/observe/peritus-trace/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.lock](Cargo.lock) | Locked dependency resolution for the neighboring package manifest. |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-rsi-evolution-replay`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [activation.rs](activation.rs) | Real F0 host authority adapter for the disposable native fixture only. Fresh explicit CLI approval input is required separately for promotion and rollback. |
| [e1.rs](e1.rs) | Real E1 materialization producer. No fabricated receipt, publication, or approval constructors. |
| [f0.rs](f0.rs) | Actual owner composition over the disposable native E1/E2/E3 fixture. |
| [observed-owner-results.json](observed-owner-results.json) | Structured observed owner results data with fields `schema`, `note`, `materialization`, `independent_review`, `selection`. |
| [publication.rs](publication.rs) | Read back actual published F0 owner evidence; never claim or acknowledge an outbox row here. |
| [review.rs](review.rs) | Imports an independently executed review into its actual D2 owner. This consumer accepts only the fresh delegated review's final scoped result. It never manufactures a review, provider identity, finding, or approval. |

## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
