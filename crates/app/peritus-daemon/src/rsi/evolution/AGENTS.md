# crates/app/peritus-daemon/src/rsi/evolution — navigation

## Responsibility

Implementation pieces for `evolution.rs`: Production F0 orchestration over the existing durable owners.

## Files in this folder

| File | Role |
| --- | --- |
| [activation.rs](activation.rs) | Explicit approve-once production activation and independently authorized rollback. |
| [debugger_publication.rs](debugger_publication.rs) | Native deterministic E2 staging and publication from retained actual E3/C7 sources. |
| [evidence.rs](evidence.rs) | Captures F0 inputs only from exact durable E1, E2, E3, and D2 owning states. |
| [local_diagnosis.rs](local_diagnosis.rs) | Deterministic observation-only E2 report producer over actual published native E3/C7 evidence. |
| [local_failure_trace.rs](local_failure_trace.rs) | First-order observation import from actual published native terminal records. |
| [local_trace.rs](local_trace.rs) | Imports genuine retained C2 native process observations into C7 without inventing model work. |
| [publication.rs](publication.rs) | Real F0 publication adapter for the exact claimed durable directive. |
| [selection.rs](selection.rs) | Deterministic production attribution, deny-wins selection, and inert proposal preparation. |

Owning module: [evolution.rs](../evolution.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
