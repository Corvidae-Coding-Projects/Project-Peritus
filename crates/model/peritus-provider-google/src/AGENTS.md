# crates/model/peritus-provider-google/src — navigation

## Responsibility

First-party stable-v1 Google Gemini adapter for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [client.rs](client.rs) | Provider ownership, authenticated stable-v1 submission, and conservative retry execution. |
| [config.rs](config.rs) | Immutable endpoint, credential, profile, and resource configuration. |
| [error.rs](error.rs) | Redacted Google HTTP, stream, and transport failure classification. |
| [lib.rs](lib.rs) | First-party stable-v1 Google Gemini adapter for Peritus. |
| [profile.rs](profile.rs) | Exact stable-v1 Google dialect and lifecycle validation. |
| [request.rs](request.rs) | Checked provider-neutral request to private stable-v1 Google JSON projection. |
| [stream.rs](stream.rs) | Cancellation-aware pull stream over bounded Google stable-v1 SSE framing. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Checked provider-neutral request to private stable-v1 Google JSON projection. |
| [stream/](stream/AGENTS.md) | Implementation pieces for `stream.rs`: Cancellation-aware pull stream over bounded Google stable-v1 SSE framing. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
