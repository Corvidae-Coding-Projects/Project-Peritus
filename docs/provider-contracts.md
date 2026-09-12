# Hosted provider contracts

Reviewed against official API documentation and upstream source on **2026-09-10**.
These are HTTP adapters. Peritus does not execute an SDK package named in remote metadata.

## Discovery and operation selection

| Service | Live model discovery | Inference contract |
| --- | --- | --- |
| OpenCode Zen | `GET https://opencode.ai/zen/v1/models` | Per-model Responses, Chat Completions, Anthropic Messages, or Google Generate Content |
| OpenCode Go | `GET https://opencode.ai/zen/go/v1/models` | Per-model protocol; exact Go gateway prefix |
| OpenRouter | `GET https://openrouter.ai/api/v1/models` | `POST https://openrouter.ai/api/v1/chat/completions` |
| Groq | `GET https://api.groq.com/openai/v1/models` | `POST https://api.groq.com/openai/v1/chat/completions` |
| Together AI | `GET https://api.together.ai/v1/models` | `POST https://api.together.ai/v1/chat/completions` |
| Fireworks AI | `GET https://api.fireworks.ai/v1/accounts/fireworks/models?pageSize=200` | `POST https://api.fireworks.ai/inference/v1/chat/completions` |
| DeepSeek | `GET https://api.deepseek.com/models` | `POST https://api.deepseek.com/chat/completions` |

Catalog requests use bearer authentication. OpenCode inference uses the selected protocol's
documented authentication: bearer for Responses/Chat, `x-api-key` for Messages, and
`x-goog-api-key` for Generate Content. There is no built-in model list or name-prefix routing.

OpenCode's model listing contains IDs without protocol metadata. Its own client obtains model
metadata from `https://models.opencode.ai/api.json`. Peritus fetches that source **without a
credential**, joins only IDs returned by the selected service, and interprets a closed set of
documented SDK protocol names. Remote package names never execute; remote URLs and headers never
control requests or credential destinations. Missing/unrecognized metadata stays unknown, and
setup offers explicit protocol selection. Switching to a different OpenCode model requires
freshly discovered protocol metadata or explicit provider setup; the previous model's protocol
is not silently reused.

Fireworks discovery lists its public serverless chat inventory. Private dedicated deployment
IDs can be entered explicitly. The management catalog exposes `supportsTools`, `contextLength`,
`conversationConfig`, and `supportsServerless`; pagination uses `nextPageToken`. Together returns
a top-level model array. OpenRouter's `supported_parameters` advertises tool support. Absent
capability fields mean unknown. Catalog success never establishes inference or billing access.

## Request and stream behavior

Named Chat routes project plain text messages as strings, preserve tool identities and results,
omit optional false/null schema fields, and use each service's documented output-token field.
Together and Fireworks explicitly reject context overflow instead of accepting server-side
truncation. Required tool-calling behavior is checked by the connection test. DeepSeek documents that
thinking mode rejects forced tool choices. Named Chat routes therefore offer only the required
tool with automatic wire choice and enforce the original required call in the stream decoder.
A missing or different call fails before acceptance; thinking is not disabled to make it pass.
The application's portable tool schemas contain optional fields and no longer unconditionally
request provider strict decoding. Host argument, permission, and grounding checks still run.
Google accepts the portable object schemas through its JSON-schema fields and returns each
function result with the original call's name and ID.

Streams accept empty initial content as a heartbeat. OpenRouter's content-free final usage
choice may repeat the preceding finish reason exactly once; it cannot introduce output or change
the finish. Chat-compatible usage snapshots remain cumulative while the stream is open; the last
snapshot becomes final only at the mapped `[DONE]` boundary. Counter regressions still fail closed.
OpenRouter's HTTP-200 error events remain failures, including an error as the first event. Groq's
`x_groq` accounting is retained. Named routes may resolve a requested model alias to a stable
returned model ID; the returned ID cannot change during the stream.

Documented reasoning fields are preserved as bounded provider-specific replay data, including
OpenRouter `reasoning_details` and DeepSeek/Fireworks `reasoning_content`. The developer loop keeps
that data in subsequent assistant/tool transcripts. No unsupported reasoning effort is inferred
from a model name. Unknown stream fields and inconsistent identities still fail explicitly.

## Connection qualification and evidence

Provider settings offer an explicit test using the same production configuration, registry,
adapters, and operating-system credential broker as real runs. It can make three small requests:
streamed generation, one harmless in-memory tool call, then a response consuming its result.
The tool has no filesystem, network, or application authority. A total 45-second deadline bounds
the test. The UI identifies the failing stage, preserves safe HTTP/diagnostic details, and keeps
configuration/discovery distinct from live qualification. Results are not saved as permanent
readiness claims. Error details remain visible in the normal conversation view.

Automated tests use synthetic, account-free fixtures derived from the documented shapes. They
are contract tests, not recordings of paid vendor sessions. A user's key, billing, deployment
permissions, and currently selected model require the explicit live connection check.

## Official sources

- OpenCode [Zen endpoints](https://opencode.ai/docs/zen/#endpoints),
  [Go endpoints](https://opencode.ai/docs/go/#endpoints),
  [catalog response source](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/util/modelsHandler.ts),
  [live metadata client](https://github.com/anomalyco/opencode/blob/dev/packages/core/src/models-dev.ts),
  [Google gateway handler](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/v1/models/%5Bmodel%5D.ts).
- OpenRouter [API reference](https://openrouter.ai/docs/api_reference/overview),
  [streaming and errors](https://openrouter.ai/docs/api_reference/streaming),
  [reasoning replay](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens).
- Groq [OpenAI compatibility](https://console.groq.com/docs/openai),
  [live models](https://console.groq.com/docs/models), [reasoning](https://console.groq.com/docs/reasoning).
- Together [compatibility](https://docs.together.ai/docs/inference/openai-compatibility),
  [Chat Completions schema](https://docs.together.ai/reference/chat-completions).
- Fireworks [compatibility](https://docs.fireworks.ai/tools-sdks/openai-compatibility),
  [model listing](https://docs.fireworks.ai/api-reference/list-models),
  [model metadata](https://docs.fireworks.ai/api-reference/get-model),
  [Chat Completions schema](https://docs.fireworks.ai/api-reference/post-chatcompletions),
  [tool calling](https://docs.fireworks.ai/guides/function-calling).
- DeepSeek [API setup](https://api-docs.deepseek.com/),
  [live models](https://api-docs.deepseek.com/api/list-models/),
  [Chat Completions and tool-choice restrictions](https://api-docs.deepseek.com/api/create-chat-completion/),
  [thinking and tool-result replay](https://api-docs.deepseek.com/guides/thinking_mode/).

Google's native JSON-schema fields and function-result identity are checked against
[Generate Content](https://ai.google.dev/api/generate-content). DeepSeek's
[strict-mode schema requirements](https://api-docs.deepseek.com/guides/tool_calls/) distinguish
provider strict decoding from ordinary optional-argument schemas.
