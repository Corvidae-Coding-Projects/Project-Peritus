# crates/model/peritus-provider-anthropic/fixtures/v1 — navigation

## Responsibility

Retained wire/provider fixture inputs for `peritus-provider-anthropic` at version `v1`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code.

## Files in this folder

| File | Role |
| --- | --- |
| [MANIFEST](MANIFEST) | Corpus identity, upstream contract sources, and fixture purpose for the neighboring retained inputs. |
| [SHA256SUMS](SHA256SUMS) | Recorded SHA-256 inventory for neighboring retained input/artifact files; keep bytes and digest entries consistent. |
| [auth_error.json](auth_error.json) | Structured auth error data with fields `type`, `error`, `request_id`. |
| [corrupt.sse](corrupt.sse) | Recorded stream/input bytes for corrupt. |
| [error_after_success.sse](error_after_success.sse) | Recorded stream/input bytes for error after success. |
| [golden_request.json](golden_request.json) | Structured golden request data with fields `model`, `max_tokens`, `messages`, `stream`, `tool_choice`. |
| [incomplete.sse](incomplete.sse) | Recorded stream/input bytes for incomplete. |
| [rate_error.json](rate_error.json) | Structured rate error data with fields `type`, `error`, `request_id`. |
| [runtime_auth_false.json](runtime_auth_false.json) | Structured runtime auth false data with fields `loggedIn`. |
| [runtime_auth_true.json](runtime_auth_true.json) | Structured runtime auth true data with fields `loggedIn`. |
| [runtime_error.json](runtime_error.json) | Structured runtime error data with fields `is_error`, `result`. |
| [runtime_incomplete.json](runtime_incomplete.json) | Structured runtime incomplete data with fields `is_error`, `result`. |
| [runtime_malformed.json](runtime_malformed.json) | Structured runtime malformed data with fields `is_error`, `structured_output`. |
| [runtime_success.json](runtime_success.json) | Structured runtime success data with fields `is_error`, `structured_output`, `usage`. |
| [runtime_tool.json](runtime_tool.json) | Structured runtime tool data with fields `is_error`, `structured_output`, `usage`. |
| [text.sse](text.sse) | Recorded stream/input bytes for text. |
| [tool_thinking.sse](tool_thinking.sse) | Recorded stream/input bytes for tool thinking. |
| [transient_error.json](transient_error.json) | Structured transient error data with fields `type`, `error`, `request_id`. |
| [unknown_ancillary.sse](unknown_ancillary.sse) | Recorded stream/input bytes for unknown ancillary. |
| [unknown_critical.sse](unknown_critical.sse) | Recorded stream/input bytes for unknown critical. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
