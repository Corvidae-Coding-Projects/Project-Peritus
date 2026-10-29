# crates/model/peritus-provider-openai/src — navigation

## Responsibility

OpenAI HTTP Responses adapter and official Codex account runtime transport, with separate OAuth, request, stream, and runtime paths.

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
| [chatgpt/](chatgpt/AGENTS.md) | Independent ChatGPT OAuth registration, protected sessions, renewal, account catalogs, and public Responses composition. |
| [client/](client/AGENTS.md) | Implementation pieces for `client.rs`: Provider composition, credential timing, HTTP submission, and stream construction. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Responses request planning, validation, and HTTP construction. |
| [runtime/](runtime/AGENTS.md) | Implementation pieces for `runtime.rs`: Constrained account-backed transport through `OpenAI`'s credential-owning `Codex` executable. |
| [stream/](stream/AGENTS.md) | Implementation pieces for `stream.rs`: Bounded `OpenAI` Responses SSE ownership and normalized event emission. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
