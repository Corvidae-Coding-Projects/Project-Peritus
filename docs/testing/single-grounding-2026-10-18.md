# One startup grounding check

The user's stopped run restarted broad discovery despite 16 hash-current file receipts. The
designer could keep using read-only tools indefinitely; reusable evidence alone did not enforce a
handoff. The earlier duplicate-byte metric also did not count novel, unnecessary discovery.

## Observable contract, defined before replay

- One successful permitted workspace listing or file read completes startup grounding, including
  a nonempty repository. It does not force a subsequent repository file read.
- Completion is bound to the run, workspace identity, and canonical root. Roles, retries, new
  requirements, changed directory entries, and changed/deleted files do not reset this phase.
- Source hashes and current access policy still control reuse of each file observation. Editing
  an existing target still requires a current read; a completed startup check is not edit or
  acceptance authority.
- After the check, the designer receives no tools and must return a task plan. The writer keeps
  targeted source and execution tools. Specific unanswered questions move into implementation.
- A real provider must reach Writing on a disposable Peritus checkout after one inspection call,
  then after no new inspection calls when restarted with changed instructions in that same scope.
- A complete real-provider run must implement a CLI change, exercise its actual commands, and
  preserve unrelated data. Independent stdout/exit checks decide success.

## Replay and results

The final native consumer passed **19 grounding/handoff scenarios**, including four generated
path forms (plain, spaces, nested Unicode, and brackets), directory/task changes, fresh processes,
changed/deleted targets, separate workspace identities, and migration of a receipt produced by the
actual old binary. The retained host/context replay passed **26 scenarios** on the final binary.
Its empty-to-nonempty expectation now follows the explicit new contract: loss of an emptiness
observation does not reset completed startup readiness.

An authenticated `gpt-6.1-sol` run operated a disposable `git archive` copy of Peritus. The first
design reached Writing with **two provider requests and exactly one `workspace_list` call**.
Its next request advertised zero tools. A new process with changed instructions reached Writing
with **one provider request and zero tool calls**. The full trace across both attempts contains
only that original listing; it contains no file reads. Both attempts were deliberately cancelled
at Writing so the fixture's source was not changed. The host supplied a missing heading without
another request.

A separate complete run implemented `--shout` in a small existing Python CLI. An independent
process appended a comment immediately after its source read. The actual host rejected the stale
patch, the writer refreshed that target, preserved the comment, and completed the change. The
independent oracle checked four exact stdout/exit cases and the unchanged unrelated file. The
run settled **Accepted**. Its complete tool sequence was:

```text
workspace_list
workspace_read
workspace_patch  (stale edit rejected)
workspace_read
workspace_patch
run_command × 4 (actual CLI cases)
workspace_read  (independent reviewer)
```

There was one startup check across designer, writer, and reviewer. The first changed file occurred
49.989 seconds into the run, including deliberate stale-edit recovery. This run used 10 provider
requests, 194,184 provider-reported input tokens, and 8,064 cached input tokens; it made no auxiliary
compaction calls. This is evidence of the handoff and targeted workflow, **not** proof that overall
token overhead is solved or a matched performance benchmark.

Two defects found during implementation were corrected and replayed:

| Defect | Correction | Replay |
| --- | --- | --- |
| Closing new tools also removed the capability required by completed tool exchanges in history. | Keep ToolCalls negotiated while advertising an empty catalog and tool choice `none`; disable parallel calls for the terminal-only request. | The final real designer reaches Writing after the single check, including a fresh-process retry. |
| A useful plan without a `#` heading was rejected and regenerated. | Add the document heading in the host while preserving the plan body. | The live Peritus task deliberately requests an unheaded plan; the host publishes its headed document without a formatting retry. |

Build the ordinary consumer with
`python3 docs/testing/reproducers/build-grounding-efficiency.py target/single-grounding/consumer`.
Builds here used two compiler jobs under a 6 GiB memory ceiling and 200% CPU quota. The final
consumer was built with `cargo build --locked --manifest-path target/grounding-efficiency/consumer/Cargo.toml -j 2`
and `CARGO_TARGET_DIR` set to this checkout's `target` directory. The CLI, daemon, and TUI were also
built with `cargo build --locked --release -j 2 -p peritus-cli -p peritus-daemon -p peritus-tui`.

Replay commands (use fresh output paths):

```sh
python3 docs/testing/reproducers/single-grounding.py \
  --consumer target/debug/peritus-real-grounding-efficiency \
  --baseline-consumer target/single-grounding/baseline/consumer \
  --output target/single-grounding/replay-final \
  --peritus-copy target/single-grounding/peritus-copy
python3 docs/testing/reproducers/grounding-efficiency.py \
  --consumer target/debug/peritus-real-grounding-efficiency \
  --output target/single-grounding/retained-host-context-final
python3 docs/testing/reproducers/grounding-concurrent-edit.py \
  --consumer target/debug/peritus-real-grounding-efficiency \
  --output target/single-grounding/live-cli-final --ordinary-task
```

The optional baseline flag needs the retained old executable. Without it the current one-check
and live-handoff replays still run. Prepare the Peritus copy with `git archive HEAD` extracted into
an empty disposable directory; it is test input for the current ProductRunner, not a second daemon.
Compact identities/results are in [the evidence receipt](evidence/single-grounding-2026-10-18.json).
Original failures, raw provider traces, source receipts, and fixture outputs remain under
`target/single-grounding/`.

No unit tests, formal verification, or repository qualification suites were run. No live run was
resumed or modified. These checks cover the public ProductRunner and its actual Codex route,
filesystem, and processes; they do not certify every provider, client UI, or a multi-hour job.
