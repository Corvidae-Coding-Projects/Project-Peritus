# Long-run runtime friction: real testing

The changes address observations from the live CLI-browser run
`81f9302c94ce667a2fff398d434f6192` without restarting that run or replacing its installed binaries.
Implementation and behavioral replays use an isolated Peritus worktree and disposable state under
`/home/doll/Peritus-runtime-friction-2026-10-12`.

## Observable contracts fixed before replay

| Path | Independent expected behavior |
| --- | --- |
| Live provider accounting | With provider input/output counters and no explicit total, the displayed/durable total includes input + output, including during an ongoing phase. Cache reads already included in input are not added twice. |
| Delayed TUI heartbeat | A valid reply delayed past one interval but inside the missed-reply grace keeps the actual TUI connected. An ordinary session remains connected; genuine timeout still disconnects. |
| Command capture | A successful command with output above the former 8 MiB artifact bound publishes its complete retained bytes, reports the real exit status, and exposes bounded output without loading the whole stream into model-facing strings. |
| Output readback/recovery | Exact byte pages preserve binary/Unicode data, offsets, and EOF; reopened runtime ownership can read retained output without reexecuting the command. Invalid handles and ranges are rejected. |
| Tool friction | Existing files require current-invocation reads; retries receive a concrete next action. Observed names, direct argv semantics, protected Git metadata, and completed handles are explained in the actual tool catalog. |
| Malformed provider arguments | Strict parsing and permissions remain intact. Invalid JSON gets safe call/line/column diagnostics; arguments and credentials are not copied into diagnostic codes. |
| Connection diagnostics | Real connection errors include a timestamp and daemon PID for attribution. |

## Baseline observations

- The copied installed daemon has SHA-256
  `e6de524a05f6340c8f150eb559ac869c94e2ca91ebb4e2b50f88a6b116658366`.
- Two fresh actual TUI sessions reproduced the delayed-heartbeat disconnect. Each paused only the
  fixture TUI at age 5 seconds for 17 seconds, then resumed before the 40-second timeout boundary.
  Ordinary heartbeats passed; a separate 42-second pause triggered the genuine timeout.
- The original run trace at byte offset `22325933` records an exit-0 search with
  `37,349,240` stdout bytes fully retained, but incomplete artifact publication and empty model output.
  The command artifact store was configured for only 8 MiB despite unbounded retained spooling.
- The original live snapshot reported about 77 million input tokens and 703 thousand output tokens,
  while its total remained `37,475,222`. The live completion path treated an absent total as zero.

## Final replay evidence

No unit, mock, formal-verification, or repository qualification suites were created or run.
The actual CLI, daemon, and TUI were compiled from the copied worktree with:

```sh
systemd-run --user --scope --unit=peritus-friction-build-v3 \
  -p CPUQuota=400% -p MemoryHigh=6G -p MemoryMax=8G -p TasksMax=256 \
  env CARGO_TARGET_DIR=/home/doll/Project-Peritus/target \
  cargo build --locked --release -p peritus-cli -p peritus-daemon -p peritus-tui -j 2
```

The final incremental build completed in 2 minutes 15 seconds. Only explicit changed source files
were formatted; a workspace formatter encountered already-removed fixture examples, and its
unrelated formatting changes were restored. No missing tests were recreated.

Final copied executable SHA-256:

| Executable | SHA-256 |
| --- | --- |
| peritus | `1978ef4018c9c3d65c4bf59b43cb83e96808f8ef31098345326991f7687d48bc` |
| peritusd | `4040fea9c24dc01aae5e0b219789221545f72d410162426d3c7b45ec1c8744e2` |
| peritus-tui | `04ce864cc904059655909160beca2f3967802de8f1c3f73e78637fe61ba71a6b` |

| Actual interface and action | Observed result |
| --- | --- |
| Real TUI/daemon ordinary heartbeats, delays of 6/11/17/29 seconds, and genuine 42-second timeout | PASS in six fresh sessions. Delayed replies stayed connected; genuine timeout remained enforced. The saved 17-second regression was also replayed on the final executable above and passed. Other timing cases retain their unchanged-source candidate evidence, with exact hashes in the heartbeat README. |
| Real daemon timeout log | PASS: epoch-millisecond timestamp and PID identify the actual fixture daemon. |
| Public command API, seven native `cat` processes | PASS at 0, 1, 16,383, 16,384, 16,385, 8,388,625, and 37,349,240 bytes. Includes all 256 byte values, 28 exact spool pages, seven past-EOF rejections, six nonempty finalized artifact checks, and seven reopened-runtime readbacks. Preview strings remained bounded. |
| Actual TUI → real ChatGPT account provider → writer → `run_command` | PASS: generator dispatched exactly once, exit 0, artifact publication complete, 37,349,240 bytes retained, bounded 16 KiB preview, exact BEGIN/middle/END/EOF pages from the same handle. |
| Actual `command_output` negative calls | PASS: past-EOF and unknown handle rejected, without another generator execution. |
| Ongoing live provider accounting | PASS: at 11 requests during the writer phase, input 273,972 + output 6,442 = total 280,414. Cached input 14,592 was not added twice. Final run counters also matched: 402,167 + 8,795 = 410,962. |
| Existing-file workflow through the real writer | PASS: writer listed the exact observed names, reread the existing evidence target within its own invocation, rewrote only evidence.json, and read it back. Parent independently compared both original files byte-for-byte and checked that exactly three files remained. No Git, shell-wrapper, build, lint, or unit-suite commands were executed. |
| Actual native provider deliberately malformed inner argument JSON | PASS: single bad call returned `openai.codex_runtime.invalid_tool_arguments.call_0.line_1.column_10`; a bad second call with a newline returned `call_1.line_2.column_12`. Each run made three real provider attempts, then stopped; neither executed any host tool. Diagnostics contain positions rather than the argument sentinel. Valid multiline output-page text also survived the ordinary evidence write/readback. |
| Owned fixture shutdown | PASS: copied CLI reported all six shutdown stages clean with remaining=0; fixture endpoint removed. Heartbeat children were separately reaped. User's original TUI/daemon remained running. |

The native generator run is `4ddff1d8e5fa14a33b744f97a1352034`, using the real
`gpt-5.6-sol` account route and isolated XDG directories. Its command handle is
`6965aa76f5cae067d4fb7c44f109ae02`. The parent independently decoded the actual durable tool
observations and compared page bytes with the fixture's independently specified markers and
16-byte period; the model's success summary was not the oracle. Peak daemon RSS was about 42 MiB.
Malformed runs are `fd1428dd48504259cb99880e4c089a51` and
`637e9d5b7fd83970ab9189b53c163a90`. The single-line invalid token is at column 10; the
multiline invalid closing brace is at column 12 on line 2, independently countable from the inputs.

An initial fresh account request stopped with the provider's generic `reported` error before any
tool ran. A new conversation with the explicitly selected available model completed the replay.
The driver also initially sent pasted slash commands and used a cooked outer terminal; these
driver mistakes were corrected using keyboard-owned commands and raw outer input. They do not
count as application passes or product defects.

Real replays caught two implementation mistakes before commit: increasing the command allowance
without increasing the shell descriptor ceiling caused preparation rejection, and changing the
descriptor without binding it in the dispatcher caused digest mismatch. The final change selects
an explicit ceiling at construction and dispatch while preserving exact digest/authority checks.
Legacy shell descriptor/dispatcher entry points retain their previous 8 MiB contract.

## Retained replays, scope, and limits

- [Heartbeat replay](reproducers/runtime-friction/heartbeat/README.md) preserves the concrete
  timing failure and deterministic timing generator, operating actual copied binaries and PTYs.
- [Command output replay](reproducers/runtime-friction/command-output/README.md) preserves the
  ordinary public-library consumer, independent byte generator, native TUI task, and exact commands.
- [Malformed native-provider replay](reproducers/runtime-friction/native-arguments.md) preserves
  both intentionally invalid inputs and the actual expected rejection diagnostics.

Local raw evidence is under `/home/doll/Peritus-runtime-friction-2026-10-12`: `candidate-v3/`,
`command-consumer-v3/results.json`, `heartbeat/`, and `live-candidate/`. The latter retains
`native-command-v3-observations.json`, `native-readback-v3.json`, `live-accounting-v3.json`,
`native-malformed-v3-results.json`, the actual TUI ANSI stream, and isolated daemon run traces.
Large successful transcripts are kept outside Git; only useful replay inputs/drivers are committed.

This is bounded regression evidence, not another 11-hour run or whole-application coverage. The
logical command artifact quota remains 1 GiB; capture and bounded spool readback remain separate
from publication. The campaign did not force a new storage-permission/quota failure, exercise
every interpreter/dependency-path/completed-handle combination, or qualify every provider. Guidance
for those friction cases was inspected in the tool catalog and workflow; existing permissions and
read-before-edit enforcement were preserved.

The artifact-only native fixture produced and reread its requested deliverable, then the existing
automatic acceptance stage stopped because evidence.json has no applicable project check contract.
No automatic checks ran. This does not invalidate the independently observed command/tool behavior,
and does not establish that the pipeline's acceptance stage passed. That existing acceptance policy
was outside this runtime correction and was not removed or bypassed.
