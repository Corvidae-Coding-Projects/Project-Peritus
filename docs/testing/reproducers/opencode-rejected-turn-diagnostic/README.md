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
| [replay.py](replay.py) | Builds both consumers and runs the four scenarios with disposable state. |
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
  without a daemon session, and the failure hint names that command.
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

## Limits

- The vendor is scripted. No network access, no credential use, and no vendor inference occur;
  the replay establishes the adapter retention contract and the loop's public narration only.
- The `rejected` text asserted here is the scripted stand-in's own reply. It shows that the exact
  refused bytes are retained, not that any particular model produces this defect.
- The extension name `peritus.opencode_turn_correction` is the adapter-owned private-trace
  contract; the replay asserts it directly rather than through a rendered terminal.
