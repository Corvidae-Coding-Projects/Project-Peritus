# crates/tools/peritus-tool-protocol/src — navigation

## Responsibility

Bounded canonical model-facing tool protocol.

## Files in this folder

| File | Role |
| --- | --- |
| [artifact.rs](artifact.rs) | Content-addressed artifact references without filesystem paths. |
| [call.rs](call.rs) | Versioned bounded proposed tool-call envelope. |
| [control.rs](control.rs) | Supported-control declarations and bounded control envelopes. |
| [descriptor.rs](descriptor.rs) | Immutable versioned tool descriptor. |
| [envelope.rs](envelope.rs) | Common bounded view over canonical versioned envelope bytes. |
| [error.rs](error.rs) | Stable protocol validation errors. |
| [identity.rs](identity.rs) | Validated protocol identities and bounded text. |
| [lib.rs](lib.rs) | Bounded canonical model-facing tool protocol. |
| [limits.rs](limits.rs) | Production JSON parsing ceilings and explicit construction capacities. |
| [prepared.rs](prepared.rs) | Effect-free descriptor-bound call preparation and replay identity. |
| [progress.rs](progress.rs) | Ordered bounded progress envelope. |
| [result.rs](result.rs) | Truthful closed terminal result envelopes. |
| [schema.rs](schema.rs) | Deterministic bounded JSON values and the supported schema subset. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `ProtocolBoundFacts`, `protocol_bounds_complete`, `canonical_order_complete`, `schema_shape_complete`. |
| [wire.rs](wire.rs) | Small deterministic envelope encoding primitives. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [descriptor/](descriptor/AGENTS.md) | Implementation pieces for `descriptor.rs`: Immutable versioned tool descriptor. |
| [result/](result/AGENTS.md) | Implementation pieces for `result.rs`: Truthful closed terminal result envelopes. |
| [schema/](schema/AGENTS.md) | Implementation pieces for `schema.rs`: Deterministic bounded JSON values and the supported schema subset. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
