# crates/model/peritus-provider-openai/fixtures/v1 — navigation

## Responsibility

Retained wire/provider fixture inputs for `peritus-provider-openai` at version `v1`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code.

## Files in this folder

| File | Role |
| --- | --- |
| [MANIFEST](MANIFEST) | Corpus identity, upstream contract sources, and fixture purpose for the neighboring retained inputs. |
| [SHA256SUMS](SHA256SUMS) | Recorded SHA-256 inventory for neighboring retained input/artifact files; keep bytes and digest entries consistent. |
| [auth-error.json](auth-error.json) | Structured auth error data with fields `error`. |
| [corrupt.sse](corrupt.sse) | Recorded stream/input bytes for corrupt. |
| [error-after-success.sse](error-after-success.sse) | Recorded stream/input bytes for error after success. |
| [golden-minimal-request.json](golden-minimal-request.json) | Structured golden minimal request data with fields `model`, `input`, `stream`, `store`, `background`. |
| [golden-realistic-request.json](golden-realistic-request.json) | Structured golden realistic request data with fields `model`, `input`, `stream`, `store`, `background`. |
| [incomplete.sse](incomplete.sse) | Recorded stream/input bytes for incomplete. |
| [ordered-duplicate.sse](ordered-duplicate.sse) | Recorded stream/input bytes for ordered duplicate. |
| [quota-error.json](quota-error.json) | Structured quota error data with fields `error`. |
| [rate-error.json](rate-error.json) | Structured rate error data with fields `error`. |
| [runtime-auth-error.jsonl](runtime-auth-error.jsonl) | Checked-in data/record for runtime auth error. |
| [runtime-golden-prompt.txt](runtime-golden-prompt.txt) | Checked-in data/record for runtime golden prompt. |
| [runtime-golden-schema.json](runtime-golden-schema.json) | Structured runtime golden schema data with fields `additionalProperties`, `properties`, `required`, `type`. |
| [runtime-incomplete.jsonl](runtime-incomplete.jsonl) | Checked-in data/record for runtime incomplete. |
| [runtime-malformed.jsonl](runtime-malformed.jsonl) | Checked-in data/record for runtime malformed. |
| [runtime-native-tool.jsonl](runtime-native-tool.jsonl) | Checked-in data/record for runtime native tool. |
| [runtime-ordered-duplicate.jsonl](runtime-ordered-duplicate.jsonl) | Checked-in data/record for runtime ordered duplicate. |
| [runtime-success.jsonl](runtime-success.jsonl) | Checked-in data/record for runtime success. |
| [runtime-tool.jsonl](runtime-tool.jsonl) | Checked-in data/record for runtime tool. |
| [success.sse](success.sse) | Recorded stream/input bytes for success. |
| [tool-reasoning.sse](tool-reasoning.sse) | Recorded stream/input bytes for tool reasoning. |
| [transient-error.json](transient-error.json) | Structured transient error data with fields `error`. |
| [unknown-ancillary.sse](unknown-ancillary.sse) | Recorded stream/input bytes for unknown ancillary. |
| [unknown-critical.sse](unknown-critical.sse) | Recorded stream/input bytes for unknown critical. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
