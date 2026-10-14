# crates/model/peritus-model-protocol/src/event_codec — navigation

## Responsibility

Versioned canonical encoding for normalized provider event envelopes.

## Files in this folder

| File | Role |
| --- | --- |
| [decode.rs](decode.rs) | Decodes canonical normalized-event schema-version-one bytes. Decoding reconstructs inert normalized data only. Stream legality and terminal semantics are |
| [encode.rs](encode.rs) | Encodes one normalized event envelope into canonical schema-version-one bytes. Provider raw bytes are not included; the envelope's exact provider digest and all normalized |
| [mod.rs](mod.rs) | Versioned canonical encoding for normalized provider event envelopes. |
| [primitive.rs](primitive.rs) | Local implementation for event_codec. Entry names: `MAGIC`, `MAX_CANONICAL_EVENT_BYTES`, `codec_limits`, `max_usize`, `write_codec`. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
