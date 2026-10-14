# crates/state/peritus-approval/src/codec — navigation

## Responsibility

Implementation pieces for `codec.rs`: Bounded canonical authority codecs for durable B1 approval inputs.

## Files in this folder

| File | Role |
| --- | --- |
| [decision.rs](decision.rs) | Canonical signed approval-decision authority frames. |
| [reader.rs](reader.rs) | Exact bounded readers for the B1 canonical field-record grammar. |
| [registry.rs](registry.rs) | Canonical credential-registry snapshot decoding. |
| [request.rs](request.rs) | Canonical approval-request authority frames. |
| [value.rs](value.rs) | Shared exact value encoders and checked decoders for approval codec records. |

Owning module: [codec.rs](../codec.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
