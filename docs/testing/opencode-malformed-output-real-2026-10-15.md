# OpenCode malformed turns — issue 127, 2026-10-15

## Outcome and scope

[Issue 127](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues/127)
was reproduced using actual DeepSeek inference through the official OpenCode runtime.
Two identical file-reading workflows completed; the third stopped after a successful
vendor execution because its model-authored private JSON envelope had an extra closing
brace. This was a real model-format failure at Peritus's adapter boundary, not evidence
that the user's files or selected HTTP protocol were wrong.

The adapter now distinguishes completed inference with an invalid model envelope from
invalid native framing, a failed vendor execution, or an access/configuration rejection.
Only the first class gets one new inference with a fixed corrective instruction. Rejected
tool proposals never reach host execution. Successful corrections account for both vendor
inferences and carry a fixed, private diagnostic event. Persistent format errors identify
the rejected part and permit ordinary bounded request recovery.

Valid tool-only replies can have empty public commentary. They produce real normalized
tool-call items without a fabricated text item. Tool names, selection/count limits,
argument object structure, host argument validation, and permissions remain enforced.

Only the OpenCode adapter's behavior changed. Shared developer error presentation gained
branches matching explicit OpenCode diagnostic codes; other provider paths are unchanged.
No live personal daemon, credentials, workspace settings, or account Privacy setting was
changed by this issue campaign. The earlier authorized second-window deployment is recorded
in [its separate report](second-instance-real-2026-10-15.md).

## Confirmed defects

| Defect | Before | After / evidence |
| --- | --- | --- |
| A completed model-format mistake aborts the workflow without an adapter-level correction. | Third ordinary DeepSeek workflow failed with `opencode.runtime.malformed_output`; the vendor frames themselves were complete and ended normally. | Two further naturally malformed replies were corrected through actual fresh inference. Both workflows completed and retained their exact host-read evidence. |
| Tool-only replies are rejected solely for empty commentary. | The transport schema required a nonempty string and the decoder rejected empty content even with valid tools. | Seven real tool-only replies were accepted in the varied matrix; no empty assistant message was manufactured. |
| Go regional Privacy rejection is described as a generic invalid API request. | Actual Go/DeepSeek HTTP 400 said the model required Global regions; Peritus reported `opencode.runtime.invalid_request`. | Fixed diagnostic `opencode.runtime.go_global_regions_required`, early recognition of the specific SDK configuration error, and guidance to choose an allowed model or explicitly change the account setting. Peritus does not change it automatically. |

The [saved original response](reproducers/opencode-malformed-output/observed-invalid-turn.jsonl)
contains the concrete extra-brace failure. It is retained as evidence and never injected
as a substitute response in a passing service replay.

## Actual operation and results

The primary independent contract was established in
`/home/doll/Peritus-Issue-127-2026-10-15/REAL-TESTING.md` before the final generated runs.
Every library workflow had to read both ordinary fixture files and echo a fresh token
that was unavailable before the host result. Report variants also had to preserve the
exact quoted Windows path and Unicode label as parseable public JSON. Native vendor tool
events were forbidden. Only normalized, accepted proposals reached the real file reader.

| Interface / sequence | Inputs and independent expectation | Observation |
| --- | --- | --- |
| Baseline real provider consumer | Three ordinary read/plan workflows | Two completed, third failed on actual malformed model JSON. |
| Varied DeepSeek and GPT-6 Luna matrix | Eight workflows per model; prose plans, empty commentary, public JSON, Windows paths, Unicode, fresh file tokens | 16/16 completed; 32 logical turns, 34 completed vendor inferences; two real corrections and seven tool-only replies; zero native tool events. |
| Claude Haiku 5.5 | Four varied workflows covering the same response forms | 4/4 completed. A preceding access probe also completed one ordinary workflow; prior assumptions that this Zen model was disabled were outdated. |
| Final portable replay | Three DeepSeek workflows using the checked-in consumer/runner and final source | 3/3 completed with exact host-read and report values. |
| Provider settings TUI | Select Zen, enter the authorized key through hidden normal input, discover/select DeepSeek, test the connection | Generation, tool calling, and tool-result round trip passed. |
| Actual planning TUI | Three read-only exchanges, each with a newly generated canary file | 3/3 completed with exact fresh-token readback; owned daemon shut down cleanly afterward. |
| Cancellation | Cancel a real owned inference, then issue an ordinary request | Cancellation completed; subsequent real answer was exactly `46`. |
| Mixed registry / actual Codex | Missing secondary OpenCode executable must not prevent Codex generation and host-tool round trip | Passed with current adapter changes; parent environment unchanged. |
| Go / DeepSeek with supplied key | Preserve the vendor's regional-privacy rejection; never reinterpret it as malformed model output | Final portable consumer returned `InvalidRequest / opencode.runtime.go_global_regions_required`; no format correction attempted. Successful Go inference remains blocked by account configuration. |
| Go / GLM with supplied key | Preserve subscription rejection | Vendor returned HTTP 403 requiring an active Go subscription; success is blocked by entitlement. |
| Final Go / GLM replay | Subscription rejection must not enter format correction | `Permission / opencode.runtime.go_subscription_required`; no correction attempted. |
| Final invalid-key replay | Invalid credentials must remain authentication failures | `Authentication / opencode.runtime.authentication`; no correction attempted. |
| Go diagnostic TUI readback | Reopen the exact stopped conversation in a fresh client | Specific regional-Privacy guidance was visible and persisted across restart/reconnection; no new inference was requested by readback. |

The two successful correction sequences declared 3,645 input / 1,066 output tokens and
3,620 input / 903 output tokens across their respective original-plus-correction pairs.
Independent parsing of the actual vendor counters matched the normalized totals exactly
(`correction-accounting.json`). Missing vendor counters remain unknown; no zero accounting
or duplicated host read was substituted.

Final-source rejection/diagnostic replays are recorded in `portable-go-subscription`,
`portable-invalid-key`, and `product-go-fixture`. Their nonzero consumer exits are the
expected account-rejection outcomes, not successful inference. The terminal diagnostic
readback completed successfully. `portable-cancel` also passed cancellation and recovery
on the final source. `final-mixed-provider.log` records the final-source Codex replay.

## Reproduction and provenance

Use the [portable replay](reproducers/opencode-malformed-output/README.md). It constructs
the production provider registry and sends real requests through the public model API.
Its transparent CLI observer relays unchanged vendor stdout/stderr; it does not implement
a provider or fake any tool result. The host reader really opens the owned fixture files.
The explicit key file is read locally; no key is included in source, argv, or captures.

Source starts from `332845d2b`; official runtime is OpenCode 1.19.0 on Linux x86-64.
The pinned [official CLI implementation](https://github.com/anomalyco/opencode/blob/v1.19.0/packages/opencode/src/cli/cmd/run.ts)
defines the JSON event stream. Peritus retains its strict native session/message and
completion checks around those events. No vendor version, provider selection policy,
other adapter implementation, or global credential environment was changed.

All owned fixture daemons were shut down. Three temporary provider credential entries
created through fixture onboarding were removed through the production credential API,
after checking that none overlapped the live installation's credential references. The
authorized original key file and existing credentials were preserved.

This issue's changes are prepared for develop; the personal daemon remains on the earlier
second-window fix so its resumed browser run can continue without another update/restart.

Builds used two Cargo jobs. The actual CLI/daemon build command was
`cargo build --locked -p peritus-cli -p peritus-daemon -j 2`. The external public consumers
were built against current path dependencies. The checked-in portable runner also built
its generated consumer from the checkout's cached dependency lock. No unit, mock, formal,
repository verification, or qualification suites were created or run.

Local evidence root: `/home/doll/Peritus-Issue-127-2026-10-15`:

- `peritus-log.txt`: issue-supplied transcript, kept separate from generated evidence.
- `baseline-deepseek-v4.1-flash.jsonl` and `captures/4164229.stdout`: pre-fix failure.
- `final-consumer-results.json`, `final-*.jsonl`, and `final-*.log`: varied service matrix.
- `final-consumer-capture-index.json`: exact 34 completed vendor captures for that matrix.
- `correction-accounting.json`: independent correction-pair accounting.
- `haiku-varied.log`, `cancellation.log`, and `mixed-provider.log`: adjacent actual behaviors.
- `product-fixture-final/`: provider-settings transcript, terminal screens, run records,
  independent result, and clean shutdown response.
- `portable-zen/`: actual replay of the portable consumer against final source.

## Limits and harness corrections

Model inference is nondeterministic. Saved requests and the observed invalid response
preserve the failing sequence, but a replay is not guaranteed to reproduce the identical
format mistake. Two naturally occurring errors on the changed adapter establish actual
correction behavior; ordinary successful replies alone were not counted as that evidence.
Persistent invalid replies can still exhaust bounded recovery and produce a specific error.
Cancellation during the correction invocation itself and two consecutive malformed replies
were not observed in this campaign.

Native execution was Linux only. Windows-style paths were ordinary exact-value inputs,
not a claim of native Windows execution. Positive Go inference needs appropriate account
configuration and entitlement, which were not changed. No live user code was edited.

The first terminal replay hit a `pyte` wide-character rendering exception after one
successful exchange. The observer was corrected to render empty continuation cells, its
owned daemon was stopped, and a fresh three-exchange journey passed. That was a harness
failure, not a Peritus failure. A capture-wrapper revision also made stderr relay live,
preserving the timing required by the existing vendor-error observer. The first Go-screen
check ran before the next UI update, and its initial readback oracle incorrectly required
an idle footer on an intentionally stopped conversation. A fresh client with the correct
expected stopped state displayed the persisted, specific error guidance successfully.
