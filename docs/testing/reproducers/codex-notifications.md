# Codex nonfatal notifications and model rejection

The builder run stopped with `openai.codex_runtime.unsupported_event`. Its official
Codex executable emitted a documented nonfatal `item.completed` / `error` item
about missing model metadata before the actual terminal response. Peritus stopped
decoding at that item and hid the service's model-access rejection.

`codex-model-unavailable.jsonl` retains the actual official executable output from
2026-10-18. It contains only a thread identifier, model-metadata notification, and
service rejection; no credentials, prompt body, or project source is included.
The executable was Codex 0.163.0, and the account rejected `gpt-6.1-sol` while
advertising and successfully serving `gpt-6-luna`. Availability is account/time
specific; do not assume those choices remain current on another installation.

Expected behavior comes from Codex's `ThreadItemDetails::Error` contract (nonfatal
item), its actual error/turn terminal, and Peritus's tool-free transport boundary:

- A valid advisory item does not prevent an otherwise complete valid response.
- An advisory item cannot create a successful terminal response by itself.
- The real model rejection is `ModelUnavailable`, with an actionable `/model`
  hint and no pointless replay of the same rejected request.
- Native-tool items, unknown item kinds, malformed notifications, and invalid
  lifecycle/envelope data continue to reject the response.

These are ordinary CLI consumers and actual provider requests, not Cargo test
targets, mocked providers, or replacement Codex executables. The decoder consumer
compiles the production decoder source directly for captured-wire replay. Its
`live` mode separately exercises the public `CodexRuntimeProvider` and
`DeveloperLoop` using the actual authenticated official executable.

Build and replay from the repository root:

```sh
python3 docs/testing/reproducers/build-codex-notification-consumer.py target/codex-notifications
target/codex-notifications/consumer replay docs/testing/reproducers/codex-model-unavailable.jsonl ModelUnavailable
```

To demonstrate the original decoder's failure, build with
`--baseline 87dedeaa80b3e286970ac38e12a22ebc35446266` into a separate output crate
and replay the same file expecting `UnsupportedEvent`. The baseline option
changes only the decoder source loaded for captured-wire replay; live mode always
uses the current public provider. Rebuild without `--baseline` before checking
the fix. The helper does not edit workspace membership or dependencies.

For live operation, run as the account that owns the official Codex login:

```text
peritus-real-codex-notifications live /absolute/path/to/official/codex AVAILABLE_MODEL /tmp/new-live-success complete
peritus-real-codex-notifications live /absolute/path/to/official/codex REJECTED_MODEL /tmp/new-live-rejection model_unavailable
```

Both requests authorize no host/native tools. Verify current account model
availability before choosing values. The generated receipt records the actual
outcome, diagnostic, and retry count without disclosing credentials.

Given a real successful `codex exec --json` capture and its output-last-message
file, run varied placements and failure boundaries:

```text
python3 docs/testing/reproducers/codex-notification-sequences.py /absolute/path/to/consumer success.jsonl final.json /tmp/new-notification-sequences
```

Each generated consumer has a distinct Cargo package/binary identity and an
output-local `consumer` copy, so rebuilding the original decoder cannot cause a
cached original executable to be mistaken for the fixed consumer.

The driver retains each concrete input and an independent expected disposition.
It does not execute a command described inside a deliberately forbidden native
tool item. Full TUI/daemon replay and deployment observations are recorded in the
[campaign document](../codex-notifications-real-2026-10-18.md).
