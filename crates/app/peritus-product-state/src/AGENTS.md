# crates/app/peritus-product-state/src — navigation

## Responsibility

Pure, resumable product state for the single-command Peritus experience.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Product-state validation failures. |
| [identity.rs](identity.rs) | Stable non-secret installation identity. |
| [lib.rs](lib.rs) | Pure, resumable product state for the single-command Peritus experience. |
| [phase.rs](phase.rs) | Durable bootstrap phases and derived live-launch readiness. |
| [provider.rs](provider.rs) | Durable provider choices without credentials or live-status claims. |
| [state.rs](state.rs) | Versioned durable product-state document. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `BootstrapPhaseModel`, `bootstrap_transition_model_exec`, `provider_failover_shape_model_exec`, `WorkspaceTrustModel`, `workspace_registration_shape_model_exec`. |
| [workspace.rs](workspace.rs) | Durable workspace choices and remembered trust without live filesystem claims. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [provider/](provider/AGENTS.md) | Implementation pieces for `provider.rs`: Durable provider choices without credentials or live-status claims. |
| [workspace/](workspace/AGENTS.md) | Implementation pieces for `workspace.rs`: Durable workspace choices and remembered trust without live filesystem claims. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
