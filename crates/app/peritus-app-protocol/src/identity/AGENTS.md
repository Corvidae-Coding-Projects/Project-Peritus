# crates/app/peritus-app-protocol/src/identity — navigation

## Responsibility

Implementation pieces for `identity.rs`: Stable identities owned by the application protocol.

## Files in this folder

| File | Role |
| --- | --- |
| [flow.rs](flow.rs) | Subscription, artifact, and prompt identifiers. |
| [inputs.rs](inputs.rs) | Nominal workbench input identity, distinct from transport requests and control operations. |
| [invocation.rs](invocation.rs) | Nominal identity for a sealed model-request invocation, not a transport request. |
| [request.rs](request.rs) | Request and relationship identifiers. |
| [runtime.rs](runtime.rs) | Terminal, delivery-attempt, and heartbeat identifiers. |
| [workbench.rs](workbench.rs) | Nominal user-conversation and control-operation identities, distinct from runs and sessions. |

Owning module: [identity.rs](../identity.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
