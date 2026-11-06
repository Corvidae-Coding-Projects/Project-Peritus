# Design steering recovery — 2026-11-06

This records the earlier steering-only repair and its installation snapshot. The subsequent
expert-workflow repair and current qualification are in
[domain-design-expert-pass-2026-11-06.md](domain-design-expert-pass-2026-11-06.md).

## Reproduced failure

The local Jackdaw run `915377a98f4584b16562bf5b8602b9fd` remained in Designing with a
Captured expansion bound to input revision 2 while its conversation had reached revision 3.
Three complete design submissions were rejected as stale. The captured trace showed 846 model
requests and 5,280 tool calls, including repeated reads of unchanged source. No implementation
write from that run was found. The run was already Cancelled when repair began.

The three rejected submissions were recovered from its trace into private, explicitly unvalidated
proposal artifacts. They have not been promoted to accepted designs by recovery.

## Repair contract

- Refresh the expansion after steering is incorporated, preserving the original source prefix and
  appending the exact new instruction. Pending input cannot replace the current source.
- Preserve previous source/ledger manifests and every submitted publication proposal. An observed
  proposal remains unvalidated; publication still checks current input, evidence, and record links.
- Return a verified first source page with state observations and provide `design_source_read` for
  exact bounded continuation pages. Archived model messages are not substitutes for source bytes.
- Require a durable design checkpoint after three consecutive repeated-inspection warning batches,
  or 24 model requests without a checkpoint. New evidence resets the repeated-inspection streak;
  actual ledger progress resets both counters. Two unchanged attempts at a required checkpoint stop
  the attempt with retained recovery evidence. This is a progress boundary, not a token budget.
- Name an invalid change-ledger entry and explain its required source-reference type in rejection
  feedback, so the designer can repair the relevant record.

## Verification

The 65 product-runner library tests pass, including actual filesystem replay of pending versus
incorporated steering, stale submission, restart, exact retained draft bytes, source paging,
successful current-input publication, repeated file reads, fresh-evidence reset, cached-read
admission, and a context-only loop reaching the checkpoint boundary. Strict Clippy passes for the
package's targets and features. These state replays do not invoke a model.

A workspace check with all targets and features passed earlier in the repair. The final package
checks and release binaries cover the subsequent source-reader and diagnostic additions.

The credentialed disposable-repository replay with OpenAI Codex and `gpt-6.1-sol` reached Ready
and then Writing in six model requests after a message arrived during design. Its saved design
bound revision 2, preserved the original `initial_value` requirement, and included the new
determinism requirement. Evidence is retained locally under
`~/.cache/peritus-design-steering-final.tsypG89X/steering/`. The harness deliberately cancels when
Writing begins; this establishes design handoff, not implementation or complete task acceptance.
This replay preceded the final diagnostic-wording clarification; final installed-run evidence is
separate. An earlier diagnostic attempt exposed the missing direct source reader and was stopped;
it is not a passing replay.

## Installed run

Built the final release binaries and atomically installed `peritus`, `peritusd`, and `peritus-tui`
for doll and builder. Verified matching SHA-256 values for both accounts. Original binaries remain
under each account's `~/.local/state/peritus/install-backups/design-steering-20261106T111900Z/`.

The new doll daemon's executable digest is
`fa04be8e0a085aa4504dc211c7ad5542e97587a545a2d15989889bb0122dcd6c`.
The actual CLI and TUI reopened the original run, and `runs retry` resumed it. The model
acknowledged the existing instruction to finish coding before testing and handed control back to
the coding pipeline. The installed runner refreshed source and expansion bindings to revision 3,
preserving the original source as the prefix. Its first design-state observation succeeded and
retrieved the retained proposal. The run remains active in Designing while reconciling that draft;
the browser project's implementation and acceptance are not claimed complete by this repair.
