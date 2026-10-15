# Terminal disconnect and retry recovery

This records the initial transport-recovery repair. The later first-connection timeout and
its underlying CPU stall are covered by [handshake-stall-real-2026-10-15.md](handshake-stall-real-2026-10-15.md).

## Requirement and source findings

The user reported that `/retry` appeared ineffective and the terminal showed `client task failed`; manual Ctrl+R restored it. Read-only inspection confirmed that the original run had actually accepted Retry and was progressing through the pipeline. The personal daemon and coding task were left running.

The displayed prefix comes from `TuiError::Task` and covers several failures, including writer backpressure, closed queues and connection deadlines; it does not establish a panic. The complete original detail was lost when the terminal redrew, so the exact first transport failure is not retrospectively confirmed.

A concrete recovery defect is visible in the source: reader/writer disconnect events only invalidated model state. They did not discard the connection owner's stale session/outbox or schedule reconnection. Subsequent sends could report another client task failure, and the terminal required Ctrl+R despite live daemon work.

## Planned real-interface checks

Before replay, the oracle is: a terminal with an unsent Unicode draft must remain usable when its own disposable daemon stops, preserve offline edits, reconnect automatically after restart, and refresh live state without replaying Retry or the unsent draft. Repeat with abrupt process death. A private bounded log must retain failure stage/details and reconnect generations without draft or message payloads. The existing binary must demonstrate the missing automatic recovery and successful manual Ctrl+R. Existing durable recovery may complete a previously submitted chat transaction through its original receipts; that behavior must remain intact.

Use actual CLI/TUI/daemon entrypoints, isolated XDG directories, real PTYs, real Unix IPC and real process stop/restart. Select offline browsing to avoid inference. No unit tests, mocks, formal verification or repository verification suites are used. Build with `cargo build --locked --release --jobs 2 -p peritus-cli -p peritus-tui`.

The new owner drops failed transport handles and queued old-context frames, applies 0.5/1/2/4/8-second bounded backoff, and reconnects with the retained durable session and draft. Existing revisioned receipt recovery resolves uncertain operations; the connection owner does not resend controls. Stable connections reset the backoff. Normal launcher windows record private bounded `peritus-tui.log` diagnostics in their existing log directory.

## Final program and reproduction

Linux, native PTYs (140 × 36), Unix sockets, real packaged daemon and local persistence;
release build with two Cargo jobs. Source baseline: `57440fc16475237dc2bf3609996f4991e1186840`.
The final client binaries exercised and installed have these SHA-256 hashes:

| Artifact | SHA-256 |
| --- | --- |
| Changed `peritus` | `d67fb90f1193246e5538748c930750c5bd5ffb0cef734769d63712e03f9a46b8` |
| Changed `peritus-tui` | `c83101aa24bc25a2a2d517136a4c48e01f67f2dce161500c850cab9289dad146` |
| Unchanged real `peritusd` | `102d80f42275dceaee8db8888e9be3bf05028324ffcf73ebde9918c80b58a889` |
| Original `peritus` | `41cf89263c295bc70f9727947ffea2731e51c5a8063aea295d905c8b312c0da4` |

Retained driver: [tui-reconnect/replay.py](reproducers/tui-reconnect/replay.py).
Use a fresh output directory for each invocation:

```sh
cargo build --locked --release --jobs 2 -p peritus-cli -p peritus-tui
uv run --offline --with pyte python3 docs/testing/reproducers/tui-reconnect/replay.py --binary target/release/peritus --output /home/USER/pr-fixed
uv run --offline --with pyte python3 docs/testing/reproducers/tui-reconnect/replay.py --binary target/release/peritus --output /home/USER/pr-retry --with-retry
```

Keep the matching real daemon beside the client. The optional Retry replay requires the real
ChatGPT account/vendor CLI and chooses `gpt-5.6-sol`; it does not substitute a provider or
fabricate output. The default replay is offline. To demonstrate the original defect, use the
original binary pair with `--expect-bug`; it must stay offline after its daemon returns and
recover only after the driver explicitly presses Ctrl-R.

## Observed results

| Scenario | Observation | Result |
| --- | --- | --- |
| Original installed client | Daemon returns, but TUI stays offline; Ctrl-R restores the exact draft. | PASS: defect reproduced |
| Graceful daemon loss | Editing stays responsive during missing-socket failures; client reconnects automatically. | PASS |
| Abrupt daemon death | SIGKILL of the owned daemon produces EOF/refused connections; real WAL/session recovery reconnects to the TUI. | PASS |
| Draft preservation | Exact `retain café 雪 — edited offline + crash recovery` survives both loss cycles. Independent run used distinct `Zoë 🚲 雪` text. | PASS |
| Continued operation | Independent operator clears its disposable draft, types `/doctor`, receives the actual daemon report and exits cleanly. | PASS |
| Failure diagnosis | Log records read EOF, missing endpoint, connection refused and successive connected generations; observed retry delays 0.5/1/2/4 seconds. | PASS |
| Log privacy | Actual log mode is `0600`; no draft fragments appear. | PASS |
| Explicit Retry | Real Plan request is stopped, `/retry` restarts the same run and no extra public input is created. | PASS |
| Loss after admitted Retry | Restart reconnects to the same retained run; it remains RecoveryRequired, with two total model requests and no extra Retry or model request during observation. | PASS |
| Fixture preservation/cleanup | `notes.md` unchanged; fixture clients exit 0 and owned daemons stop. | PASS |

Bounded systematic exploration used five successful fresh fixtures: one original-binary fixture,
two parent fixed-binary fixtures (offline and real Retry), and two independent fixed-binary
fixtures. Together they exercised ten actual transport-loss cycles, including the baseline
failure, graceful missing sockets, stale sockets after SIGKILL, repeated connection attempts,
offline typing, automatic recovery, and live command readback. These were enumerated lifecycle
sequences rather than random fuzzing; there is no random seed. Failed driver-development attempts
are additional to that count and are not classified as product defects.

The independent operator ran separate fresh processes and inspected its own raw terminal
observations, file contents and log permissions. It also inspected the existing receipt recovery:
the connection owner drops queued transport frames, and ProductControl Retry remains an explicit
user action. Existing durable chat reconciliation can finish an already submitted transaction;
this campaign does not claim all in-flight mutations were exercised.

Local raw evidence:

- `/home/doll/prn-old3/result.json`: old-binary failure and manual recovery.
- `/home/doll/prn-new2/result.json`: graceful/abrupt automatic recovery and exact draft.
- `/home/doll/prn-retry3/result.json`: real Plan stop/Retry, stable run identity and no repeat after restart.
- `/home/doll/prn-independent2/result.json`: first successful independent lifecycle replay.
- `/home/doll/prn-independent4/independent-report.json`: independent repeated replay, `/doctor`, binary hashes and precise limits.

Driver corrections were kept distinct from application defects: a transient `reconnected`
notice can be replaced immediately by subscription status, so the oracle now checks persistent
Online rendering; wide-character continuation cells need correct terminal text extraction;
control actions must wait for the visible cancellation and its acknowledgement rather than
only seeing an earlier on-disk state. The first real Retry attempts hit those timing assumptions;
the final replay uses the normal visible controls and passed without changing production code.

## Deployment and limits

Only rebuilt `peritus` and `peritus-tui` were installed, with matching hash readback. Previous
clients and the unchanged companion daemon are retained in
`/home/doll/Peritus-Tui-Reconnect-2026-10-15`. The live personal daemon and client were not
signaled or restarted; the original browser run continued. An already open terminal retains its
old code until the user closes that client with Ctrl-Q and opens the run again.

The exact first error from the user's old client is still unknown because it was not persisted.
The fix establishes automatic transport recovery and makes future failure details durable; it
does not prove that every possible reason a local stream could fail has been eliminated.
Windows named pipes, macOS, writer-queue saturation, a silent but unclosed peer, the 1-MiB log
truncation boundary, and ambiguous in-flight write/command delivery were not executed here.
The log cap and eight-second maximum retry delay are implemented but their boundary values were
not reached in this campaign. No unit tests or formal/repository verification gates were run.
