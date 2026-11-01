# Stage 4 native evidence admission, freshness and portable bundles

Final bounded qualification is recorded in `../STAGE4-REPORT.md` and `../inventory/stage4-targets.json`; earlier worker-status notes describe their historical snapshot.

This is a bounded public consumer of `peritus-evidence`, the actual `SqliteJournal`, and the actual `ArtifactStore`. Each checker owns a new temporary directory, eight committed journal events, four finalized artifact files and a shared SQLite database. It never opens an application/user database. Source implementation is drafted; compilation, strict lint, final-source searches and mutation qualification are parent-owned steps, not completed claims here.

## Contract and caller coverage

Read the state/evidence/sqlite/bundle/manifest navigation maps, public `lib.rs`, `record.rs` construction/canonical encoding, `freshness.rs`, `invalidation.rs`, `causality.rs`, `admission.rs`, `admission_work.rs`, `artifact.rs`, `provenance.rs`, SQLite `store.rs` admission/load/freshness/invalidation/causal snapshot and `row.rs` journal/load observations. Read bundle `plan.rs`, format ancestry validation, manifest canonical construction/validation, preparation staging, assembly/copy/output cursor and verification state machine/EOF boundaries. The generator never uses the production `verified` predicates or graph/record/manifest encoders as truth.

Relevant current callers inspected: `peritus-eval/src/runtime/publication.rs` recognizes committed retry before obtaining a journal export, otherwise admits an artifact-backed report, then independently settles publication through C0. Daemon `authority/owner/runtime.rs` opens the real evidence store for evolution/debugger publication; daemon startup opens evidence on its shared database. These are selected public/caller/error paths, not every application evidence consumer or a whole-package audit.

The exact-retry distinction is intentional: a previously committed draft returns its immutable receipt before journal-export or artifact-byte work. It does not assert current authority, current dependency availability or current revision. Present authority is separately checked through freshness/bundle planning. The target explicitly corrupts a referenced file and supplies an older empty export for exact retries; the original receipt must still return, while changed content for the same identity must conflict.

## Independent reference model

A small map of four logical evidence identities stores only synthetic input slot, parent-bitset and payload byte. A separate invalidated-identity set models retirement. Parent validation uses existence and strict numeric producing-position order; it does not call the owner's causal validator. Generated interactions include missing and newer parents, self-cause constructor rejection, exact retries versus different-parent/payload conflicts, and continued admissions after rejection. Default graph waves form a diamond with shared ancestors. Random histories can construct other legal small DAGs.

The fixture owns the source content and independently constructs expected B3 bytes, event/batch/head SHA-256 preimages, complete canonical record bytes, exact record digests, selected causal closure, complete canonical manifest bytes and complete bundle stream bytes. Binary encoding correspondence follows the frozen documented wire contract; ordinary SHA-256 is a primitive, not a cryptographic-security proof. `revision_digest` is used only to construct journal inputs; expected record/bundle revision bytes and freshness comparisons are independently expressed from the seven synthetic tuple components.

Every accepted or reloaded record must exactly match all independently constructed canonical bytes, including its journal event/batch/head/frame/schema/revision binding, payload, artifacts and causes. Every rejected binding identity is queried as absent. Missing producer, differing artifact reference set and differing revision are separate typed rejection partitions. The native producer commits real `ArtifactDependency` references, not fake metadata rows.

Each new ordinary record admission advances a real `EvidenceAdmission` owner through exactly three artifact bytes, cancels without changing its retained hash cursor/completed count, then resumes the same owner through actual commit or causal rejection. Existing receipts take the documented retry path. This tests retained progress without holding a fake storage transaction.

Freshness samples current revision plus each of acceptance specification, harness, workspace identity, fencing generation, workspace revision, policy and provider-profile drift separately. The invalidation model dominates every revision comparison. Invalidation operations prove the actual later journal event, distinguish non-later/wrong-event/missing-target rejection, and preserve immutable record bytes across real close/reopen of all three owners.

## Bundle closure and streaming obligations

A list/stack traversal independently computes the exact transitive parent closure of selected authority IDs. Authority records must have the selected revision; historical parents may drift. Any durable invalidation in either role rejects the whole plan. The reference encoder independently derives exactly the sorted record, frame and artifact sets, their original input bytes/digests and authority IDs; v1 all-authority and v3 historical manifests are both reached. Repeated planning must be deterministic.

The full native assembled stream must equal independent canonical bytes, and its receipt must match independently computed size, root/export identity and full-stream SHA-256. The public standalone verifier must accept that stream, reproduce the expected manifest and reject wrong magic, truncation, a flipped interior byte and trailing data. The framed verifier is first advanced through a partial field, cancelled without consuming more bytes, then resumed; it must stop exactly at the authenticated trailer and retain a following `NEXT` transport sentinel.

The output-failure writer accepts a known prefix and then returns a real I/O error. `BundleExportCursor` must equal precisely the bytes accepted. A fresh `resume_bundle` invocation must yield the exact remaining suffix, with no duplicate or omitted bytes. Separately, an owned export is advanced, cancelled with progress preserved, dropped and restored from its exact cursor; completing it must reproduce the canonical stream.

Artifact corruption happens to the actual finalized object, before assembly. The target flips one byte without changing file length. Private authentication must reject `CorruptArtifact` before touching caller output. The original content is restored and the same plan must then assemble and verify successfully. This is a concrete source-authentication boundary, not a simulated error or an assertion that planning authenticates artifact bytes.

## Bounds, counters and calibration

Histories contain at most 48 requested operations; four records, eight fixed producer/invalidation events and artifacts of 64–85 bytes keep the model cheap. Constructive generated schedules first admit a parent at producing position two, then attempt to make it the cause of a new record at position one: the actual newer-parent rejection must occur, followed by legal graph construction and continued bundle work. These schedules also deliberately reach historical-parent export, invalidated closure rejection, broken-dependency exact retry and actual reopen. Counters separate the seven freshness axes, causal rejection partitions, binding/invalidation failures, admitted records/retries, shared database reopens, exact closure sizes, historical drift, source preflight failure, partial output resumption and cancelled admission/export/verification continuation.

`calibration/stage4-evidence.patch` deliberately omits provider-profile equality from the real evidence revision comparison. `calibration()` selects only that changed axis and expects `RevisionStale(ProviderProfile)`. Broad ordinary/constructive strategies repeatedly generate axis seven. Parent qualification must compile the actual mutation, demonstrate both exact and generated failures under two seeds, restore the owner and pass retained witnesses. No mutation has been applied by this worker.

## Explicit limitations

The target does not qualify v2 large-count encodings, representation ceilings, selected BundleLimits boundaries, every malformed record/manifest permutation, artifact quarantine/GC, catalog containment and repaired quarantine recovery, arbitrary blocking transport interruption, threaded races, read/write failures at every staging byte, `publish_bundle` filesystem no-clobber/fsync behavior, process kills or power loss. The concrete retained-progress and close/reopen schedules are not a substitute for those gaps. It also does not claim signatures or transport authenticity; the bundle supplies deterministic integrity only.
