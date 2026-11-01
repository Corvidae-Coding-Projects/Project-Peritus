# Stage 4 artifact catalog, streaming writers, references and collection

Final bounded qualification is recorded in `../STAGE4-REPORT.md` and `../inventory/stage4-targets.json`; earlier worker-status notes describe their historical snapshot.

The subject is the real public `ArtifactStore`, its actual SQLite catalog and actual files in a
new temporary directory for every case. This target combines independent quota/GC tables with
an actual six-object content/reference ledger and controlled recovery windows. The separate
`artifact_stream` target qualifies ongoing owned-reader integrity; the stable-content reads here
do not establish detection of corruption introduced during a stream.

Selected owner files read completely: `lib.rs`, `config.rs`, `digest.rs`, `quota.rs`, `gc_plan.rs`,
`references.rs`, `metadata.rs`, `writer.rs`, `owned.rs`, `store.rs`, `catalog.rs`,
`catalog/{schema,value}.rs`, `finalize.rs`, `path.rs`, `recovery.rs` and `verified.rs`.
Caller segments inspected include daemon startup's actual shared journal/artifact/evidence opens
and product-runner local-context storage's shared database/ownership/checkpoint setup. These
segments are orientation and selected caller coverage, not a claim of complete application-file
or package review. Evidence admission and typed journal references are separately exercised by
their own Stage 4 campaigns.

## Independent models

Quota expectations use exact `u128` addition before comparison with the public `u64` representation
and optional selected limit. These are arithmetic cases without huge allocations. The GC table
uses raw logical entries, a separately formed root set and direct state/action expectations:
unmarked active -> quarantine, marked quarantine -> restore, strictly older unmarked quarantine
-> delete, same-generation unmarked quarantine -> retain. Duplicate inventory, absent roots,
zero/future quarantine generations reject. Root membership never excuses a future generation.

That last partition exposed a confirmed public validation defect. The original implementation
checked future generations only in the unmarked branch. A rooted future entry returned Restore
despite the documented InvalidCollectionPlan contract. A readable public example fails before
the focused common validation guard and passes afterward, preserving same/older-generation
restoration. This establishes invalid-plan acceptance, not actual artifact loss or a demonstrated
ordinary caller path into that state.

History cases own six distinct payload families, exact SHA-256 identities, quarantine generations
and journal/evidence owner sets. They drive borrowed and owned chunked writers, duplicate content,
wrong size/digest, poisoned oversize writes, abandoned writers, quota exhaustion, references,
saved/stale collection plans, reads and real close/reopen. A private `WriteObservation` separates
actual library rejection from checker failures so a failed assertion cannot count as an expected
negative result. Temporary writers are dropped explicitly; no leaked live writer is relabeled
as crash recovery.

Every created store GC plan is compared with an independent action/mark table over the current
ledger before it is retained. Applying a saved plan models the exact successful action prefix
before a stale-action rejection. A plan is not asserted globally atomic: each action has its own
recoverable durable boundary. Constructive histories create a plan, add a reference, require the
stale quarantine to refuse, remove that reference and continue using the plan. After every
operation, current catalog membership/state, logical quota, canonical roots and stable-content
bytes are compared with the model. Stream offsets, exact bytes and rejected zero chunk bounds
are also checked.

## Controlled recovery windows

These are different mechanisms and their activation is observed directly:

- A real SQLite trigger rejects catalog insertion after normal object publication. Exact active
  bytes must exist with no catalog row; reopening quarantines the orphan, a later recovery removes
  it. The trigger is removed before reopening.
- A filesystem obstruction prevents the quarantine move after metadata commits. Both quarantined
  metadata and the still-active exact source file must be observed. Removing the obstruction and
  reopening completes the move.
- A finalized file is corrupted without changing size. Ordinary reads reject, reopening durably
  contains it, and a second reopen preserves non-referenceability.
- An explicitly constructed restore image moves a healthy active-catalog file into the quarantine
  namespace. Actual reopening restores its exact bytes. This image is constructed by the driver;
  it is not claimed to be a process killed inside `apply_gc_plan`.

Repeated recovery checks every public report field, including removed swept files. Tests here
qualify statement/filesystem failures and reopen, not power loss or all concurrent filesystem
interleavings. The ordinary history cannot create a referenced+quarantined row through the public
add-reference API, so that native Restore/reference cross-product is not established by its
history count; the pure marked-Restore table is exercised separately.

## Calibration and bounds

`stage4-artifacts.patch` deliberately permits deletion in the same generation as quarantine.
The fixed witness and a generated generation-edge family must detect the wrong Delete action.
The family varies generation, key, size, root membership and current/future relationships.
Calibration requires a successful mutant build, generated detection under two seeds, stale-build
refusal, exact source restoration and rebuilt saved-witness replay.

GC collections contain at most 64 entries in direct replay; ordinary generation is much smaller.
History payloads are at most 41 bytes, per-object bound 64 and selected quota 96 or absent.
Recovery payloads are small. Histories cap at 64 operations and honor lower requested bounds.
Native evidence is Linux only; large real-object limits, cross-store live writers, arbitrary
symlink/ACL races, actual crash/power-loss cuts and other operating systems remain separate work.
Final evidence selection and its source hashes determine qualification, not this document alone.
