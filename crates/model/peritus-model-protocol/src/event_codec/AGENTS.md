# crates/model/peritus-model-protocol/src/event_codec — navigation

## Responsibility

Versioned canonical encoding for normalized provider event envelopes.

## Files in this folder

| File | Role |
| --- | --- |
| [decode.rs](decode.rs) | Decodes canonical normalized-event schema-version-one bytes. Decoding reconstructs inert normalized data only. Stream legality and terminal semantics are |
| [encode.rs](encode.rs) | Encodes one normalized event envelope into canonical schema-version-one bytes. Provider raw bytes are not included; the envelope's exact provider digest and all normalized |
| [mod.rs](mod.rs) | Versioned canonical encoding for normalized provider event envelopes. |
| [primitive.rs](primitive.rs) | Local implementation for event_codec. Entry names: `MAGIC`, `codec_limits`, `max_usize`, `write_codec`. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
