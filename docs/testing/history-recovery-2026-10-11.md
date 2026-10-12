# Oversized context and durable-history recovery

Baseline: develop `32c012b8a531b2838ade25888af709f35782f6bd`, official Codex
0.162.1, Fedora Linux. Use actual Peritus CLI/TUI/daemon, real file storage and
the authenticated account provider. No unit tests, mocks, formal checks,
repository qualification suites or Clippy gates.

External evidence: `/home/doll/Peritus-history-recovery-2026-10-11/`.

## Observable contracts fixed before replay

- A successful large host read must not stop an ordinary conversation solely
  because Peritus resends too much inline history. Preserve a fitting read
  intact. For an oversized result, retain exact evidence durably, label the
  visible prefix and offer an actual retrieval handle and exact byte offset.
- The saved continuation must retain all its data across saving and restart.
  It must not fail solely because numeric JSON byte arrays inflate the snapshot.
- Existing version-six records remain readable. New artifact references must
  be digest- and length-checked; a missing or corrupt artifact cannot admit
  execution. Restart alone never retries settled or uncertain effects.
- Capacity errors must not claim that the user lost write access.
- Read-only turns and independent in-place reviewers need their own evidence
  archives without writer-derived memory or additional workspace-write authority.

## Confirmed defects

### HR-001: stopped continuation could exceed the 16 MiB snapshot ceiling

The user reported a 37,194,683-byte save rejected by the 16,777,216-byte bound.
An independently generated fixture used an actual retained Peritus continuation
and added three MiB of non-authoritative historical text. Its compact legacy
record was 12,007,350 bytes, so the old daemon could load it. The public CLI
`runs cancel` then attempted a 28,058,725-byte pretty-printed save and returned
exit 12 with the same durable-limit failure. The destination remained writable.

The new writer publishes raw continuation bytes in a digest-addressed artifact
before atomically replacing the small run-record pointer. The initial replay
saved a 67,294-byte version-seven snapshot plus a 3,207,745-byte continuation.
A fresh daemon reopened the cancelled run. All retained continuation values,
including the exact historical text, matched the independent fixture; the
existing documented restart transition changed next_phase 7 to Checking (3).

The metadata projection has a separate 128 MiB defensive file bound; it no
longer contains accumulated continuation byte arrays. Version-six inline
continuations are accepted and migrated when next saved. Old continuation
artifacts remain retained; no history cleanup is performed by this change.

### HR-002: every persistence failure recommended restoring write access

`ProductRunServiceError::persistence` appended a permission diagnosis to size
and serialization errors. Capacity now has a specific classified diagnostic;
other storage errors direct the user to their actual cause, disk space or
permissions without claiming that permissions caused the failure.

### HR-003: context selection ignored Codex's submitted-turn character cap

The Codex adapter wraps instructions, retained messages and tools into one new
turn. Model token estimates did not account for its separately enforced
1,048,576-character guard, or JSON escaping. Even an intact reader result could
therefore end an otherwise working run. Adapters now expose transport-aware
context-fit checks. Codex measures its own encoded projection, with framing
headroom, and guards its final exact prompt before starting a native process.
Other transports receive no new global token or character ceiling.

The first actual replay also demonstrated that read-only conversations and
in-place reviewers bypassed the local context port. Run
`0362dfc83b30219ed38ba9dda06f39c7` stopped after three requests and two tools.
Those paths now use separate reviewer archives, excluding writer-derived
memory. Explicitly disabling local memory still selects legacy behavior.

The copied Rust browser replay found a further bypass in design:
`9dcf9b1fcbc017c9c9659a477aba21e2` stopped at the first oversized design read.
Design now uses a separate read-only subtask namespace. Its observation handles
cannot resolve in the independent reviewer archive.

## Final replay

Final daemon SHA-256:
`e6de524a05f6340c8f150eb559ac869c94e2ca91ebb4e2b50f88a6b116658366`.
`build-origin-final.json` records all four binary digests and 21 changed Rust
source digests. The final bounded release build completed in 2m09s.

| Scenario | Actual result |
| --- | --- |
| Three MiB generated history, actual public cancel/save | PASS: exact retained values; 67,294-byte snapshot and 3,207,745-byte artifact |
| Four MiB generated history, actual public cancel/save | PASS: old representation would be 37,286,566 bytes; new snapshot 67,299 bytes and artifact 4,265,439 bytes |
| Fresh daemon opens artifact-backed cancelled history | PASS: CLI readback; exact values and artifact digest verified |
| Same-length corrupted artifact | PASS: rejected and quarantined before execution; trace unchanged |
| Missing artifact | PASS: rejected and quarantined before execution; trace unchanged |
| 2.77 MB / 18,000-line read through `/plan` | PASS: run `df1b39f6cdf5a44a36280b4b6ea38c6f`, five requests/six tools, all independent markers correct |
| Exact archive retrieval | PASS: four pages at offsets 1,444,000; 2,888,000; 0; and 2,903,925 match original observation bytes |
| Fitting 769,652-byte / 5,000-line default read | PASS: run `372777719ddf29dfb229dad8912ce3c7`, one intact read, three requests/two tools, all independent markers |
| Copied `/home/doll/browser` project | PASS for design, writer and subsequent independent read-only review; 20 requests/25 tools across those actions |
| Complete Rust project qualification | BLOCKED by the fixture's intentional prohibition on builds; its required Rust compile was refused and the candidate retained |

The copied-project run `6d84d371f43c08dc3ad3ab957d1bd50e` produced only
`marker-report.json`. All nine original source files retained their hashes.
The designer, writer and read-only reviewer independently read the evidence
file in three distinct context scopes. No command tool, unit test or repository
verification command ran. The read-only review returned all three exact marker
values and reached ordinary idle; it is not counted as completed D2 pipeline
qualification. The build prohibition was a fixture setup constraint, not a
new product defect or permission to remove product checks.

Two checker expectations were corrected from independently established
contracts: the existing restart behavior intentionally restores next_phase to
Checking, and the CLI reports an unavailable exact run with exit 14 rather than
12. The corruption contract requires quarantine and no effect replay, not a
particular CLI exit category. The actual daemon logs identify the failed digest
or missing file; no bad continuation was admitted.

Replay commands:

```sh
python3 docs/testing/reproducers/history-recovery/prepare-legacy.py ACTUAL_DISPOSABLE_V6_RUN.json CAMPAIGN --filler-bytes 4194304
python3 docs/testing/reproducers/history-recovery/check-history.py CAMPAIGN STATE/workbench-v1/runs/RUN.json
python3 docs/testing/reproducers/history-recovery/check-context.py DOCUMENT ORACLE RUN.json TRACE
python3 docs/testing/reproducers/history-recovery/replay-integrity.py CAMPAIGN DISPOSABLE_XDG_PARENT CONFIG CLI DAEMON ENDPOINT
```

For history replay, retain the matching disposable control store, copy the
generated legacy record into its exact run path, start the actual daemon and
invoke the public `peritus --endpoint ENDPOINT --json runs cancel --run RUN`
command. Save the resulting record/artifact, stop with `shutdown --wait`, then
reopen and query with `runs show`. `replay-integrity.py` consumes the preserved
`fixed-saved-run.json` and `fixed-continuation.bin`; it launches bounded real
daemon scopes, never substituted storage or providers. Use only disposable
state. The generated exact input and binary fixture are retained externally.

## Resources and remaining scope

Builds use two Cargo workers in scopes bounded to four cores and 8 GiB.
Fixtures use four cores and 4 GiB. The user's active daemon and workspace are
preserved during testing. No multi-hour campaign or exhaustive coverage claim.
The final copied-project scope peaked at 300,630,016 bytes (about 287 MiB),
with zero high/max/OOM events. Integrity-replay daemons peaked below 27 MiB.
All owned fixture daemons shut down cleanly; installed-user runtime handling
is separate from these fixture results. The captured user handoff remains
private external evidence and is not committed. These replays fix future
saves; they cannot retroactively recreate an old failed unsaved continuation.
