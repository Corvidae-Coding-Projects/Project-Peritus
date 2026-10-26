# crates/model/peritus-provider-compatible/fixtures/v1 — navigation

## Responsibility

Retained wire/provider fixture inputs for `peritus-provider-compatible` at version `v1`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code.

## Files in this folder

| File | Role |
| --- | --- |
| [MANIFEST](MANIFEST) | Corpus identity, upstream contract sources, and fixture purpose for the neighboring retained inputs. |
| [SHA256SUMS](SHA256SUMS) | Recorded SHA-256 inventory for neighboring retained input/artifact files; keep bytes and digest entries consistent. |
| [auth-error.json](auth-error.json) | Structured auth error data with fields `error`. |
| [chat-corrupt.sse](chat-corrupt.sse) | Recorded stream/input bytes for chat corrupt. |
| [chat-error-after-http-success.sse](chat-error-after-http-success.sse) | Recorded stream/input bytes for chat error after http success. |
| [chat-incomplete.sse](chat-incomplete.sse) | Recorded stream/input bytes for chat incomplete. |
| [chat-refusal.sse](chat-refusal.sse) | Recorded stream/input bytes for chat refusal. |
| [chat-success.sse](chat-success.sse) | Recorded stream/input bytes for chat success. |
| [chat-tool.sse](chat-tool.sse) | Recorded stream/input bytes for chat tool. |
| [chat-unknown-ancillary.sse](chat-unknown-ancillary.sse) | Recorded stream/input bytes for chat unknown ancillary. |
| [chat-unknown-critical.sse](chat-unknown-critical.sse) | Recorded stream/input bytes for chat unknown critical. |
| [golden-chat-request.json](golden-chat-request.json) | Structured golden chat request data with fields `max_completion_tokens`, `messages`, `model`, `parallel_tool_calls`, `response_format`. |
| [golden-responses-request.json](golden-responses-request.json) | Structured golden responses request data with fields `background`, `input`, `max_output_tokens`, `model`, `parallel_tool_calls`. |
| [rate-error.json](rate-error.json) | Structured rate error data with fields `error`. |
| [responses-corrupt.sse](responses-corrupt.sse) | Recorded stream/input bytes for responses corrupt. |
| [responses-error-after-http-success.sse](responses-error-after-http-success.sse) | Recorded stream/input bytes for responses error after http success. |
| [responses-incomplete.sse](responses-incomplete.sse) | Recorded stream/input bytes for responses incomplete. |
| [responses-no-usage.sse](responses-no-usage.sse) | Recorded stream/input bytes for responses no usage. |
| [responses-ordered-duplicate.sse](responses-ordered-duplicate.sse) | Recorded stream/input bytes for responses ordered duplicate. |
| [responses-success.sse](responses-success.sse) | Recorded stream/input bytes for responses success. |
| [responses-tool.sse](responses-tool.sse) | Recorded stream/input bytes for responses tool. |
| [responses-unknown-ancillary.sse](responses-unknown-ancillary.sse) | Recorded stream/input bytes for responses unknown ancillary. |
| [responses-unknown-critical.sse](responses-unknown-critical.sse) | Recorded stream/input bytes for responses unknown critical. |
| [transient-error.json](transient-error.json) | Structured transient error data with fields `error`. |
| [zen-deepseek-batched-choice.sse](zen-deepseek-batched-choice.sse) | Recorded stream/input bytes for zen deepseek batched choice. |
| [zen-deepseek-live-1.sse](zen-deepseek-live-1.sse) | Recorded stream/input bytes for zen deepseek live 1. |
| [zen-deepseek-live-2.sse](zen-deepseek-live-2.sse) | Recorded stream/input bytes for zen deepseek live 2. |
| [zen-deepseek-live-3.sse](zen-deepseek-live-3.sse) | Recorded stream/input bytes for zen deepseek live 3. |

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
