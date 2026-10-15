# crates/extensions/peritus-plugin-sdk/src — navigation

## Responsibility

Stable contracts for isolated Peritus plugins.

## Files in this folder

| File | Role |
| --- | --- |
| [canonical.rs](canonical.rs) | Deterministic JSON rendering for manifest trust material. |
| [error.rs](error.rs) | Stable SDK error categories. |
| [framing.rs](framing.rs) | Four-byte big-endian length-delimited JSON framing. |
| [identity.rs](identity.rs) | Validated plugin, request, version, and digest identities. |
| [lib.rs](lib.rs) | Stable contracts for isolated Peritus plugins. |
| [manifest.rs](manifest.rs) | Canonical plugin manifest and declared authority/resource boundaries. |
| [payload.rs](payload.rs) | Recursively bounded canonical JSON payloads. |
| [protocol.rs](protocol.rs) | Typed host/plugin request, result, lifecycle, and failure protocol. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Canonical plugin manifest and declared authority/resource boundaries. |
| [protocol/](protocol/AGENTS.md) | Implementation pieces for `protocol.rs`: Typed host/plugin request, result, lifecycle, and failure protocol. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
