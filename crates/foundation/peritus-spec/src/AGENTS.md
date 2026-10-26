# crates/foundation/peritus-spec/src — navigation

## Responsibility

Immutable acceptance contracts for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [contract.rs](contract.rs) | Complete immutable acceptance contracts and exact revision binding. |
| [documents.rs](documents.rs) | Immutable content references that define contract intent and operating scope. |
| [error.rs](error.rs) | Stable acceptance-contract validation failures. |
| [evidence.rs](evidence.rs) | Required evidence and explicit human-authority declarations. |
| [gate.rs](gate.rs) | Checked deterministic gate declarations and dependency graph. |
| [gate_model.rs](gate_model.rs) | Executable refinement of the stored gate-graph validity certificate. |
| [identity.rs](identity.rs) | Stable content-addressed identifiers owned by acceptance specifications. |
| [lib.rs](lib.rs) | Immutable acceptance contracts for Peritus. |
| [policy.rs](policy.rs) | Bounded retry and review-cycle policy. |
| [proofs.rs](proofs.rs) | Formal gate-order model and acyclicity theorems. |
| [requirement.rs](requirement.rs) | Immutable requirement, exclusion, and assumption declarations. |
| [review.rs](review.rs) | Reviewer quorum, category, severity, and independence policy. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [contract/](contract/AGENTS.md) | Implementation pieces for `contract.rs`: Complete immutable acceptance contracts and exact revision binding. |
| [gate/](gate/AGENTS.md) | Implementation pieces for `gate.rs`: Checked deterministic gate declarations and dependency graph. |
| [gate_model/](gate_model/AGENTS.md) | Implementation pieces for `gate_model.rs`: Executable refinement of the stored gate-graph validity certificate. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
