# crates/model/peritus-provider-openai/src — navigation

## Responsibility

OpenAI HTTP Responses adapter and official Codex account runtime transport, with separate request/stream/runtime paths.

## Files in this folder

| File | Role |
| --- | --- |
| [client.rs](client.rs) | Provider composition, credential timing, HTTP submission, and stream construction. |
| [config.rs](config.rs) | Validated production endpoint, credential, and routing configuration. |
| [error.rs](error.rs) | Redaction-safe local errors and provider failure construction. |
| [lib.rs](lib.rs) | First-party `OpenAI` Responses adapter for Peritus. |
| [profile.rs](profile.rs) | Exact first-party Responses profile validation. |
| [request.rs](request.rs) | Responses request planning, validation, and HTTP construction. |
| [runtime.rs](runtime.rs) | Constrained account-backed transport through `OpenAI`'s credential-owning `Codex` executable. |
| [stream.rs](stream.rs) | Bounded `OpenAI` Responses SSE ownership and normalized event emission. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-provider-openai`; follow each binary into its package implementation. |
| [client/](client/AGENTS.md) | Implementation pieces for `client.rs`: Provider composition, credential timing, HTTP submission, and stream construction. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Responses request planning, validation, and HTTP construction. |
| [runtime/](runtime/AGENTS.md) | Implementation pieces for `runtime.rs`: Constrained account-backed transport through `OpenAI`'s credential-owning `Codex` executable. |
| [stream/](stream/AGENTS.md) | Implementation pieces for `stream.rs`: Bounded `OpenAI` Responses SSE ownership and normalized event emission. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
