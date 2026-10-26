# crates/state/peritus-leases/src/transition — navigation

## Responsibility

Implementation pieces for `transition.rs`: Pure checked reducers and move-only logical transition values.

## Files in this folder

| File | Role |
| --- | --- |
| [authority.rs](authority.rs) | Exact policy-use and current-lease intersection reducer. |
| [authority_validation.rs](authority_validation.rs) | Exact executable policy/lease intersection validation. |
| [core.rs](core.rs) | Linear authority-time advancement and exact transition construction. |
| [core_proofs.rs](core_proofs.rs) | Exact proof bridges for the linear transition constructor. |
| [fencing.rs](fencing.rs) | Holder-loss, clock-discontinuity, and revocation fencing reducers. |
| [fencing_apply.rs](fencing_apply.rs) | Shared fence application after command-specific validation. |
| [fencing_model.rs](fencing_model.rs) | Exact internal first-error and authority-time model for fencing application. |
| [fencing_rejections.rs](fencing_rejections.rs) | Exact rejection constructors for command-specific fencing decision contracts. |
| [fencing_retire.rs](fencing_retire.rs) | Terminal fencing transition when version or generation space is exhausted. |
| [fencing_validation.rs](fencing_validation.rs) | Command-specific executable fencing guards and their exact specifications. |
| [lifecycle.rs](lifecycle.rs) | Mint, acquisition, and renewal reducers. |
| [reconciliation.rs](reconciliation.rs) | Exactly correlated reconciliation and terminal retirement reducer. |
| [record_duplication.rs](record_duplication.rs) | Duplication of unprivileged transition records. |
| [validation.rs](validation.rs) | Shared validation and checked-successor helpers for pure reducers. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [fencing/](fencing/AGENTS.md) | Implementation pieces for `fencing.rs`: Holder-loss, clock-discontinuity, and revocation fencing reducers. |
| [lifecycle/](lifecycle/AGENTS.md) | Implementation pieces for `lifecycle.rs`: Mint, acquisition, and renewal reducers. |
| [reconciliation/](reconciliation/AGENTS.md) | Implementation pieces for `reconciliation.rs`: Exactly correlated reconciliation and terminal retirement reducer. |
| [validation/](validation/AGENTS.md) | Implementation pieces for `validation.rs`: Shared validation and checked-successor helpers for pure reducers. |

Owning module: [transition.rs](../transition.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
