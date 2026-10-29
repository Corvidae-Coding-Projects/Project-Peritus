# crates/model/peritus-provider-anthropic/src — navigation

## Responsibility

Anthropic Messages HTTP adapter and official Claude account runtime transport, with separate native and HTTP encoding/decoding.

## Files in this folder

| File | Role |
| --- | --- |
| [client.rs](client.rs) | Provider ownership, authenticated submission, and conservative retry execution. |
| [config.rs](config.rs) | Immutable endpoint, credential, profile, beta, and resource configuration. |
| [error.rs](error.rs) | Redacted Anthropic status, stream, and transport failure classification. |
| [lib.rs](lib.rs) | First-party Anthropic Messages adapter for Peritus. |
| [profile.rs](profile.rs) | Exact Anthropic Messages profile and lifecycle validation. |
| [request.rs](request.rs) | Checked provider-neutral request to private Anthropic Messages projection. |
| [runtime.rs](runtime.rs) | Constrained account-backed transport through Anthropic's credential-owning executable. |
| [stream.rs](stream.rs) | Cancellation-aware pull stream over bounded Anthropic SSE framing. |
| [wire.rs](wire.rs) | Private Anthropic Messages request wire ownership and explicit JSON projection. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-provider-anthropic`; follow each binary into its package implementation. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Checked provider-neutral request to private Anthropic Messages projection. |
| [runtime/](runtime/AGENTS.md) | Implementation pieces for `runtime.rs`: Constrained account-backed transport through Anthropic's credential-owning executable. |
| [stream/](stream/AGENTS.md) | Implementation pieces for `stream.rs`: Cancellation-aware pull stream over bounded Anthropic SSE framing. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
