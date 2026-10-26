# crates/model/peritus-model-protocol/src — navigation

## Responsibility

Provider-neutral, versioned, bounded model protocol for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [bounds.rs](bounds.rs) | Immutable resource ceilings for one model request and response stream. |
| [canonical.rs](canonical.rs) | Stable semantic request encoding independent of provider JSON field order. |
| [canonical_decode.rs](canonical_decode.rs) | Bounded fail-closed decoding of version-one canonical model requests. |
| [capability.rs](capability.rs) | Explicit provider capability profiles and pure negotiation. |
| [content.rs](content.rs) | Bounded sensitive text, multimodal input, replay state, and provider extensions. |
| [error.rs](error.rs) | Stable model-protocol validation failures. |
| [event.rs](event.rs) | Provider-neutral normalized event grammar with bounded sensitive fragments. |
| [failure.rs](failure.rs) | Provider-neutral failures with transport phase, certainty, and redacted detail. |
| [finish.rs](finish.rs) | Normalized provider finish reasons and exactly-one terminal outcomes. |
| [identity.rs](identity.rs) | Checked public names and redacted provider identities. |
| [json_duplicates.rs](json_duplicates.rs) | Recursive duplicate-key detection before JSON maps discard duplicates. |
| [lib.rs](lib.rs) | Provider-neutral, versioned, bounded model protocol for Peritus. |
| [message.rs](message.rs) | Ordered role/content messages with role-specific structural validation. |
| [message_codec.rs](message_codec.rs) | Profile-independent canonical message archives using the existing C5 content encoding. |
| [rate_limit.rs](rate_limit.rs) | Provider rate-limit and cache observations without authority semantics. |
| [redaction.rs](redaction.rs) | Bounded allowlisted diagnostics that cannot contain provider payloads. |
| [reducer.rs](reducer.rs) | Ordered bounded event reduction with fail-closed terminal classification. |
| [request.rs](request.rs) | Complete revision-bound model requests and cross-field validation. |
| [retry.rs](retry.rs) | Deterministic bounded retry legality and delay planning. |
| [schema.rs](schema.rs) | Bounded canonical JSON values and provider-facing JSON Schema documents. |
| [tool.rs](tool.rs) | Provider-facing function tools, completed calls/results, and output/reasoning policy. |
| [usage.rs](usage.rs) | Optional nonnegative usage counters and monotonic cumulative accounting. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `ReducerTransitionFacts`, `reducer_transition_legal`, `DeduplicationFacts`, `deduplication_legal`, `FragmentCompletionFacts`. |
| [version.rs](version.rs) | C5 model-protocol version identity. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [canonical/](canonical/AGENTS.md) | Implementation pieces for `canonical.rs`: Stable semantic request encoding independent of provider JSON field order. |
| [canonical_decode/](canonical_decode/AGENTS.md) | Implementation pieces for `canonical_decode.rs`: Bounded fail-closed decoding of version-one canonical model requests. |
| [capability/](capability/AGENTS.md) | Implementation pieces for `capability.rs`: Explicit provider capability profiles and pure negotiation. |
| [event_codec/](event_codec/AGENTS.md) | Versioned canonical encoding for normalized provider event envelopes. |
| [identity/](identity/AGENTS.md) | Implementation pieces for `identity.rs`: Checked public names and redacted provider identities. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Ordered bounded event reduction with fail-closed terminal classification. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Complete revision-bound model requests and cross-field validation. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
