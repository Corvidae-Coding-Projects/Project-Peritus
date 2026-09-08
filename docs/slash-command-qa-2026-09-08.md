# Interactive slash-command audit — 2026-09-08

## Scope and environment

Live installed `/usr/bin/peritus` 0.0.1, actual TUI in a 120x40 tmux PTY,
OpenAI-account provider with configured `gpt-5.6-sol`. Local branch `develop`
starts at release/main commit `abe690a5cff4ffb51498721e557996e942234b85`.
No production implementation changes or push are part of this audit.

All app config, daemon state, cache, and Git work are isolated under
`/tmp/peritus-slash-qa.SBeIgf`; no personal Peritus workspace is used.
Endpoint: `state/peritus/daemon/peritus-82852c9c3da9c9dd84f38fccf66702d5.sock`
relative to that fixture. Initial screen captures are retained in
`initial-command-screens.json` there. Two malformed automation inputs containing
extra slashes were corrected and retested; they are not product findings.
Additional live captures are in `additional-command-screens.json`; the completed
build snapshot is in `final-build-result.json`.

## Confirmed findings

### High: new conversations are blocked by an idle conversation

Reproduction: `/chat Reply exactly QA-CHAT-OK without tools.`, wait for reply,
then `/new`, then submit another chat or build request. The UI resets, but
the daemon rejects the new run with `command / idempotency-conflict / retry never`.
Subsequent polling can replace that error with `codec / invalid-identifier`.
The old conversation remains `WaitingForUser`; `/stop` on that idle conversation
reports `No active work to stop.`

Evidence: conversation `c935169a703b64e26e99f18ba5b95a42` completed chat, plan,
and review replies. Cancelling that exact isolated run using CLI `runs cancel`
allowed the previously rejected `/build` request to start normally. Restarting
the TUI alone did not resolve it.

Cause: `crates/app/peritus-tui/src/model/chat/commands.rs` resets only local
conversation state for `/new`; `crates/app/peritus-daemon/src/product_run/snapshot.rs`
`workspace_has_active_run` treats `WaitingForUser` as active. Meanwhile
`crates/app/peritus-tui/src/model/chat.rs` prevents `/stop` when chat work is idle,
although daemon `product_run/lifecycle.rs` explicitly supports cancelling that phase.

Corrective direction: reconcile idle conversation ownership with starting a new
conversation without silently discarding existing work; make any blocking state
and its supported resolution visible. Test idle-to-new and active-to-new separately.

### Medium: failed submissions keep polling a nonexistent run

`send_chat_message` in `crates/app/peritus-tui/src/model/chat.rs` sets `chat.run_id`
before the durable receipt. `response_error` in `model/protocol/response.rs`
restores the draft after a rejected `ChatSubmit`, but does not roll back that ID.
`poll_chat` queries it thereafter. Daemon `query_interaction` returns `NotFound`,
mapped to `InvalidIdentifier` in `crates/app/peritus-daemon/src/session/request.rs`.
This masks the actionable original failure and leaves the client tracking a run
that was never created. Preserve prior accepted conversation state on rejection
and distinguish submitted IDs from durably accepted IDs.

The same generic `idempotency-conflict` mapping also hides why `/discard` rejects
an already committed candidate. That rejection is correct, but the UI should
explain that a committed candidate cannot be discarded.

### Medium: trace list and detail can disagree

`/trace` showed `No matching live events received` on the left and a
credential-registry event on the right. In `crates/app/peritus-tui/src/render.rs`,
the list is filtered with `visible_event_indices`, while the detail pane reads
the unfiltered `selected_event_record`. A selected event from a different family
can therefore remain visible. Bind both panes to the same filtered selection,
including the empty-list state.

### Low: released daemon advertises the old version

Dashboard header says `peritusd/0.0.0` in the installed 0.0.1 release.
`crates/app/peritus-daemon/src/config.rs:29` hardcodes `DAEMON_VERSION` to `0.0.0`.
Derive it from the package version and verify the handshake/display in release tests.

## Command coverage

Each advertised command was entered into the live TUI. Passing a navigation or
empty-state check does not establish all downstream operations or policy enforcement.

| Command | Live observation |
| --- | --- |
| `/chat` | Mode selection and real `QA-CHAT-OK` response pass. |
| `/plan` | Read-only mode selection and real `QA-PLAN-OK` response pass; adversarial write-policy test not performed. |
| `/review` | Read-only mode selection and real `QA-REVIEW-OK` response pass; independent-review quality not assessed. |
| `/build` | Qualified three-file Rust candidate completed after clearing the idle-run blocker; eight acceptance commands passed. |
| `/model` | Fresh seven-model catalog opens for writer/reviewer; advertised writer selection and explicit manual fixer selection pass using the same configured model. Missing manual ID gives usage feedback. |
| `/new` | Clears the view, but subsequent work is blocked by the prior idle run: confirmed failure. |
| `/status` | Shows new-conversation and active build status; unexpected arguments are rejected with the draft retained. |
| `/diff` | No-run guard, active empty-diff view, and completed candidate paths/diff/verification/run-instruction display pass. |
| `/runs` | Run dashboard opens. |
| `/trace` | Opens, but filtered list and details disagree: confirmed failure. |
| `/terminal` | Opens detached terminal view with explicit ProcessId attach instructions; actual process attachment was not exercised. |
| `/approvals` | Opens empty approval queue; no genuine pending approval was available. |
| `/details` | Initially invisible when no tool details exist; actual build tool metadata expands correctly. Empty-state feedback would improve discoverability. |
| `/stop` | Cancels active build `cd5a73432418ca20ae13a56a6c9cecb6`; idle conversation cannot be cancelled here despite daemon support. |
| `/accept` | No-run guard and qualified candidate acceptance pass; durable accepted flag verified. |
| `/commit` | Creates candidate-only commit in disposable managed worktree; exact revision verified. |
| `/export` | Writes candidate patch; `git apply --check` against unchanged fixture source repository passes. |
| `/discard` | No-run guard and committed-candidate rejection exercised. Rejection is correct but cryptic. Successful live discard was not exercised; focused daemon test passes. |
| `/run` | Executes retained candidate command and reports successful completion. |
| `/reconnect` | Reconnects and resumes event subscription after cursor 1. |
| `/help` | Lists all 22 commands; intentionally leaves `/` in composer for selection. |
| `/quit` | Closes isolated client while daemon remains `ReadyReadWrite`; reconnect succeeds. |

## Verification and remaining coverage

`cargo test --locked -p peritus-tui --lib`: **38 passed**, no failures.
`cargo test --locked -p peritus-daemon --lib product_run::deliverable::tests`:
**5 passed**, covering bounded export/discard paths, exact-candidate identity,
restart staleness, idempotence, and excluding unrelated staged files from commit.
Unknown slash commands are rejected locally with explicit feedback.
This existing unit suite is supporting evidence, not a substitute for the live checks.
No code changes means no claim of fixing findings or passing workspace-wide gates.

Completed live build: `ef634bad6db538f578c914787dc55120`, qualified candidate digest
`e86da8819feae3896fb648c5e0ea2fedf6fd963cf943c86d1d0496cf0ad34f95`.
`/commit` produced `cd881da1eb3f3ff46787910e6425f013377ae70f` in the disposable
managed worktree, not in Project-Peritus. Export remains under fixture daemon
`exports/ef634bad6db538f578c914787dc55120.patch`.

Coverage limits: no live successful discard of an uncommitted candidate, terminal
attachment, pending-approval response, unqualified-candidate confirmation, or
adversarial read-only-policy test. Those paths are not claimed as end-to-end passes.
All 22 slash entry points were exercised; overall assessment: **changes required**.
