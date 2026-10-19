# Codex notification decoding and builder recovery — 2026-10-18

## Observed failure and cause

The builder-owned PRESCIENT run stopped with
`malformed_payload; openai.codex_runtime.unsupported_event`. The daemon remained
alive. The stopped record retained its work and a 115,527,433-byte resume artifact.
The builder installation was still version 0.0.9; this repair installs the current
0.1.1 development build with the changes below.

An actual request through the same official Codex 0.163.0 executable reproduced
two distinct conditions:

1. Codex emitted an `item.completed` item of type `error` warning that model
   metadata for `gpt-6.1-sol` was missing. Codex's `ErrorItem` is a nonfatal item;
   it is distinct from a top-level `error` or `turn.failed` event.
2. The service then rejected that selected model for the ChatGPT account. At the
   time of the replay, the account advertised and successfully served
   `gpt-6-luna`. This is an account/time-specific availability observation, not a
   statement that the selected model is unavailable to every account or why its
   availability changed.

Peritus's decoder rejected the advisory item before reading the real terminal
error. That produced misleading protocol guidance and repeated the same rejected
request. The original decoder reproduces `UnsupportedEvent` on the retained
actual stdout. The corrected decoder reports `ModelUnavailable` on those bytes.

## Repair

- Recognize the documented completed advisory-error item, validate its message,
  and continue to the authoritative terminal response. A warning alone cannot
  complete a response.
- Unwrap one level of a service error encoded inside the executable's message
  string, and recognize explicit model-access failures. Preserve structured error
  codes ahead of generic error types.
- Classify an unavailable model as a non-retryable invalid request with
  `openai.codex_runtime.model_unavailable`.
- Present `/model` recovery guidance and explain that saved work is retained.
- Keep unknown-item rejection, native-tool rejection, final-message binding,
  JSON/envelope validation, and all other provider adapters intact.

The production changes are confined to the Codex output decoder, its error
classification/projection, and one exact-diagnostic hint in the developer loop.
No provider credentials or user model preferences were changed.

## Real operation evidence

The [structured receipts](codex-notifications-evidence.json) bind the exact source
files, tested/deployed CLI and daemon bytes, real provider observations, and
reattachment result. No unit tests, mock providers, formal verification, or
repository qualification suites were run.

| Operation | Observed result |
| --- | --- |
| Replay actual captured stdout against the original decoder at `87dedeaa80b3e286970ac38e12a22ebc35446266` | Reproduced `UnsupportedEvent`. |
| Replay the same stdout against the corrected production decoder | Correctly identified `ModelUnavailable`. |
| Fifteen actual-decoder input sequences | Passed real success/rejection captures plus advisory placement, repeated notices, missing terminal, forbidden native-command item, unknown item, malformed/control-bearing notice, safety failure, and nested model-error code cases. The varied transcripts are protocol inputs, not fifteen separate live-provider runs. |
| Public `CodexRuntimeProvider` → `DeveloperLoop`, actual authenticated executable and available model | Returned exactly `PERITUS-PROVIDER-READY`, with no tools and zero retries. |
| Same public path, actual rejected model | Returned the model-unavailable diagnostic and actionable recovery hint, with zero retries. |
| Final CLI → isolated daemon → actual Codex, operated through a PTY as builder | Available-model reply completed with one model request, zero tools, and zero retries; client exited normally and the disposable daemon shut down cleanly. |
| Same final CLI/TUI/daemon path with the rejected model | Stopped with the specific model-access diagnosis after one request and zero retries; saved state remained readable and disposable shutdown was clean. |
| Actual builder installation | CLI, daemon, TUI, and sandbox helper replaced atomically with builder-owned files; installed hashes read back against the final artifacts. |
| Original run reattachment using the installed client and restarted installed daemon | Connected successfully to the original run, retaining its stopped phase, 8,532 tool calls, and complete referenced resume artifact. No new implementation request was submitted. |

Builds used the pinned toolchain, two Cargo jobs, and systemd resource scopes.
The main build used `MemoryHigh=6G`, `MemoryMax=8G`, and `CPUQuota=400%`; its
observed sampled peak was approximately 3.8 GiB. These were build-process resource
controls, not new limits on the user's agent execution. The final program build
and separate sandbox-helper build succeeded.

Early disposable setup attempts used an empty automation-data directory and an
incorrect workspace-selection fixture shape. They were corrected before the
successful final-binary journeys: the final fixtures use the installed automation
runtime, isolated configuration/state, a valid workspace-selection record, and
the builder account's existing official-client login. Those setup mistakes were
not counted as product regressions or passing checks.

## Deployment and retained state

The old daemon shut down cleanly through the public CLI. A coherent post-exit
reflink snapshot of its daemon state, product state, and configuration is retained
under the builder account's private repair directory, together with the previous
binaries and installed-file hashes. The first snapshot attempt raced final
WAL/instance cleanup; it was replaced after confirming process exit. The final
snapshot completed successfully.

The fixed daemon is running and the installed client was exercised against it.
The original stopped task was not resumed automatically: changing its unavailable
model is a separate user choice, requested during the repair. A successful small
live request with the available model proves that route works; it does not prove
completion of the entire PRESCIENT task or establish restored access to the
original model. Existing open client processes load a new client binary when
reopened.

## Replay

See [the reproducer guide](reproducers/codex-notifications.md). It retains the
actual failing transcript, source-version-selectable decoder consumer, actual
authenticated provider consumer, and concrete varied-input generator. Consumer
builds use distinct identities to avoid confusing cached baseline and fixed
executables. None changes Cargo workspace membership or substitutes an
implementation for the application under test.
