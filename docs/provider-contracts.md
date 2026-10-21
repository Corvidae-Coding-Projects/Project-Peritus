# Hosted provider contracts

HTTP contracts reviewed against official documentation and upstream source on **2026-09-10**.
Zen and Go now use the official OpenCode 1.19.0 executable contract, exercised on **2026-10-13**.
Other hosted services continue through the reviewed HTTP adapters.

## Model token capacities

Generated named hosted routes set `discover_model_limits = true`. Their profile token numbers
are fallbacks for missing metadata, rather than ceilings imposed on every selected model.
`ModelProvider::prepare_context` loads the selected route's catalog before role projection and
developer turns. `request_profile(requested_output_tokens)` then resolves the exact model's
numeric limits for prompt assembly, local and legacy compaction, negotiation, and generation.
Capabilities, route identity, non-token limits, and lifecycle guarantees remain configured.
Changing the selected model reuses cached catalog entries without copying another model's limits.

Input-only and output-only ceilings are distinct from a shared input-plus-output context window.
When the catalog supplies a shared window, an explicit output allowance reserves only that
allowance, bounded by the model's output maximum. Uncapped developer turns select input against
the shared window with minimum reply space, then fit output to the smaller of the model maximum
and the space left after input assembly, including tool framing. They do not reserve the entire
maximum output on every turn or request unlimited provider output. OpenCode's output allowance
remains advisory. The existing compaction trigger still leaves headroom during ordinary growth.

Set `discover_model_limits = false` on a named hosted route to retain fixed configured token
limits. For existing configurations without this key, the old generated 32,768/4,096 pair opts
into discovery; other custom pairs stay fixed. Explicit compatible endpoints and account routes
keep their configured behavior. Hosted HTTP catalog failures retain fallbacks without retrying
discovery on every tool turn; an explicit catalog refresh can update them. OpenCode still requires
a successful vendor catalog because exact model admission already depends on it.

## Discovery and operation selection

| Service | Live model discovery | Inference contract |
| --- | --- | --- |
| OpenCode Zen | `opencode models opencode --verbose` | Official CLI chooses SDK, operation, authentication, and gateway headers |
| OpenCode Go | `opencode models opencode-go --verbose` | Official CLI chooses SDK and Go gateway; Go subscription required |
| OpenRouter | `GET https://openrouter.ai/api/v1/models` | `POST https://openrouter.ai/api/v1/chat/completions` |
| Groq | `GET https://api.groq.com/openai/v1/models` | `POST https://api.groq.com/openai/v1/chat/completions` |
| Together AI | `GET https://api.together.ai/v1/models` | `POST https://api.together.ai/v1/chat/completions` |
| Fireworks AI | `GET https://api.fireworks.ai/v1/accounts/fireworks/models?pageSize=200` | `POST https://api.fireworks.ai/inference/v1/chat/completions` |
| DeepSeek | `GET https://api.deepseek.com/models` | `POST https://api.deepseek.com/chat/completions` |

HTTP catalog requests use bearer authentication. There is no Peritus fallback model list.
Zen and Go discovery delegates to the vendor CLI and reads its advertised model metadata;
setup no longer asks users to choose the underlying API protocol. Existing saved Zen/Go
protocol selections keep their model and opaque credential reference and migrate to
`wire_protocol = "opencode-runtime"`.

Each OpenCode inference uses `run --pure --agent peritus-router --format json` in a freshly
created private vendor configuration directory, with PWD and XDG configuration/data/cache/state
paths isolated. The vendor home override and external-skill flag isolate home integrations;
child temporary paths are also private. Host HOME remains unchanged.
Peritus supplies the OS-stored API key to a disposable 0600 auth file. Native permissions
are denied globally and for the private primary agent; native tools, MCP, LSP, formatters,
sharing, updates, automatic compaction and default plugins are disabled. The CLI receives an
explicit title and a two-step ceiling, avoiding the native final-step prompt on the first
inference. A successful turn still performs one inference and zero native tools. Peritus replays the complete transcript and inert
tool catalog, validates returned tool names/counts/arguments, executes host tools itself,
and replays their real results on the next invocation. The private result envelope accepts
public text as a string or a task JSON report as an object, serialized once by Peritus. Explicit
GPT commentary parts are separated from the final result. Native tool events, errors, nonzero
exits and incomplete terminals never become successful turns. Cancellation owns the child
process group and disposable state. Provider rejection bodies remain private; safe status
and recognized diagnostics distinguish subscription, model-access, authentication, quota and
context failures. Recognized authentication and permission errors abort the official client's
incorrect retries immediately; other native retries remain bounded by the process deadline.

The private global-config directory and ancestor guard are part of the isolation contract:
in OpenCode 1.19.0 the flags
alone did not suppress a deliberately planted local project plugin during independent replay.
Peritus rejects temporary storage beneath project/config ancestors before writing a key or
launching vendor code, and never reuses vendor session directories. The tested CLI version is
required because this isolation depends on its actual configuration semantics. Supported-version
discovery skips incompatible PATH candidates. Executable resolution is deferred until the
OpenCode route is used, so an unavailable secondary Zen/Go route cannot block other providers.
Text and host tool history are supported; image input, explicit reasoning controls and native
session continuation are not advertised. Output-token limits are advisory. Native JSONL
results are buffered, then emitted through the Peritus event stream. Initial reviewer context
uses the selected model's advertised input allowance after native catalog discovery; that
metadata is a projection hint, not a promise about live quotas or accepted request sizes.

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
A missing or different call fails before acceptance as a bounded safe-new-request provider
failure. Developer-loop retries restate the exact prerequisite so a weaker model can correct the
miss without accepting terminal text or disabling thinking.
The application's portable tool schemas contain optional fields and no longer unconditionally
request provider strict decoding. Host argument, permission, and grounding checks still run.
Google accepts the portable object schemas through its JSON-schema fields and returns each
function result with the original call's name and ID.

Streams accept empty initial content as a heartbeat. Chat deltas may omit the optional role
or set it to null; a non-null role must still be `assistant`. A null `tool_calls` field means
no calls and does not require tool capability; non-null values still require valid tool arrays.
OpenCode's Chat compatibility gateway may serialize several sequential fragments for choice zero
in one `choices` array, including a tool delta followed by its finish marker. Zen and Go consume
those entries in order as one completion; any nonzero choice index, output after a finish, or the
same shape from another hosted service still fails closed.
OpenRouter's content-free final usage
choice may repeat the preceding finish reason exactly once; it cannot introduce output or change
the finish. Chat-compatible usage snapshots remain cumulative while the stream is open; the last
snapshot becomes final only at the mapped `[DONE]` boundary. Counter regressions still fail closed.
The developer loop negotiates usage reporting when the profile supports it, just as the setup
canary does. Chat wire requests set `stream_options.include_usage` from that negotiation; they
must not request usage that their own stream decoder would reject.
OpenRouter's HTTP-200 error events remain failures, including an error as the first event. Groq's
`x_groq` accounting is retained. Named routes may resolve a requested model alias to a stable
returned model ID; the returned ID cannot change during the stream.

Documented reasoning fields are preserved as bounded provider-specific replay data, including
OpenRouter `reasoning_details` and DeepSeek/Fireworks `reasoning_content`. The developer loop keeps
that data in subsequent assistant/tool transcripts. No unsupported reasoning effort is inferred
from a model name. Unknown stream fields and inconsistent identities still fail explicitly.
Local context archives preserve bounded assistant reasoning with the exact transcript and tool
exchange, including recovery. Opaque replay is not promoted to user instructions or derived
working entries, and it is not replayed into a new host invocation. Other opaque provider
extensions remain rejected by local memory.

## Connection qualification and evidence

Provider settings offer an explicit test using the same production configuration, registry,
adapters, and operating-system credential broker as real runs. It can make three small requests:
streamed generation, one harmless in-memory tool call, then a response consuming its result.
The tool has no filesystem, network, or application authority. A total 45-second deadline bounds
the test. The UI identifies the failing stage, preserves safe HTTP/diagnostic details, and keeps
configuration/discovery distinct from live qualification. Results are not saved as permanent
readiness claims. Error details remain visible in the normal conversation view.

Provider setup, connection tests, and daemon requests open the same persisted OS credential
namespace. Opaque references do not encode a namespace, so a separate daemon namespace cannot
resolve credentials saved by setup.

Most automated tests use synthetic, account-free fixtures derived from the documented shapes.
The `zen-deepseek-live-{1,2,3}.sse` fixtures are sanitized recordings of the bounded connection
test against OpenCode Zen / `deepseek-v4-flash` on **2026-09-12**. Response IDs, tool-call IDs,
and timestamps are replaced; the synthetic canary content, nullable fields, reasoning replay,
tool-argument fragments, cumulative usage, and terminal ordering are preserved. No headers or
credentials were recorded. Their offline replay exercises all three connection stages through
the production decoder and reducer. The same live test passed after allowing null tool-delta
roles. This establishes that model/route at that time, not every provider or model. A user's
key, billing, deployment permissions, and currently selected model still require a live check.
The ordinary interactive daemon chat was also verified to complete after aligning usage
negotiation, accepting null tool-call fields, and retaining assistant reasoning in local context.
The final installed build completed a read-only `workspace_list` call through the interactive
app, consumed its result with normal local-context replay, and returned the requested test token.

## Official sources

- OpenCode [Zen endpoints](https://opencode.ai/docs/zen/#endpoints),
  [Go endpoints](https://opencode.ai/docs/go/#endpoints),
  [catalog response source](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/util/modelsHandler.ts),
  [Chat compatibility stream conversion](https://github.com/anomalyco/opencode/blob/dev/packages/console/app/src/routes/zen/util/provider/openai-compatible.ts),
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
