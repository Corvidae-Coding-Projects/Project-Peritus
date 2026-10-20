# OpenCode rejected-turn diagnostics — 2026-10-20

## Outcome and scope

On WSL2 Debian with OpenCode Go and `deepseek-v4.1-flash`, two user-visible problems appeared in the
same run:

1. The chat repeated `Requesting model deepseek-v4.1-flash · effort not requested` very often.
2. A run ended with `execute D0 developer loop: provider opencode ended the request
   (malformed_payload; opencode.runtime.invalid_turn_json) … the diagnostic identifies the rejected
   part of the reply`, and no command could show that diagnostic.

Both are real. The `Requesting model` line was emitted from `admit_role_request` on every bounded
attempt of one logical turn, so a turn that retried printed the same request line again. The
"diagnostic" did not exist as a readable artifact: the adapter's
`peritus.opencode_turn_correction` extension recorded only `{attempt, reason, tools_executed}`, and
nothing in the product could read a run's durable private trace.

The repair keeps the rejected reply text in the private trace, adds a local trace reader
(`peritus runs trace --run <ID> [--extensions]`), names that command in the failure hint, announces
a model request once per logical turn, and narrates each refusal and retry as its own transition.

## Confirmed defects

| Defect | Before | After / evidence |
| --- | --- | --- |
| The refused reply text was never retained. | Every one of the 94 correction records in the retained run carries only `{"attempt":N,"reason":"opencode.runtime.invalid_turn_json","tools_executed":false}`. | `TurnRejection::audit` now publishes `rejected`, `rejected_bytes`, and `rejected_truncated`, bounded to 16 KiB with a UTF-8-safe prefix. The scripted replay reads the exact refused bytes back. |
| The failure hint promised a diagnostic that could not be read. | `…the diagnostic identifies the rejected part of the reply.` with no command or file. | The hint now reads `Read the rejected reply with \`peritus runs trace --run <run-id>\`…`, and that command exists. |
| No local reader existed for a run's private trace. | The trace could only be inspected by reading the framed file by hand. | `peritus runs trace --run <ID>` decodes every record locally, needs no daemon session, and `--extensions` filters to provider extensions. |
| One logical turn narrated its model request once per attempt. | 71 `Requesting model` lines inside a 256-activity window that covers one run's final stretch. | `ModelStarted` is emitted only on the first attempt; retries report `Retrying model request (attempt N of M)` and each refusal reports `Provider reply rejected (<reason>)`. |

## Actual operation and results

### Retained real run

`/home/yria/.local/state/peritus/daemon/product-runs/dfdaf12468db88f9ad3e72e9d98f0f37.trace`
(1202 records, written by the pre-fix binary) and its workbench record were measured directly.

| Observation | Value |
| --- | --- |
| Trace records | 1202: 965 provider-envelope, 109 local-memory-observation, 99 local-memory-checkpoint, 29 retry-scheduled. |
| Host attempts (streams reaching a terminal outcome) | 82: 46 `response-completed`, 36 `response-failed` with `MalformedPayload opencode.runtime.invalid_turn_json`. |
| Refused replies recorded | 94: 58 first-attempt refusals (22 corrected into a valid turn, 36 followed by a second refusal) and 36 second-attempt refusals that ended the turn. |
| Envelope repairs (`peritus.response_healing`) | 39. |
| Retained workbench window | 256 of 331 sequenced activities: 71 `Requesting model`, 29 `Repaired model JSON`, 0 refusal or retry lines. |

All 94 pre-fix correction records lack the rejected text, which is the direct evidence that the old
hint's promise was false. The 29 `retry-scheduled` records show that bounded retries happened but
were never narrated.

### Private-trace reader

`target/debug/peritus runs trace --run dfdaf12468db88f9ad3e72e9d98f0f37` reads the retained real
trace as 1202 records and exits 0; `--extensions` narrows it to the 133 extension records, including
`peritus.response_healing` and `peritus.opencode_turn_correction`.

Framing was checked against copies of that real trace under a disposable `XDG_STATE_HOME`:

| Input | Observation |
| --- | --- |
| Complete trace | 1202 records, exit 0. |
| Same trace with its final byte removed | 1201 records, exit 0; the trailing incomplete record is ignored. |
| Valid first record followed by record tag 99 | `peritus: protocol: … unknown record tag 99 at byte 1039`, exit 14. |
| Empty file | 0 records, exit 0. |
| Unknown run identifier | `peritus: local-io: read the run private trace: … has no retained private trace`, exit 13. |
| Trace copied with mode `000` | `peritus: local-io: read the run private trace: …`, exit 13. |

### Scripted-vendor replay

`docs/testing/reproducers/opencode-rejected-turn-diagnostic/replay.py` builds the two consumers
offline and drives them against a scripted OpenCode stand-in. All four initial scenarios passed
with 0 failed checks:

| Scenario | Observation |
| --- | --- |
| `accepted` | One correction carrying the rejected text, then a succeeded turn. |
| `rejected` | Two corrections, then `TERMINAL failed MalformedPayload opencode.runtime.invalid_turn_json`. |
| `retry` | `ModelStarted` once, `ProviderRetry 2/3 retryable_provider_response`, succeeded turn. |
| `refused_turn` | `model_started=1 provider_retry=2 response_rejected=6`, and the terminal message names `peritus runs trace`. |

## Follow-up: prelude repair and run discovery

The same failure recurred after the first repair. With rejected text now retained, the new trace
reader showed that 29 refusals carried a reply. Fourteen contained one complete Peritus envelope
after an explanatory prelude; the old prelude rule refused all fourteen because the text included
inline code spans, quoted pseudo-tool markup, or one `json` fence. The remaining fifteen were
DSML-style tool-call markup without a complete Peritus envelope and remain correctly refused.

The private-envelope repair now accepts a blank-line-separated prelude containing inline code
spans or quoted prose, and one final Markdown-fenced object introduced by that prelude. It still
requires the object to be the sole structured candidate ending the payload. Preludes containing
container delimiters or additional fences, multiple candidates, trailing text, and unterminated
containers remain rejected. The scripted replay now covers `prelude_backticks`, `prelude_quoted`,
and `prelude_fenced`; all seven prelude-era scenarios pass.

Run discovery also changed:

| Action | Observed result |
| --- | --- |
| `peritus runs trace` with no `--run` | Lists retained traces newest first with their 32-hex identities, byte sizes, and paths. A disposable state root with two valid trace names and two decoys listed only the two valid traces. |
| `peritus runs trace --extensions` without `--run` | Usage error, exit 2; extensions only narrow a selected run. |
| `peritus runs trace --run <missing>` | Local-I/O error, exit 13, naming the absent trace. |
| `y` in the Runs dashboard | Copies the selected run's raw 32-hex identity, the value `--run` expects, rather than the displayed operation prefix. |

The TUI key is compiled and announced in the dashboard title and help. A live interactive copy was
not exercised because the running installed daemon uses different executable or provider settings
than this checkout, and the launcher correctly preserves it instead of restarting it.

## Follow-up: DeepSeek DSML and repeated refusals

The user reported that DeepSeek still often needed two `invalid_turn_json` refusals before a turn
succeeded. The retained trace now contained 93 correction records with their rejected text. Of
those, 70 carried a DeepSeek DSML calls block. Across those 70 replies there were 169 `invoke`
blocks: 79 used ordinary named parameters, 72 used the `arguments` object plus `name` form, and 18
were structurally malformed hybrids. The remaining refusals were JSON envelopes behind prose,
XML-like calls, or multiple candidates.

The request previously said only that the response must be one JSON object. It did not name the
model-native markup that DeepSeek was emitting, and the correction prompt likewise repeated the
generic requirement. The adapter now adds an explicit JSON-only tool-syntax instruction to the
wire prefix, the prompt tail, and the correction prompt. It names DSML, XML/HTML-like tags,
special-token markers, and pseudo-tool syntax as invalid and shows a normal JSON call with an
argument object.

A complete terminal DSML calls block is also normalized without another inference. The adapter
accepts a prose prelude followed by one `calls`/`tool_calls` block, parses standard parameters and
the observed `arguments`-plus-`name` form, converts nested arguments to the ordinary call shape,
and publishes the private `peritus.opencode_turn_normalization` audit. The existing checks still
gate declared tool names, object-valued arguments, maximum call count, and tool selection. The
parser requires the block to be unique and final, rejects fenced examples and incomplete tags, and
does not execute anything itself.

The replay now has ten scenarios. The DSML success case expects two normalized calls and
`TERMINAL requires_action`; the undeclared-tool case expects one `invalid_turn_tools` correction;
the malformed case expects one `invalid_turn_json` correction. The replay also captures the actual
submitted prompt and checks that `STRICT JSON TOOL SYNTAX`, `Never emit DSML`, and `tool_calls array`
reached the vendor process. All ten scenarios pass with 0 failed checks.

## Retained evidence

- [Reproducer](reproducers/opencode-rejected-turn-diagnostic/README.md) — consumers, scripted
  stand-in, and the ten scenarios.
- [Observed baseline](reproducers/opencode-rejected-turn-diagnostic/observed-baseline.txt) —
  verbatim transcripts of the recorded run and its check outcomes.

## Limits

- The replay's vendor is scripted: no network access, no credential use, and no vendor inference
  occur. It establishes the adapter retention contract, the trace reader, and the loop's narration,
  not model behavior.
- The replay needs a shell without the repository sandbox. It points `TMPDIR` at its own output
  directory, and a sandbox that mounts read-only paths under `/tmp` makes the adapter's
  ambient-project guard reject the temporary workspace before any vendor command runs.
- The retained real run was produced by the pre-fix binary, so it shows the absence of the rejected
  text and the repeated request lines. It cannot show the repaired narration; the scripted replay
  and the fresh trace reader cover that.
- The 16 KiB retention bound is exercised by construction, not by a real reply that large; the
  scripted replies report `rejected_truncated: false`.
- The extension name `peritus.opencode_turn_correction` is the adapter-owned private-trace
  contract. Only the OpenCode adapter publishes it; other adapters are unchanged.
