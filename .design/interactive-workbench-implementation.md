# Interactive workbench implementation record

Tracking: CL-66. Source base: `e6f8adb68972c6fcf96189674f034b260bbdb2f8`.
Scope: the user's subsequent explicit request authorizes all P0–P7 coding, real-TUI
testing, and a final tested commit/push to `develop`. The source design's design-only
authorization statements describe its earlier authoring task, not this implementation.

This record distinguishes delivered code from acceptance evidence. No phase is complete
merely because its command appears in a menu. No user installation or active user run is
part of testing; use isolated QA state and workspaces.

## Historical integration acceptance status (2026-09-09)

| Phase | Status | Evidence / remaining work |
| --- | --- | --- |
| P0 | In progress | Typed local catalog; exact-match-first completion; pasted-command guard; additive bounded doctor request/report with negotiation enforcement; focused diagnostic panel. Unit, IPC, native QA and durable-control foundation tracked below. |
| P1 | Integrated; qualification pending | Queue, images, files/import, brief, context and compaction combined with P2–P4. Four-crate cargo check passed; combined behavior tests and native qualification remain. |
| P2 | Integrated; qualification pending | Goals, accounting and pause/resume combined with P1/P3/P4. Four-crate cargo check passed. Worker demonstrated pause/resume and a one-request budget preventing a second provider call; combined qualification remains. |
| P3 | Integrated; qualification pending | Anchored review feedback, stale rebinding and leave-alone enforcement combined with P1/P2/P4. Four-crate cargo check passed; combined review/goal behavior tests remain. |
| P4 | Integrated; qualification pending | Controlled GUI/capture/behavior flow combined with P1–P3. Four-crate cargo check passed after resolving a response-dispatch merge mismatch. Full combined strict and native qualification remain. |
| P5 | Integrated; qualification pending | Rewind now uses exact committed G4/C1 authority under the authenticated session. Owned-byte restore and independent-edit conflict tests pass. Full/native qualification remains. |
| P6 | Integrated; qualification pending | Forks validate actual saved checkpoint references and load exact historical state while separately fencing the current parent revision. Actual checkpoint/isolated-child test passes. Full/native qualification remains. |
| P7 | Integrated; qualification pending | Session-bound init, original-receipt replay, host-intersected writes, memory and all-feature negotiation are wired. Init service tests and the actual 18-feature IPC init/checkpoint/reconnect flow pass. Full V1–V6/native and strict gates remain. |

Status above is the 2026-09-09 integration checkpoint, not a release qualification.
All workers have handed off; their patches are being combined into this `develop` worktree.
Earlier entries below retain their original incremental scope and do not establish current
combined passes.

## P0 implementation observations

- Doctor request tag 28 and report tag 18 are additive; existing payload tags and bytes
  retain their meanings. `app.product-diagnostics` is separately negotiated and checked
  by the authenticated daemon before dispatch. Schema, TypeScript and fixture generation
  remains driven by A3 sources.
- Current diagnostic checks inspect admitted in-memory configuration and state. They
  make no filesystem probe, authentication request, provider inference, command execution,
  repair or upload. Authentication and capture remain explicitly unprobed/unsupported.
  Launcher binary/readiness and bounded tool/store checks still require integration.
- Panel navigation preserves composer contents, cursor and transcript scrolling. A late
  response cannot reopen a dismissed panel. Malformed/unsupported commands retain drafts.
- Bracketed paste that introduces or changes the command token is inert. Explicit Tab
  selection from the catalog makes a keyboard-owned command; pasted text following an
  already keyboard-entered command token remains an argument.
- Conversation metadata now has nominal IDs, expected-revision operations, authenticated
  actor/workspace binding, and a separate C0 journal generation. One append publishes
  intent, successor root, and original receipt together. Exact replay recovers the original
  receipt without rolling back later state. Read-only inspection does not create the store.
- Additive workbench tags 29–31/19–20 have generated fixtures and separately negotiated
  `app.workbench-control`. The TUI metadata inspector supports explicit create/open/rename/
  pin/archive; it resolves unknown delivery by querying the original receipt after reconnect.
  Full listing/search/fork/run association is not yet implemented; this is not complete P6.
- Launcher binary/version, loaded product-state/configuration, provider-reference and tool-policy
  observations are now provided to the doctor panel. They are explicitly labeled launch-time,
  not live filesystem/authentication checks; opening diagnostics does not rerun bootstrap.

## P1 implementation observations

- The domain ledger retains immutable content revisions and supersession, held/queued order,
  withdrawals, explicit corrections to incorporated inputs, and exact invocation manifests.
  Dependency-breaking reorder/withdraw/incorporation plans reject. Host-only incorporation is
  intentionally not yet exposed as a user A3 command.
- Bounds are 1024 content revisions, 32 dependencies per input, 1024 invocation bindings,
  8192 bytes per input, and 512 KiB total input text per current aggregate. Capacity rejects
  explicitly; no retained input/evidence is silently evicted. Pagination/archive storage
  integration remains needed for full history support.
- Empty ledger fields are omitted from canonical metadata roots, preserving already-written
  metadata-only event/root bytes. New governed runs use a separate `workbench-v1/runs`
  namespace and phase generation rejected by old readers. Legacy control, continuation and
  model-update paths reject governed runs. Legacy migration is not yet implemented.
- D0 now passes the exact fully constructed `ModelRequest` to the host's required
  `prepare_request` port before provider execution. A stale admission rebuilds context;
  failed persistence sends no request. The legacy daemon implementation compares governing
  revision under the same write lock as incorporation persistence and restores prior visible
  state on failure. Governed runs now bind the ledger at this boundary and publish the exact
  canonical semantic request bytes, source-selection manifest, incorporation and original
  receipt in one C0 transaction. Replay verifies source selections against the prior ledger.
  This proves prepared input, not provider receipt or completion. Legacy runs retain their
  prior path. The dynamic governing message replaces its previous version before each request;
  it cannot accumulate withdrawn instructions or be compacted as optional history.
- `/queue` supports enqueue/edit/correct/hold/release/withdraw/reorder and revision-fenced
  pending/history pagination. `app.workbench-inputs` is advertised independently from
  `app.workbench-execution`, which remains unavailable until execution controls qualify.
  Starting a governed run exists internally and is tested through the actual product runner,
  but is not yet exposed to native clients. Queue management never starts inference.
- Missing-feature failure of a receipt lookup does not prove the original command failed;
  the TUI retains that unresolved operation and original draft across reconnect/downgrade.
- Governed runs disable implicit remote semantic compaction. Exact source-aware local context
  inspection and mutable-media withdrawal handling remain incomplete; these are prerequisites
  to enabling governed execution.
- Public host-delivered replies now have immutable digest/length/invocation references and
  exact UTF-8 artifacts published together through C0. A repeated reply cannot rewrite prior
  text. Trailing answers enter governing context only with a later user instruction, preserving
  retry context; the next request manifest includes their source references. Composition fails
  explicitly above 1 MiB before loading an oversized collection of reply artifacts.
- The queue list uses bounded previews with `/queue show <row>` for exact complete text.
  A regression test reaches the final text after 1200 lines, beyond the previous scroll bound.
- Argument completion now handles session actions and role/effort/model command words locally;
  freeform text and manual model identifiers are not rewritten by completion.
- `/context next`, `history`, `show <invocation ID>`, `more`, and `previous` now use an
  independently negotiated read-only query (tag 33) and bounded metadata page (tag 22).
  The next-input view distinguishes eligible, held, withdrawn, superseded, blocked and
  deferred-public-reply sources. It explicitly does not claim a complete next request is sealed.
  Invocation inspection returns the exact request and source-manifest digests, source revisions,
  UTF-8 source digests/sizes, and ordered C5-message role/digest/encoded-size metadata. Raw model
  message content is never returned. Token counts remain explicitly unknown; byte counts for
  input and message rows are not additive. Inner file/media/local-memory lineage and pins are
  still needed to complete AC04.
- Request manifests now have their own digest in the immutable incorporation event, not just
  an artifact-row checksum. Tampered message metadata rejects even when the artifact checksum
  is recomputed. Source text digests are checked against the prior input ledger on replay.
- C5 adds a semantics-preserving bounded canonical encoder. The control archive now enforces
  its 16 MiB ceiling while encoding and reuses that encoding's digest, rather than allocating
  the protocol-wide maximum twice before rejecting an oversized request.
- Store ownership now explicitly unlocks its RAII guard after the journal closes. A deterministic
  duplicated-handle test reproduced the restart-lock failure seen under concurrent process
  tests; dropping only the original file had left ownership held by the other handle.
- User-confirmed brief records are now exposed through negotiated A3/TUI inspection and edits.
  Objective, acceptance, constraints and confirmed-assumption fields bind exact input revisions.
  Editing an unincorporated field uses the existing queue edit transition and preserves holds;
  editing an incorporated field creates a new correction. Ordinary queue edits refresh the
  field's exact source revision. Held/withdrawn fields are excluded from mandatory brief text.
  Brief edits advance the input generation and stale prepared requests reject. The sealed
  manifest records the effective brief bindings and validates them against the prior root.
  Three domain tests and a C0 restart/admission test pass. Agent-proposal/acceptance UI, observed
  fields, explicit clearing, and the rest of AC06 remain unfinished.

## Verification record

- Initial unit pass: A3 22, daemon 96, TUI 63 tests passed after correcting the diagnostic
  no-mutation fixture to compare before/after storage, rather than assume setup creates none.
- Full workspace compile and strict affected Clippy passed for the initial doctor/control
  foundation. Subsequent A3/runner/daemon all-target/all-feature tests passed; TUI 68 unit tests
  passed with metadata receipt/reconnect/draft and all three layout sizes covered. Later input
  ledger focused tests and strict runner/daemon Clippy passed; final full checks remain pending.
- Actual authenticated IPC covers feature rejection, absent read with no journal creation,
  create/rename, stale edit, idempotency conflict, daemon restart, original receipt recovery,
  and absence of an executable run. C0 tests also cover single ownership, wrong store identity,
  SQLite-full failure without partial publication, and both withdrawal/incorporation race orders.
- Native Konsole on isolated Xvfb `:97` was inspected for doctor at wide and 40x12 sizes,
  and metadata create/pin at 80x24 and resize to 40x12. Screenshots are retained under
  `/tmp/peritus-layout-qa.avJYsQ/workbench-{doctor,metadata}*.png`; composer and Esc return
  stay visible. This is Linux/X11 evidence only, not cross-platform qualification or V1–V6.
- AC01–AC18 and V1–V6 remain incomplete as a full-program acceptance set.
- Additional P1 checks: C0 six tests; governed runner two tests; authenticated metadata/queue
  IPC two tests; A3 all-target/all-feature tests and generator `--check` passed. TUI 72 tests
  passed, followed by eight focused workbench tests including the new receipt-query regression.
  Later focused TUI workbench tests passed all nine; C0 passed all eight including disk-full
  request-artifact rollback and reply/restart provenance. Strict affected Clippy and the full
  workspace all-target/all-feature compile passed at the recorded incremental boundaries;
  final checks after subsequent work remain required.
- Native Konsole queue QA exercised enqueue, hold, edit, history and withdrawal at 80x24,
  plus 40x12 resize/scroll with the composer and return hint visible. Screenshots are retained
  as `/tmp/peritus-layout-qa.avJYsQ/queue-{added,held,edited,history,withdrawn-history}-80x24.png`
  and `queue-held{,-scrolled}-40x12.png`. These binaries precede the latest detail-view and
  public-reply changes; final native verification must use the final tested build.
- Latest broad daemon all-target/all-feature suite and TUI 74-unit-test suite passed after
  public reply persistence; strict runner/daemon/TUI Clippy and workspace all-target/all-feature
  compilation also passed. No final full-program or cross-platform acceptance is implied.
- Latest checks: full workspace all-target/all-feature compilation and A3 generator `--check`
  passed after the prepared-request port change. Strict Clippy passed across A3, agent, runner,
  daemon, TUI and launcher after moving the retained UI model onto the heap to bound the
  launch future. TUI 69 tests and D0 31 developer-loop integration tests passed; final full
  feature/platform qualification remains pending.

## Context increment verification

Latest context increment verification: all six new context compatibility fixtures round-trip;
invalid page/seal/view combinations and excessive pre-allocation counts reject. Twelve focused
C0 tests cover exact source metadata, ownership release, corruption, restart and pagination.
The full daemon all-target/all-feature suite passed after the ownership fix (110 unit tests
plus integration suites). The context IPC test and all 77 TUI unit tests passed. C5's full
all-target/all-feature tests passed after bounded encoding. Final checks after subsequent
changes remain required.

Native Linux/X11 context inspection used the rebuilt `peritus-cli` executable, not just the
launcher library. The next-source panel showed superseded and withdrawn revisions with exact
digests at 80x24, and scrolling reached the withdrawn state at 40x12 while preserving the
composer and Esc hint. History correctly showed zero invocations without starting work.
Evidence: `/tmp/peritus-layout-qa.avJYsQ/context-next-80x24.png`,
`context-next-scrolled-40x12.png`, and `context-history-80x24.png`. These captures precede the
ownership-guard and bounded-encoder fixes; final native qualification is still required.

## Brief increment verification

`app.workbench-brief` now negotiates separately. Query tag 34 returns response tag 23 with up
to four canonically ordered fields and exact source rows. Control intent tag 7 edits a confirmed
field; both mutations and original-receipt lookup require the feature. `/brief` inspects without
inference; explicit field edits require the inspected revision and preserve the draft until a
matching durable receipt. Stale rejection invalidates the inspection. Late responses do not
reopen a dismissed panel or overwrite a newer draft. Fields show user-confirmed provenance,
exact source ID/revision, and held/withdrawn status rather than implying every field is eligible.

Six new compatibility fixtures cover query, every field edit, and a full projection. A3's
all-target/all-feature suite passed (30 unit tests plus integrations), including exact round-trip,
pre-allocation bounds, unknown-field, duplicate-source and noncanonical-order rejection.
The authenticated IPC test verifies feature rejection for query/edit/receipt, absent inspection
without store creation, held source editing, stale revision rejection, restart, original receipt,
unchanged historical text and no run creation. All 80 TUI tests passed. Strict A3/daemon/TUI
Clippy, workspace all-target/all-feature compilation and generator `--check` passed.

Native Konsole with rebuilt `peritus-cli` and daemon verified empty inspection, objective edit,
queue hold, and another brief edit retaining the hold and source identity with content revision 2.
The 40x12, 80x24 and 120x38 panels preserve the composer and return hint. Evidence is under
`/tmp/peritus-layout-qa.avJYsQ/brief-{empty,objective,held-edited}-80x24.png`,
`brief-held-40x12.png`, and `brief-held-120x38.png`. This remains Linux/X11 incremental evidence,
not full AC06 or final platform qualification. Explicit attachments, observed fields, agent
suggestions, pins/compaction and P2–P7 remain required.

## Request-boundary media preparation

The D0 live input snapshot now carries image inputs atomically with governing text/revision.
It negotiates image capability for the current snapshot, then replaces the governing message
including its selected media. Stale admission rebuilds that message without retaining withdrawn
image bytes. Governed `ConversationView` explicitly disables ambient role-prompt image discovery;
ordinary legacy image behavior is unchanged. A failed control-binding lookup cannot enable
ambient discovery. The workbench host currently supplies an empty explicit image selection until
the durable attachment importer is implemented; execution is still not advertised.

All 33 D0 developer-loop integration tests passed, including current-image, stale-withdrawal and
unsupported-provider-before-admission cases. Three actual governed-runner tests passed, including
a textual screenshot mention with an on-disk image and a text-only provider: no image was read
into the request. Strict agent/runner/daemon Clippy, full workspace all-target/all-feature
compilation, and formatting passed. This is attachment-path groundwork, not completed AC06.
The brief increment also passed the full daemon all-target/all-feature suite (111 unit tests
and integrations) before this media change. Final native qualification must use the final build.
After the media change, the combined agent/runner/daemon all-target/all-feature test command
passed, including 33 D0 loop tests, 112 daemon unit tests and its integration suites, and 216
runner unit tests plus checkpoint/recovery/production-composition integration suites.

## Explicit image validation increment

G4 now exposes `attachment::ValidatedImage`, constructed only by decoding exact original bytes.
Format is detected from content (PNG, JPEG, GIF, WebP), not a filename. Original media bytes,
SHA-256, encoded size, canvas dimensions and verified frame count remain immutable. No path
reads or authority grants occur in this adapter. The caller still owns authorized reads,
external-path preview/consent, artifact publication, visibility and ledger selection.

Limits are 4 MiB per image (or the lower current provider limit), 16 images and 12 MiB per
complete selection, 8192 per side, 16 Mi pixels per canvas, 64 animation frames, and 128 MiB
aggregate decoded output per encoded image. APNG's default-image decode also consumes the
output budget. Decoder allocation limits are best-effort, not a hard process-RSS guarantee;
strict dimension/pixel/output checks are separate. Import execution must use bounded owned
work, not block the UI or serialized command owner with decoder work.

Architecture decision: a pinned `image` 0.25.10 dependency with only PNG/JPEG/GIF/WebP features
replaces signature-only validation for the new importer. Handwritten multi-format decoders
would add a substantial unreviewed security surface. Defaults, Rayon, AVIF and other format
backends are disabled; no toolchain or lint changes. The license/advisory/source gates pass.
The exact existing-style duplicate exception records `png` 0.18.1's private miniz_oxide 0.8.9
versus `flate2` 1.1.10's private 0.9.1; no public Peritus API contains either implementation type.
This is a focused decoding dependency, not platform clipboard/screenshot integration.

Focused tests decode all four formats, preserve original bytes/digest, reject fake signatures
and truncated pixel data, exercise strict width/pixel limits, verify complete GIF frames and
frame limits, and check provider/count/aggregate selection failures without omission. Full G4
all-target/all-feature tests passed (223 unit tests plus integrations), including the final
decoded-output-budget regression; strict G4 Clippy and full workspace compilation passed.
APNG/animated-WebP end-to-end fixtures remain to be qualified. Importer/TUI integration is
recorded in the later increment below.

C5 also now centrally enforces negotiated per-inline-payload provider limits for images,
audio and documents. A regression first proved that 5 bytes were admitted under a 4-byte
provider limit, then passed after the shared invariant repair. The full C5 suite, strict
C5 Clippy, all 33 D0 loop tests and full workspace compilation passed after that repair.

## Workbench-affected responsibility splits

The architecture gate identified 14 source-layout violations. Twelve were in files affected
by this work and have been resolved by moving coherent responsibilities: public facade exports,
canonical scalar encoding, protocol media-bound tests, pure conversation transitions, D0 input
and terminal helpers, persisted JSON fields, TUI key/paste handling, reviewer prompt projection,
public request response/error mapping, live interaction adaptation, and owned run launch.
Public exports, canonical bytes, JSON fields, request ordering and scope fences are preserved.
The final reply-publication transition has a named validator instead of a new lint exception.

After the splits, full C5/agent/runner/daemon/TUI all-target/all-feature tests passed, including
all 10 canonical compatibility tests, 33 D0 loop tests, 223 runner unit tests, daemon IPC/restart
suites and 80 TUI tests. Strict Clippy passed for all five crates after the final transition
extraction. Full workspace compilation passed. The architecture gate still fails solely on two
unchanged HEAD baseline files: G4 `budget.rs` (470 lines) and `execution.rs` (406 lines), both
over the 400-line soft budget. Neither file nor the source-layout policy was changed here.
These baseline failures are not represented as a passed full architecture gate.
The runner/daemon all-target/all-feature suites passed again after the final reply-publication
helper extraction (223 runner and 112 daemon unit tests plus their integration suites).

## Scoped image import and selection increment

Completed artifact uploads now retain actor/conversation/workspace claims. Preview reads only
an exact claimed artifact through the authority owner, checks declared MIME against decoded
content, validates the selected provider/model/effort and its current revision, and publishes
the exact consent proof. Confirmation binds the original artifact, digest, decoded metadata,
provider revision/model selection and conversation revision. No upload, preview, confirmation,
selection or metadata inspection starts inference or grants filesystem authority.

C0 atomically publishes validated image bytes, caption input, immutable reference, successor
root and receipt. Original operation lookup/replay survives daemon restart and a later provider
configuration change; an unconfirmed stale preview does not. Tampered or missing consent
proofs fail closed. Receipt and artifact publication positions are independently validated.
Disk-full tests verify unchanged roots/receipts/artifacts after failed publication.

G4 retains selected/deselected references, enforces 256 historical references, and captures
the exact eligible image set at the request boundary. Held/withdrawn caption inputs exclude
images independently of their selection preference. The real runner receives exact original
bytes only with an image-capable provider; incompatible providers send no request or input
incorporation. Context manifests distinguish current eligibility/deselection from sealed
historical inclusion. Unknown provider delivery is never displayed as confirmed delivery.

Additive A3 request tags 35–37 cover scoped upload, preview and bounded image inspection;
response tags 24–25 carry preview and image pages. Image intents 8–9 and their original
receipt lookups require the independent `app.workbench-images` feature. Generated schema,
TypeScript and eleven image compatibility cases cover four decoded formats, consent,
selection, receipt lookup and retained-image pages. Page size is 32 with revision-fenced
continuations; row counts are rejected before allocation. Existing wire tags remain intact.

Real restart tests exposed missing B3 registrations for scope claims (3500) and the existing
artifact-upload event (65000). Registering the actual inert event families repairs replay
without accepting unknown families or weakening projection validation. Protocol/projection
suites and authenticated image restart tests pass after regeneration.

The TUI `/attach` panel inspects retained images; `i` opens explicit import and path/caption
editing, `p` reads/uploads/previews, and `c` confirms the exact preview. Path reads are owned,
bounded, regular-file-only operations, with no background clipboard scan. Final-component
symlinks/reparse points, relative paths, parent traversal, empty and oversized files reject.
Absolute imports are snapshots, not a claim of workspace-constrained traversal; workspace
`@path` authorization and refresh remain separate unfinished work. Upload advances only on
matching chunk acknowledgements. Reconnect resolves the original confirmation operation;
late reads and mismatched previews cannot silently start another import or overwrite drafts.
Left/Right select a retained row, Up/Down scroll one line, Space changes future selection,
and `n`/`b` request revision-fenced pages. A native 40×12 check found that reserving Up/Down
for row navigation made some details unreachable; the new key separation has a regression
test checking every identity/state section through one-line scrolling.

Verification includes full A3/daemon/TUI all-target/all-feature suites (36 A3, 125 daemon,
93 TUI unit tests at the first full pass, plus integrations), strict affected Clippy, full
workspace compilation, six authenticated image IPC cases, owner/workspace/stale inspection
fences, a 33-image/two-page test, and the later narrow-scroll regression (94 TUI tests total).
Windows GNU cross-compilation passed for the TUI and all its test targets; native Windows
execution is not claimed. The architecture gate retains only the two unchanged baseline
source-size failures described above.
The final combined rerun after the pagination and narrow-scroll regressions passed all
36 A3, 126 daemon and 94 TUI unit tests, all their integration suites, formatting and
generated-protocol checks. Strict affected Clippy passed with these source changes.

Native Linux QA used only isolated state under `/tmp/peritus-layout-qa.avJYsQ` and Xvfb `:97`.
The explicit system icon import preview displayed its exact 1273-byte PNG, 32×32 dimensions,
digest, provider revision and model; confirmation published revision 10 and an image/caption
reference without starting inference. Deselection published revision 11 and persisted across
daemon restart. Native 40×12 scrolling exposes both selected=false and eligible=false;
80×24 preview/confirmation and larger-screen retained metadata were also inspected. These
are image-increment checks, not the complete V1–V6 or P0–P7 acceptance gate. No commit/push yet.

## P1 scoped file-reader substrate (integration pending)

C1 now owns a root-bound `FolderInspection` capability. Every relative path component is
opened from its parent directory handle without following symlinks; regular-file checks
reject special nodes, and nonblocking opens avoid waiting for a FIFO writer. Root identity
is checked against the opened handle and again before returning bytes. Source metadata,
reopened identity, complete SHA-256, resolved byte range and selected digest accompany the
bounded read. This is an observation, not an atomic snapshot against arbitrary writers.
Whole-file selection retains the 8 MiB bound. Explicit byte/line ranges can select from a
source up to 64 MiB, with complete-source hashing and no silent truncation. Existing C1
read-only file callers use this same implementation; directory listing is not claimed to
have received equivalent handle-relative hardening.

The platform dependency decision pins cap-std and cap-fs-ext 4.0.3 with default features
disabled. Their directory/file handles stay private; no new Peritus unsafe code is needed.
The alternative was separate Unix openat and Windows native-handle implementations owned
by Peritus, with a larger platform-specific trusted surface and verification burden.
This is a tested external substrate, not new Verus-verified filesystem behavior.

Reviewed transitive policy exceptions are exact: io-lifetimes 2.0.4 is mandatory through
fs-set-times 0.20.3 alongside cap-primitives/io-extras' version 3; windows-sys 0.52.0 meets
winx/fs-set-times' pre-0.60 constraints alongside existing version 0.61. The published winx
0.36.4 LICENSE was inspected in full; its declared Apache-2.0 WITH LLVM-exception is allowed
only for that version. Global duplicate denial, license policy and source/advisory checks
remain unchanged. Release attribution must include this new dependency closure.

The host integration below authorizes actor/workspace/protected-path policy.

## P1 workspace-file workflow

Workspace-relative file references now run through explicit preview, confirmation, retained
selection, and request admission. `/files <path>` and a standalone `@path` open an inert draft;
the user chooses whole text, one-based inclusive lines, or a half-open byte range, snapshot or
refresh-on-request mode, and a caption. Preview shows exact source/selection digests, resolved
range, folder identity and provider/model revision. Confirmation atomically publishes exact
UTF-8 bytes (including original line endings), consent, queue input and receipt in C0, without
starting inference. Selected text is capped at 256 KiB, with bounded count and aggregate bytes;
invalid encoding and binary controls reject rather than being silently transformed.

The real runner includes only eligible selected versions, seals them into the request manifest,
and retains original versions after refresh. A changed refresh observation causes a stale
preparation/rebuild before any provider send. Snapshot selection continues using its confirmed
bytes. Hold, withdrawal and deselection exclude future inclusion without rewriting old requests.
Selection and exact operation receipt replay survive restart and later source/provider changes.
Outside-workspace text import is still pending; this workflow grants no ambient external reads.

New A3 file operations have independent feature negotiation, exact wire descriptors and nine
compatibility fixtures. Verification: full A3/daemon/TUI all-target/all-feature suites passed
(37 A3 and 98 TUI unit tests plus daemon and all integrations); four TUI file workflow tests
cover inert paste/ranges, exact consent, stale response and reconnect recovery, and 40x12,
80x24 and 120x38 rendering. Authenticated IPC checks preview, confirmation, unsupported client,
restart/replay, retained source digest and deselection while a sentinel provider is never run.
The actual governed runner test exercises both snapshot and refresh modes with a scripted
provider: one request, the right selected version, no unselected lines, and preserved history.
Strict four-crate Clippy and full workspace all-target/all-feature compilation passed.

Native Linux evidence is under `/tmp/peritus-layout-qa.avJYsQ`: `files-preview.png`,
`files-confirmed.png`, `files-deselected.png`, `files-context-deselected.png`,
`files-restart-40-selection.png`, and `files-restart-wide.png`. The confirmed source is 115 bytes,
range 32..76 selects 44 bytes, operation 8358a04720757470ee1aa8cbc07691ad. Confirmation revision
12 and deselection revision 13 were observed; native restart retained the exact metadata and
exclusion. The context display's image-only deselection wording was corrected to attachment
deselection. These are Linux workflow checks, not native Windows/macOS or full-program acceptance.

## Combined verification checkpoint, 2026-09-09

All phase patches are now combined in the `develop` worktree. The observations above are
incremental history, not a statement that their earlier unfinished items remain unchanged.
The final acceptance set is still open; no implementation commit or push has occurred.

Verified against the integrated source at the respective commands:

- Workspace all-target/all-feature compilation passed after the daemon launch module split.
- A3, C5 and G4 all-target/all-feature tests passed again after explicit identity serialization
  replaced source-generating identity macros. The identity regression verifies unchanged raw
  byte-array JSON, valid round trips, and rejection of zero and malformed identities.
- The daemon all-target/all-feature suite passed: 147 unit tests, one explicitly ignored native
  fixture, and 42 integration tests. These precede the final Tetris qualification fixture.
- All 128 TUI unit tests passed, including authoritative snapshot refresh for memory/init,
  graphical-goal draft confirmation/feature-loss fencing, and responsive panels.
- Strict affected-crate Clippy, architecture checking, A3 generated assets `--check`, maintained
  documentation checking, and cargo-deny bans/licenses/sources passed. The complete workspace
  strict rerun is still pending final native-fixture cleanup.

Rebuilt native Linux/X11 TUI verification used isolated state, not the user's installation.
The exact flow reopened conversation revision 8, enqueued an instruction at revision 9, then
saved guidance after `/memory` refreshed the authoritative snapshot. `/init` automatically
observed revision 10 after that save. No manual reopen was needed between these panels. Init
decline left the absent `AGENTS.md` absent, and the reviewed diff remained scrollable at 40x12.
A graphical criterion was added only to an unconfirmed local goal draft; no live account
provider was invoked by this manual UI pass. Evidence is retained in
`/tmp/peritus-layout-qa.avJYsQ/verified-{queue-before-memory,memory-refresh,memory-save,init-refresh,init-scroll,goal-criterion}*.png`.

The separate test-only Tetris fixture implements design scenario V1: a scripted image-capable
writer uses the real pipeline to create a GUI candidate, which is launched, controlled,
captured and given source-bound feedback. It is not a Peritus application feature or bundled
game. Native graphical-goal qualification exposed a durable-trace failure before preview;
the later checkpoint below records its repair and the successful explicit native reruns.

Formal policy checks are not green. A clean export of source baseline
`e6f8adb68972c6fcf96189674f034b260bbdb2f8` independently reproduces 41 ordinary-API findings
and 3046 trust/proof-impact findings. Current changes introduce additional findings, including
serialization forms not modeled by the source checker, native-test attributes, and new
dependency/source proof-impact records. Straightforward new source findings have been repaired
without changing the checker. No proof-impact approvals or successful formal verification are
fabricated. Reproducibility checking also requires reconciling the exact reviewed pinned
dependency exceptions with its policy copy. These gates remain explicit handoff blockers.

## Native composition and final gate checkpoint, 2026-09-09

The real graphical run exposed a tool-accounting identity collision: independently constructed
designer and implementation loops reused the same local tool sequence. The durable key now
includes the exact developer-loop request prefix and role, as well as the existing goal attempt
and sequence. Exact replay remains idempotent; a changed mutation classification conflicts.
The global host-operation identity domain is unchanged. A legacy-key fence rejects ambiguous
pre-v2 tool records before appending or charging, rather than silently reusing or double-charging
them. Compatibility/reopen tests are part of the focused final repair verification.

Both explicitly invoked Linux-native fixtures passed, including the complete V1 composition:
scoped GIF import/preview/confirmation, all eight image-capable writer requests, normal Build
and reviewer flow, managed candidate launch, observed LEFT/RIGHT/ROTATE/DROP, exact selected
X11-window capture, process stop, behavior receipt, and source-bound KeepBehavior feedback.
Both mandatory criteria become satisfied and the goal becomes Achieved. The preserved capture
is `/tmp/peritus-v1-evidence.9Mcm1r/tetris-v1.png`, 300x478 pixels, SHA-256
`f44b800b9279a1a5605d2cf7ff6ba105a3ca0fbf93d8e76b217a39be41940182`. The visual check confirms
the dropped cyan piece and `DROP x=3 y=14 rotation=1`; source and identity metadata are beside it.
These are scripted-provider qualifications, not a live-account provider campaign.

The complete workspace all-target/all-feature test rerun reported 2722 passed, one failed, and
nine ignored across 410 reported suites. Its one failure is xtask's
`policy_commands_discover_the_workspace_from_a_member_directory`, which invokes the failing
formal API gate. This run preceded the final legacy-fence regression additions; later focused
checks qualify that repair separately. Strict workspace Clippy passed before those additions.
The latest ordinary-API check reports 73 findings versus 41 on the clean source baseline;
trust/proof-impact reports 3358 versus 3046; reproducibility reports one exact `deny.toml`
reviewed-policy mismatch. Native success does not waive these gates. No checker allowances,
proof-impact approvals, commit, or push are implied by these results.

The user also requested removal of old inactive worktrees and Cargo cleanup. The 23 older
worktrees were checked for active processes, all HEADs were retained by existing Git refs, and
full source/local-metadata archives (excluding top-level Cargo output) were compared against
the originals before removal. Private recovery archives and their HEAD/ref manifest are at
`/home/doll/Project-Peritus-worktree-archive-20260909-WIZk0I`. The seven phase-worker copies were
also archived, compared and removed, retaining their branch refs: 30 worktrees removed in total.
Cargo cleanup completed: the dedicated `target-c1` directory removed 1333 files (704.8 MiB),
and ordinary `cargo clean` removed 56911 files (91.9 GiB) from the integrated target directory.
Both target directories are absent. Two leftover historical handoff-evidence directories were
moved intact into the recovery archive, leaving only `develop` under `.worktrees/`. The active root,
integrated develop worktree, coordination caches, and application-owned workspace are retained.

After the legacy fence and D0 propagation regression landed, verification passed again:

- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- `cargo test -p peritus-daemon --all-targets --all-features --locked`: 191 passed,
  zero failed, two explicit native fixtures ignored in the ordinary suite (both separately run).
- `cargo test -p peritus-agent --test developer_loop --all-features --locked`: 34 passed.
- `cargo fmt --all -- --check`, `git diff --check`, architecture/source-layout checks
  (82 packages, 4119 source files), and docs-check (151 documentation files).

The rebuilt native TUI again opened the retained conversation and displayed the unconfirmed
goal draft. Writer/reviewer/fixer labels correctly say `configured model` rather than being
blank; the existing route remains unchanged. Evidence:
`/tmp/peritus-layout-qa.avJYsQ/final-binary-goal.png`. The launcher/daemon/TUI build passed;
before Cargo cleanup, the inspected binary SHA-256 values were:

- `peritus`: `811f5015856071f72b93c29af9dadd0a43ce5d94de829bc0537e91d8f4d1c31c`
- `peritusd`: `8dc51da0d6acd26deb74ef8eb02cf87acc91297546a60adac091b336c8c63d47`
- `peritus-tui`: `315491bcadb79a8075166442691d7f8c3715b77f6f860964e58a5d8eb2390426`

## Shipping repair verification, 2026-09-10

The user extended scope to repair the formal gates and deliver on `develop`. The ordinary-API
checker now models the exact pinned serialization and async macro forms with adversarial
regressions, and the dependency policy records exact reviewed pins. Formal configuration uses
the same data/control semantics as ordinary builds. These repairs do not waive proof-impact
review: the final frozen candidate still requires the complete ordinary/Verus evidence matrix
and an independent candidate-bound review before authorization and implementation publication.

The final correctness pass also tightened the implemented workbench behavior:

- Read, write, process, and network permission narrowing is enforced at admission and actual
  effect boundaries. Ordinary runs without workbench governance retain their existing policy;
  missing governed state fails closed.
- Forks retain the exact historical conversation seed, including frozen selected-file bytes.
  Isolated writable children require disjoint registered roots and matching checkpoint-covered
  before-images. Read-only children can chat, with an explicit parent reservation when governed.
- `/rewind <checkpoint-id> [files|conversation|combined]` binds the selected mode into the
  confirmed preview. Logical modes create a new read-only historical branch, accepting optional
  `time`, `requests`, `tools`, and `tokens` allocations. Conversation-only mode does not restore
  files; combined mode publishes its branch only after successful file settlement.
- Restore settlement invalidates context and current goal evidence without appending a synthetic
  provider input. The full-message-ledger regression passes, so message capacity cannot prevent
  recording a completed filesystem effect. Cumulative usage is preserved.
- Graphical qualification requires ordered, observed interaction/capture evidence associated
  with the exact selected criterion; generic preview success does not satisfy every criterion.

At this checkpoint the full protocol tests, full TUI tests (132), focused restore-capacity test,
and strict Clippy for protocol, runner, daemon, and TUI pass. Subsequent recovery/enrollment
changes and the frozen formal campaign must be requalified; earlier counts are not final claims.

## Required final handoff

The final shipping source includes automatic per-path before-images captured before admitted
workspace effects, including newly enrolled paths and the declared scope of process execution.
Automatic checkpoints carry host-created run provenance. Successful writes seal only their
exact owned post-images; later writes in the same run update that post-image without changing
the original before-image. Arbitrary external effects and empty-directory recovery remain
explicit exclusions, not claims of whole-machine rollback.

Restore receipt queries are observational. Authorized retry reconciles a durable Prepared
operation against C1 transaction/consumption evidence, and inconclusive recovery remains fenced.
Prepared logical branches reserve their child IDs durably; publication uses the exact original
restore binding. Historical queued inputs are held in forked branches until explicitly released.

Record immutable source and binary digests, exact successful commands and any unrelated
baseline failures, native terminal/platform evidence, and the complete acceptance matrix.
Commit only reviewed design/source/generated/test artifacts; exclude local integration
metadata. Push to `develop` only after the full requested implementation and checks.
