# crates/model/peritus-provider-compatible/src — navigation

## Responsibility

Explicitly profiled compatible model-provider adapters for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [client.rs](client.rs) | Provider composition, credential timing, conservative retries, and stream ownership. |
| [config.rs](config.rs) | Exact endpoint, header authentication, fixed headers, limits, and retry configuration. |
| [error.rs](error.rs) | Redaction-safe compatible-adapter errors and normalized failures. |
| [hosted_reasoning.rs](hosted_reasoning.rs) | Closed hosted reasoning fields preserved for exact subsequent tool-result replay. |
| [lib.rs](lib.rs) | Explicitly profiled compatible model-provider adapters for Peritus. |
| [profile.rs](profile.rs) | Separately validated Responses and Chat Completions compatibility contracts. |
| [request.rs](request.rs) | Fail-closed validation and private wire projection for both compatible dialects. |
| [stream.rs](stream.rs) | Bounded SSE ownership and normalized compatible event emission. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [client/](client/AGENTS.md) | Implementation pieces for `client.rs`: Provider composition, credential timing, conservative retries, and stream ownership. |
| [config/](config/AGENTS.md) | Implementation pieces for `config.rs`: Exact endpoint, header authentication, fixed headers, limits, and retry configuration. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Fail-closed validation and private wire projection for both compatible dialects. |
| [stream/](stream/AGENTS.md) | Implementation pieces for `stream.rs`: Bounded SSE ownership and normalized compatible event emission. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
