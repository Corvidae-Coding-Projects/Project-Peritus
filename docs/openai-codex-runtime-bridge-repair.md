# OpenAI Codex runtime bridge repair

During a local Peritus agent run, the saved OpenAI account was selected with
`gpt-5.6-sol`. The Codex subprocess sometimes emitted native tool activity or
competing final messages. Peritus rejected those responses with
`openai.codex_runtime.native_tool` or
`openai.codex_runtime.multiple_messages`, interrupting the run.

The local fix is in the [Codex process invocation](../crates/model/peritus-provider-openai/src/runtime/provider/invocation.rs):

- The [disabled native feature list](../crates/model/peritus-provider-openai/src/runtime/provider/invocation.rs#L26)
  now includes `sleep_tool` and `tool_suggest`.
- The [subprocess arguments](../crates/model/peritus-provider-openai/src/runtime/provider/invocation.rs#L120)
  disable the native plan tool, experimental user input, and web search.

These settings keep tool proposals on Peritus's agent-to-agent host interface.
The [response decoder](../crates/model/peritus-provider-openai/src/runtime/output.rs)
still rejects native execution; this change does not relax its checks. The
[invocation test](../crates/model/peritus-provider-openai/src/runtime/provider/invocation/tests.rs)
checks every disabled feature and explicit tool setting, alongside the selected model effort.

Verification reported by the original local repair session:

- `cargo test -p peritus-provider-openai --lib`: 31 passed.
- Focused provider Clippy with warnings denied, formatting, and diff checks passed.
- The release daemon rebuilt, and the installed daemon binary matched the rebuilt binary.
- The supervised live run continued without further bridge rejection observations
  after the rebuilt daemon started. This is a run observation, not a general
  guarantee for every Codex response shape.

The source fix and this repair record are included in the omnibus bugfix branch. The saved
account configuration and login were retained during the original rebuild. No credential
value is recorded here. The omnibus validation reruns the provider tests and keeps the decoder's
native-tool and competing-message rejection tests intact; it does not repeat the live paid run.
