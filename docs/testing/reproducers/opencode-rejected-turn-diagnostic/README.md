# OpenCode rejected-turn diagnostic replay

## Folder guide

Real-interface replay resources for the OpenCode adapter's refusal of a model reply and for the
developer loop's public narration of that refusal; start here and use [AGENTS.md](AGENTS.md) for
file-level responsibilities.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder. |
| [AGENTS.md](AGENTS.md) | Local navigation map and file responsibilities. |
| [consumer.rs](consumer.rs) | Provider-level public-library consumer; asserts the retained rejected text and the terminal outcome. |
| [agent.rs](agent.rs) | Developer-loop consumer; records the public activities the loop publishes for one refused turn. |
| [scripted-opencode.py](scripted-opencode.py) | Scripted OpenCode stand-in answering `--version`, `models`, and `run` from a fixed reply script. |
| [replay.py](replay.py) | Builds both consumers and runs the fifteen scenarios with disposable state. |
| [observed-baseline.txt](observed-baseline.txt) | Verbatim transcripts of the recorded run and its check outcomes. |

## What this case shows

A refused OpenCode reply used to leave no readable evidence. The adapter recorded only
`{attempt, reason, tools_executed}`, while the user-facing failure hint claimed "the diagnostic
identifies the rejected part of the reply" — and no command could read a run's private trace.

The same run also narrated one model request per bounded attempt, so a single logical turn that
retried three times printed `Requesting model …` four times.

The repaired behavior:

- The adapter's `peritus.opencode_turn_correction` extension retains the rejected reply text
  (`rejected`, `rejected_bytes`, `rejected_truncated`), bounded to 16 KiB.
- `peritus runs trace --run <ID> [--extensions]` reads a run's durable private trace locally,
  without a daemon session, and the failure hint names that command. Omitting `--run` lists
  retained run identities newest first for terminal-side discovery.
- The private-envelope repair accepts an explanatory prelude containing inline code spans or
  quoted pseudo-tool markup, and a single final object wrapped in a Markdown fence, while still
  rejecting ambiguous candidates.
- Long planning replies recover literal newlines, tabs, and other C0 controls inside JSON strings,
  a content-only envelope with the optional empty `tool_calls` omitted, and the exact duplicated
  trailing `,"tool_calls":[]}` fragment. An unenveloped reply is accepted as plain content only
  when it contains no JSON container opener, `tool_calls` field, angle-bracket tag, or DSML
  marker; that normalization records `plain-text-v1` and never executes a tool.
- The request and correction instructions explicitly forbid DSML, XML, HTML-like tags, and
  special-token pseudo-tool syntax. A complete terminal DSML calls block is also converted under
  `peritus.opencode_turn_normalization` into ordinary inert tool calls, with the normal declared
  name, argument-object, count, and selection checks still applied. Incomplete or ambiguous DSML
  is refused rather than partially interpreted.
- The loop announces its model request once per logical turn and reports each refusal and retry
  as its own transition.

## Prerequisites

- A sandbox-free shell. The replay points `TMPDIR` at its own output directory; a sandbox that
  mounts read-only paths under `/tmp` makes the adapter's ambient-project guard reject the
  temporary workspace before any vendor command runs.
- A Rust toolchain with the checkout's cached dependencies; the replay builds two public-library
  consumers offline into the checkout `target` directory.

## Replay

```sh
python3 docs/testing/reproducers/opencode-rejected-turn-diagnostic/replay.py \
    --output /tmp/peritus-rejected-turn
```

The output directory must not exist. Each scenario writes its transcript to
`<output>/transcript-<scenario>.txt`; `result.json` records the selected scenarios, the checkout
commit, and any failed check. The script exits non-zero if any check fails.

## Expectations

| Scenario | Consumer | Scripted replies | Expected |
| --- | --- | --- | --- |
| `accepted` | provider | `invalid`, `valid` | One correction carrying the rejected text, then a succeeded turn. |
| `rejected` | provider | `invalid`, `invalid` | Two corrections, then a terminal `malformed_payload; opencode.runtime.invalid_turn_json` failure. |
| `retry` | agent | `rate_limit`, `valid` | `ModelStarted` once, one `ProviderRetry 2/3 retryable_provider_response`, succeeded turn. |
| `refused_turn` | agent | `invalid` | `ModelStarted` once, two retries, six refusals across three attempts, terminal error naming `peritus runs trace`. |
| `raw_newlines` | provider | `raw_newlines` | One `private-envelope-v1` repair for literal control characters, no refusal, succeeded turn with newline and tab content preserved. |
| `missing_tool_calls` | provider | `missing_tool_calls` | The omitted empty array defaults to no calls, no refusal or repair, succeeded turn. |
| `trailing_tool_calls_fragment` | provider | `trailing_tool_calls_fragment` | One `private-envelope-v1` repair rejoining the exact trailing empty field, no refusal, succeeded turn. |
| `plain_prose` | provider | `plain_prose` | One `plain-text-v1` normalization, no refusal or JSON repair, succeeded turn with the prose preserved. |
| `plain_tool_markup` | provider | `plain_tool_markup`, `valid` | Unenveloped pseudo-tool prose is still refused as invalid JSON, then the corrected turn succeeds. |
| `prelude_backticks` | provider | `prelude_backticks` | One `private-envelope-v1` repair, no refusal, succeeded turn. |
| `prelude_quoted` | provider | `prelude_quoted` | One `private-envelope-v1` repair, no refusal, succeeded turn. |
| `prelude_fenced` | provider | `prelude_fenced` | One `private-envelope-v1` repair, no refusal, succeeded turn. |
| `dsml` | provider | `dsml` | One `dsml-v1` normalization for two declared calls, no refusal, `requires_action`. |
| `dsml_unknown_tool` | provider | `dsml_unknown_tool`, `valid` | One refusal naming the undeclared tool, then a succeeded turn. |
| `dsml_malformed` | provider | `dsml_malformed`, `valid` | One JSON refusal, then a succeeded turn. |

## Limits

- The vendor is scripted. No network access, no credential use, and no vendor inference occur;
  the replay establishes the adapter retention contract and the loop's public narration only.
- The four long-planning shapes are derived from the rejected DeepSeek replies retained in a real
  run trace; they reproduce the same malformed shapes without retaining that run's planning text.
- The `rejected` text asserted here is the scripted stand-in's own reply. It shows that the exact
  refused bytes are retained, not that any particular model produces this defect.
- The `dsml` scenario also captures the submitted prompt and asserts that the explicit JSON-only
  tool-syntax instruction reached the vendor process.
- The extension name `peritus.opencode_turn_correction` is the adapter-owned private-trace
  contract; the replay asserts it directly rather than through a rendered terminal.
