# Finalization review

Date: 2026-09-13

Baseline commit: `8644a2d2bebc4b8e5c8d5bf223eaddb5955bb5a6`

Reviewer: `/root/gap3_schema_registry`. This is a technical agent review, not human approval or
protected authorization.

This is a bounded, read-only review of finalization admission, terminal-summary construction,
canonical terminal hashing, terminal-state mutation, and the production control wiring. The
reviewed files had the following SHA-256 hashes:

| File | SHA-256 |
| --- | --- |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/control.rs` | `7dc4be2a97a1638f6b78ee21a0c937a0917bd4c7c3875de80d4d31041d9d7aee` |
| `crates/orchestration/peritus-scheduler/src/reducer/apply/control/finalization.rs` | `ac93d52283b7a147b2a49d8f558ae7cdff22d25404ec8e3f6f7ced438e1da507` |
| `crates/orchestration/peritus-scheduler/src/state/terminal.rs` | `574322e5def9e5ab0a47a5b94a64067de94f159b9aecb0298ccb440ab616d93e` |
| `crates/orchestration/peritus-scheduler/src/state/terminal/evaluation.rs` | `d29cc17e3272b3a14d5257452c2a03918e287494828969bd82de9c62034f552a` |
| `crates/orchestration/peritus-scheduler/src/state/mutation.rs` | `05fc336949d9d3c21068829f4f81ef73c7721e1860dcc9325b95c70861040418` |
| `crates/orchestration/peritus-scheduler/src/state/mutation/set_terminal.rs` | `fc8be0a8713e194f2fb99660d5e7c07f481a5081eb0cf5101c7ced02d6505c4e` |

## Verdict

PASS for the reviewed finalization boundary. I found no changed admission order, rejection
mutation, error mapping, terminal classification, non-success identity order, canonical terminal
digest input, stored/event terminal value, state-field frame, ordinary public type, or encoded
byte layout.

- Admission retains the previous short-circuit order. It scans work first and rejects on the first
  nonterminal record; it checks for retained reservations only after every work record is terminal.
  Rejection happens before mutation and maps to the same `IllegalTransition` detail.
- An admitted command summarizes the unchanged pre-state work exactly once. The prepared terminal
  has the evaluator's classification and non-success work IDs in retained work order. The control
  path computes one terminal digest from those fields, and installing that digest does not change
  the prepared classification or IDs.
- The committed terminal is cloned once for storage and moved into the `SchedulerFinalized` event.
  The stored and emitted values therefore have the same kind, ordered IDs, and supplied digest.
  `set_terminal` changes only scheduler phase and terminal summary while framing every other
  command-level state field, including work and reservations.
- `SchedulerTerminal` retains its field order and derives. Its ordinary getters and wire-facing
  constructor retain their erased Rust signatures. The terminal and event wire tags and encoders
  are unchanged, so the refactor does not alter canonical event or state bytes.
- The later `state_digest` call in `decide` remains the separate, pre-existing hash of the complete
  successor state after refresh and cursor advancement. It does not repeat terminal-summary
  evaluation or terminal-digest construction.

## Scope and independence

I authored the worker-refresh implementation and drafted the dependency-refresh measure and the
iterative reservation-count helper in this increment. Those files, their mutation wiring, and
their proof claims are excluded from this independent verdict. Within `state/mutation.rs`, this
review covers only the `set_terminal` module wiring. I did not author the finalization plan,
terminal evaluator contracts, terminal mutation, or finalization control wiring reviewed here.

The review establishes the command-level finalization boundary. The outer reducer's refresh,
cursor, complete-state digest, and event-envelope composition remain separately owned proof and
review boundaries.

No build, test, lint, or verifier command was run for this review because the root agent owns the
single validation lane. I inspected the current source and its diff from the baseline and ran the
listed source-hash command. The final frozen scheduler source is bound by the 176-entry gate 228c
manifest, whose SHA-256 is
`f3682e9867d5e449f36c6db291a58244716d9df99af38c926adb3531af596aab`; none of the six reviewed
files changed between this review and that manifest. The final root-owned validation pipeline
exited successfully: format gate 226c and architecture gate 227 passed; strict Clippy gate 231b
passed in 6.67 seconds; ordinary test gate 229b passed 79 tests with zero failures and zero ignored;
strict Verus gate 230c verified 628 verification items with zero errors in 38.92 seconds under
`--no-cheating --rlimit 20`; and ordinary-API gate 232 checked 3,641 files and 14,764 entrypoints.
The gate 228c source-manifest hash was rechecked after every validation gate passed.

```text
sha256sum \
  crates/orchestration/peritus-scheduler/src/reducer/apply/control.rs \
  crates/orchestration/peritus-scheduler/src/reducer/apply/control/finalization.rs \
  crates/orchestration/peritus-scheduler/src/state/terminal.rs \
  crates/orchestration/peritus-scheduler/src/state/terminal/evaluation.rs \
  crates/orchestration/peritus-scheduler/src/state/mutation.rs \
  crates/orchestration/peritus-scheduler/src/state/mutation/set_terminal.rs
```
