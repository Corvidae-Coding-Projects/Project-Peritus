# crates/state/peritus-approval/src — navigation

## Responsibility

Digest-bound human approval transitions for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [amendment.rs](amendment.rs) | Exact signed and approved policy-amendment identity. |
| [authentication.rs](authentication.rs) | Bounded credential snapshots and the sole strict Ed25519 verifier surface. |
| [codec.rs](codec.rs) | Bounded canonical authority codecs for durable B1 approval inputs. |
| [decision.rs](decision.rs) | Exact signed approval choices and payload bindings. |
| [digest.rs](digest.rs) | Domain-separated SHA-256 approval digests and canonical semantic encoders. |
| [failure.rs](failure.rs) | Stable fail-closed approval failures and recovery guidance. |
| [grant.rs](grant.rs) | Move-only logical approve-once consumption outputs. |
| [lib.rs](lib.rs) | Digest-bound human approval transitions for Peritus. |
| [render.rs](render.rs) | Deterministic bounded ASCII rendering of typed approval facts. |
| [request.rs](request.rs) | Canonical bounded approval requests and participant facts. |
| [state.rs](state.rs) | Move-only approval aggregate and total checked reducers. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [authentication/](authentication/AGENTS.md) | Implementation pieces for `authentication.rs`: Bounded credential snapshots and the sole strict Ed25519 verifier surface. |
| [codec/](codec/AGENTS.md) | Implementation pieces for `codec.rs`: Bounded canonical authority codecs for durable B1 approval inputs. |
| [digest/](digest/AGENTS.md) | Implementation pieces for `digest.rs`: Domain-separated SHA-256 approval digests and canonical semantic encoders. |
| [render/](render/AGENTS.md) | Implementation pieces for `render.rs`: Deterministic bounded ASCII rendering of typed approval facts. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Canonical bounded approval requests and participant facts. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Move-only approval aggregate and total checked reducers. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
