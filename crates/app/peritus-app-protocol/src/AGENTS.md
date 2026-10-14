# crates/app/peritus-app-protocol/src — navigation

## Responsibility

Transport-neutral application protocol for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [doctor.rs](doctor.rs) | Bounded non-mutating product diagnostics; observations never grant authority. |
| [family.rs](family.rs) | Stable application-protocol schema and PRTS family assignments. |
| [identity.rs](identity.rs) | Stable identities owned by the application protocol. |
| [improvements.rs](improvements.rs) | Evidence-backed suggestions. Only an explicit evaluation request may start inference. |
| [lib.rs](lib.rs) | Transport-neutral application protocol for Peritus. |
| [limits.rs](limits.rs) | Explicit resource limits for application-protocol messages and pure state machines. |
| [product.rs](product.rs) | Product-level coding-run messages exposed to interactive clients. |
| [workbench.rs](workbench.rs) | Additive revisioned workbench controls. DTO validation never authenticates an actor. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [artifact/](artifact/AGENTS.md) | Bounded, transport-neutral artifact transfer contracts. |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-app-protocol`; follow each binary into its package implementation. |
| [command/](command/AGENTS.md) | Exact B3 command submission binding, final results, and bounded idempotency. |
| [daemon/](daemon/AGENTS.md) | Honest daemon readiness, heartbeat, and shutdown observations. |
| [envelope/](envelope/AGENTS.md) | Typed post-negotiation application envelopes. |
| [error/](error/AGENTS.md) | Stable application-protocol error vocabulary. |
| [identity/](identity/AGENTS.md) | Implementation pieces for `identity.rs`: Stable identities owned by the application protocol. |
| [product/](product/AGENTS.md) | Implementation pieces for `product.rs`: Product-level coding-run messages exposed to interactive clients. |
| [prompt/](prompt/AGENTS.md) | Correlated, revision-fresh prompt values and pure admission state. |
| [schema/](schema/AGENTS.md) | Deterministic application-protocol schema and compatibility artifact generation. |
| [subscription/](subscription/AGENTS.md) | Bounded at-least-once event subscription values and state transitions. |
| [terminal/](terminal/AGENTS.md) | Bounded terminal attachment records and ordering state. |
| [verified/](verified/AGENTS.md) | Executable invariant/refinement helpers; older proof correspondence is historical. Entry names: `version_supported`, `negotiation_safe`, `next_cursor`, `delivery_advances`, `ack_legal`. |
| [version/](version/AGENTS.md) | Protocol version, feature, limit, and compatibility negotiation. |
| [wire/](wire/AGENTS.md) | Canonical binary codecs and closed dispatcher for all A3 application families. |
| [workbench/](workbench/AGENTS.md) | Implementation pieces for `workbench.rs`: Additive revisioned workbench controls. DTO validation never authenticates an actor. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
