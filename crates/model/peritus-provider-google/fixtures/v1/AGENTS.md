# crates/model/peritus-provider-google/fixtures/v1 — navigation

## Responsibility

Retained wire/provider fixture inputs for `peritus-provider-google` at version `v1`. Manifests identify the corpus; byte digests and expected streams are data, not runtime code.

## Files in this folder

| File | Role |
| --- | --- |
| [MANIFEST](MANIFEST) | Corpus identity, upstream contract sources, and fixture purpose for the neighboring retained inputs. |
| [SHA256SUMS](SHA256SUMS) | Recorded SHA-256 inventory for neighboring retained input/artifact files; keep bytes and digest entries consistent. |
| [auth_error.json](auth_error.json) | Structured auth error data with fields `error`. |
| [corrupt.sse](corrupt.sse) | Recorded stream/input bytes for corrupt. |
| [error_after_success.sse](error_after_success.sse) | Recorded stream/input bytes for error after success. |
| [generate_error_after_success.sse](generate_error_after_success.sse) | Recorded stream/input bytes for generate error after success. |
| [generate_rate_error.json](generate_rate_error.json) | Structured generate rate error data with fields `error`. |
| [generate_success.sse](generate_success.sse) | Recorded stream/input bytes for generate success. |
| [generate_tool_thinking.sse](generate_tool_thinking.sse) | Recorded stream/input bytes for generate tool thinking. |
| [generate_unknown_ancillary.sse](generate_unknown_ancillary.sse) | Recorded stream/input bytes for generate unknown ancillary. |
| [golden_generate_minimal.json](golden_generate_minimal.json) | Structured golden generate minimal data with fields `contents`, `generationConfig`. |
| [golden_generate_realistic.json](golden_generate_realistic.json) | Structured golden generate realistic data with fields `cachedContent`, `contents`, `generationConfig`, `toolConfig`, `tools`. |
| [golden_interactions_minimal.json](golden_interactions_minimal.json) | Structured golden interactions minimal data with fields `background`, `generation_config`, `input`, `model`, `store`. |
| [golden_interactions_realistic.json](golden_interactions_realistic.json) | Structured golden interactions realistic data with fields `background`, `generation_config`, `input`, `model`, `response_format`. |
| [incomplete.sse](incomplete.sse) | Recorded stream/input bytes for incomplete. |
| [interactions_ordered_duplicate.sse](interactions_ordered_duplicate.sse) | Recorded stream/input bytes for interactions ordered duplicate. |
| [interactions_success.sse](interactions_success.sse) | Recorded stream/input bytes for interactions success. |
| [interactions_tool_thinking.sse](interactions_tool_thinking.sse) | Recorded stream/input bytes for interactions tool thinking. |
| [quota_error.json](quota_error.json) | Structured quota error data with fields `error`. |
| [rate_error.json](rate_error.json) | Structured rate error data with fields `error`. |
| [transient_error.json](transient_error.json) | Structured transient error data with fields `error`. |
| [unknown_ancillary.sse](unknown_ancillary.sse) | Recorded stream/input bytes for unknown ancillary. |
| [unknown_critical.sse](unknown_critical.sse) | Recorded stream/input bytes for unknown critical. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
