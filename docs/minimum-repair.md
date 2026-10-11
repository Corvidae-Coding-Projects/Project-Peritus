# Minimum repair

Mapped to the current `develop` baseline, commit `90925794845121dd9278d06c5eec92a5bbfd85e9`, on `fix/minimum-repair`. All 718 original findings are grouped below by category and shared repair. Each finding identifies its current implementation and the proposed minimum change. Historical finding titles are retained for traceability; the current-code notes correct differences in this baseline. These are proposed repairs, not implemented changes.

Arbitrary file, data, representation, count, and resource limits come first, followed by deadlines and the remaining blockers. Related findings stay together under their shared fix. The 500-line source-file lint stays in place to prevent god files.

## 1. File, attachment, artifact, and workspace size limits

### Read large sources without imposing source or absolute-range ceilings

Keep exact selected bytes, source digests, metadata drift checks, and no-follow file handles. Remove absolute source/range ceilings, and let the caller's requested response size remain a page bound. Full-file checkpoint capture and bounded interactive previews must select their appropriate existing read mode.

#### L007 — Independent source-scan and included-content ceilings

**Current code:** `crates/runtime/peritus-workspace/src/scoped_inspection.rs` — `MAX_INSPECTION_SOURCE_BYTES`; `scoped_inspection/read.rs` — `FolderInspection::read_file`, `Scan::new`, `Scan::accept`; `scoped_inspection/selection.rs` — `FileReadSelection::bytes`, `lines`; `inspection.rs` — `MAX_INSPECTION_FILE_BYTES`.

**Proposed minimum fix:** Delete the 64 MiB source/byte-offset and 67,108,864-line cutoffs and the universal 8 MiB caller-bound clamp. The reader already hashes in 64 KiB chunks; retain that loop rather than writing another hashing reader. Use checked/wider line accounting when removing its old bound, and offer continuation for an oversized requested selection instead of silent truncation.

### Expose complete workspace evidence through bounded continuations

Keep bounded replies but return exact continuation positions and omissions. Remove whole-file exclusion policies; do not treat a truncated prefix as a complete observation.

#### L041 — Workspace inspection truncates or omits evidence

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/inspection.rs::{list,search,read,read_line_range}` has depth/entry/match caps, silently skips large search files and child errors, and sends reads through the separate output prefix truncator. List/search currently have no continuation cursor.

**Proposed minimum fix:** Add continuation to list/search, stream large search files, and report skipped entries. Return actual last displayed line plus a byte continuation for oversized individual lines; the existing line range alone cannot recover a line that exceeds the byte bound. Keep bounded response pages.

#### L045 — Tool output and filesystem-coverage exclusions

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/effect.rs::limit` cuts output at 512 KiB. `path.rs::ignored` excludes build/dependency directories from traversal, while `checked` preserves workspace/traversal/symlink authority. Deletion is redirected to the exact-target removal tool.

**Proposed minimum fix:** Make prefix output resumable under the inspection repair and permit explicitly selected ignored-directory inspection. Keep path authority and the grounded deletion route; same-session deletion authorization is addressed separately.

#### L124 — Artifact navigation samples 2,000 entries after collecting entire directories

**Current code:** `crates/app/peritus-product-runner/src/design/artifact.rs::inventory_with_limit` collects/sorts a full directory before applying the 2,000-entry sample boundary and aborts on child errors.

**Proposed minimum fix:** Enumerate incrementally with cancellation and paged continuation; report inaccessible entries individually. Keep the sample labeled partial and reuse the shared workspace inspection continuation.

### Filesystem tool scale, continuation and exact mutation handoff

Use ranged/paged access sized by encoded bytes, keep every retained result reachable and restore mutation outcomes through existing operation evidence.

#### L634 Filesystem schemas impose whole-file and inline-mutation ceilings without ranged alternatives

**Current code:** `peritus-tools-fs/src/input.rs` caps traversal depth/entries/search bytes and fs.read at 48 KiB; schemas/decoder expose whole-file and bounded inline mutations.

**Proposed minimum fix:** Add fs.read ranges and traversal/search continuation, remove artificial whole-job quotas and use existing artifact input for larger mutations. Keep transfer pages representable and mutation preimages exact.

#### L635 Filesystem traversal/search loses progress at bounds and silently excludes some search scope

**Current code:** `peritus-tools-fs/src/read.rs::{discover,search,collect_matches}` builds full traversal, skips large/binary files, fails at aggregate/match caps and prefixes previews to 512 bytes.

**Proposed minimum fix:** Return continuation plus explicit skipped/depth scope, stream actual reads and center previews on matches. Isolate unrelated unsafe entries while preserving no-follow protection.

#### L636 Filesystem result windows expose only the first 500 items and can still fail the canonical JSON ceiling

**Current code:** `peritus-tools-fs/src/render.rs` takes the first MAX_RENDER_ITEMS=500 before canonical JSON construction; protocol JSON/envelope limits can reject that window.

**Proposed minimum fix:** Size each page by actual encoded bytes and expose continuation/full artifacts. Remove independent JSON policy clamps consistently; preserve exact numeric values and file content.

#### L637 Filesystem dispatch is synchronous, loses error distinctions, and retains candidate handoff only in its live object

**Current code:** `peritus-tools-fs/src/dispatcher.rs::start` executes synchronously, uses start time as completion time and retains MutationOutcome only in an Option.

**Proposed minimum fix:** Route work through owned cancellable tool execution, preserve typed filesystem/workspace recovery causes and observe actual completion time. Reconstruct exact mutation handoff from L582's retained operation evidence.

### Hash remembered files without a total-byte cutoff

Use the existing streaming hasher; its buffer size is not a file-size limit.

#### L076 — Remembered-file hashing has a 64-MiB synthetic stop

**Current code:** `crates/app/peritus-product-runner/src/local_context/memory/environment.rs::digest_file` already streams in 8-KiB buffers but returns an error after 64 MiB.

**Proposed minimum fix:** Delete the total-byte refusal, retaining checked byte arithmetic and digest validation. Add cancellation/yielding through the existing refresh owner where needed; no alternate hash store is required.

### Continue exact memory retrieval without source-size or packing blockers

Use existing scope-bound cursors and offsets, stream source ranges/search, and charge actual encoded response bytes.

#### L078 — Memory retrieval has small pages, scan caps, and conservative byte packing

**Current code:** `crates/app/peritus-product-runner/src/local_context/tools/read.rs::{execute,source_page,append_source,state_page,cursor}` imposes request/query/handle/cursor limits, scans 32 sources and 64 MiB per page, loads full artifacts, and packs source text at one sixth of remaining bytes. A large state entry cannot be partially returned.

**Proposed minimum fix:** Remove independent query/handle/request policy maxima; keep bounded pages with explicit continuation. Stream search/ranges, continue later matches within a source, and return partial state entries when needed. Pack actual escaped bytes rather than a worst-case sixfold estimate.

### Checkpoint capture and patch restore capacity

Remove the fixed file, aggregate-payload, operation-count, and atomic-install admission ceilings together. Reuse the existing checkpoint body rows and C1 patch transaction; do not add a second checkpoint store. Patch identity encoding, manifest decoding, journal state admission, and SQLite length admission must accept the same sizes. Keep nonempty operations, exact preimages, protected paths, checked arithmetic, and one atomic publication.

#### L003 — Checkpoint capture inherits patch byte ceilings

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/capture.rs` — `capture_selected_coverage`, `capture_checkpoint_paths`, `observe_path`; `capture/automatic.rs` — `capture_automatic_checkpoint_for_operation`, `seal_checkpoint` still use the 8 MiB patch policies.

**Proposed minimum fix:** Remove the aggregate comparisons and stop passing `MAX_FILE_BYTES` as a checkpoint storage policy. Return the original `WorkspaceError` for failed inspection; reserve `StaleRevision` for observed drift. Checkpoint bodies already use `StateInstall` in `product_control/storage/checkpoints.rs`, so use that path.

#### L006 — Patch creation, replacement, deletion, and encoding admission limits

**Current code:** `crates/runtime/peritus-patch/src/set.rs` — `MAX_FILE_BYTES`, `MAX_PATCH_BYTES`, `MAX_PATCH_OPERATIONS`, `PatchSet::new`, `canonical_identity`; `content.rs` — `FinalFile::new`; `verified.rs` — `patch_bounds_valid`.

**Proposed minimum fix:** Remove the 8 MiB/1,024-operation rejections, including the preimage-size check that rejects deletes. Change the executable predicate and its Verus contract together. Stop selecting `CodecLimits::PRODUCTION` for local patch identity; preserve canonical bytes for existing patches and actual length-prefix overflow checks.

#### L013 — Checkpoint coverage and restore capabilities disagree

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/rewind/plan.rs` — `restore_plan_with_store` builds `FinalFile` and `PatchSet`; `checkpoints/projection.rs` — `patch_input` erases every `PatchError`.

**Proposed minimum fix:** Use the repaired patch admission for every captured path in the same transaction, and preserve typed patch errors in `patch_input`. Keep preview equality and exact owned-postchange comparisons; do not split a restore into independently visible partial patches.

#### L016 — Transaction manifest ancestor and decoder ceilings

**Current code:** `crates/runtime/peritus-patch/src/transaction/manifest.rs` — `validate_patch_capacity`, `Manifest::encode`, `Manifest::decode`, `read_identity`.

**Proposed minimum fix:** Ancestor paths are already deduplicated with `BTreeSet`; do not implement deduplication again. Remove the 1,024-entry and 8 MiB identity checks and use matching local-storage codec capacities on encode/decode. Keep checksum, path ordering, transaction binding, and the existing schema for unchanged encodings.

#### L020 — Journal atomic-batch ceilings restrict checkpoint admission

**Current code:** `crates/state/peritus-journal/src/append_plan.rs` — `MAX_*` batch constants; `append_plan/validation.rs` — `validate_bounds`; `crates/app/peritus-daemon/src/product_control/storage/checkpoints.rs` — `checkpoint_installs`, `checkpoint_key`.

**Proposed minimum fix:** Remove the fixed batch collection comparisons while keeping nonempty event/head requirements and canonical uniqueness checks. Replace the `u16` checkpoint index ceiling with an extended key encoding; keep the existing two-byte keys for old indexes so retained bodies remain readable. Publish bodies, root, and receipt in the same batch.

#### L035 — Tool writes and completed-command receipts impose a tighter file ceiling

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor.rs::MAX_FILE_BYTES`, `executor/effects.rs::{write,patch}`, and `executor/checkpoint_observer.rs::{prepare_write_checkpoint,prepare_patch_checkpoint,exact_file_receipt}` enforce two MiB, including after a successful command. The receipt actually needs a digest, size and mode, not an inline file body.

**Proposed minimum fix:** Remove these file-size checks with the checkpoint/storage ceilings. Stream the post-command file digest instead of reading the entire file merely to produce a receipt; retain before/after identity checks. No new receipt store is needed.

#### L036 — Command preflight checkpoints every enrolled file

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor/checkpoint_observer.rs::prepare_command_checkpoints` checkpoints every enrolled scope path and checks that the scope stayed unchanged.

**Proposed minimum fix:** Reuse already captured, unchanged preimages and remove the capture ceilings in this group. Retain preflight coverage of the actual writable scope and exact scope identity; do not bypass rollback protection.

### Checkpoint manifest representation and generated labels

Remove artificial manifest count restrictions, and prevent derived display text from invalidating a legitimate checkpoint or branch. Keep names as bounded display labels where that suffices; do not widen every text type indiscriminately.

#### L010 — Checkpoint manifest count and text-field ceilings

**Current code:** `crates/app/peritus-product-runner/src/control/checkpoint.rs` — `UserCheckpoint::new`, `validate`, `CheckpointPath`, `exclusions`, `external_effects`; `control/checkpoint/restore.rs` — `RestoreOperation::settle`.

**Proposed minimum fix:** Remove the `u16::try_from` count admission checks from constructors and validation. Store exclusion path and reason separately, or add a path-bearing exclusion variant while reading legacy strings; a valid 4,096-byte path must not be squeezed into `ControlText<512>`. Match public checkpoint DTO admission when that consumer is mapped.

#### L012 — Derived text can exceed otherwise valid field capacity

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/logical.rs` — `logical_rewind_branch` constructs `ConversationTitle::new(format!("Rewind of {}", record.title()))`; `checkpoints/capture.rs` — `empty_directory_exclusion`.

**Proposed minimum fix:** Fit the derived rewind label to the existing title byte limit at a UTF-8 boundary, or fall back to a stable short label. Use L010's structured exclusion instead of concatenating a full path into a 512-byte reason. User checkpoint/branch admission must not depend on decorative prefixes.

### Remove imposed archive quotas at configuration and enforcement together

Make logical quota/per-artifact policy optional in the existing store configuration, reservation and catalog contracts, and select no quota for persistent local memory. Preserve checked durable accounting, physical failures, finalized digests and referenced history.

#### L057 — Local context archive, record, and artifact storage capacities

**Current code:** `crates/app/peritus-product-runner/src/local_context/storage.rs` selects 64-MiB artifacts and a one-GiB quota. `local_context/record.rs::{encode,decode}` independently imposes 32 MiB; storage records also decode with the production codec.

**Proposed minimum fix:** Remove those caller/record maxima and use matching writer/reader capacity, including the shared codec/state-install repair. Keep the same lineage and existing artifact references. Remove incidental archived call ID/name text caps without weakening their identity checks.

#### L062 — Generic artifact quota admission and storage-location fences

**Current code:** `crates/state/peritus-artifact-store/src/config.rs::StoreConfig::new` requires positive numeric artifact/quota limits and quota within SQLite i64 range.

**Proposed minimum fix:** Add an explicit absent-quota policy and thread it through existing store admission. Keep real SQLite representation checks for stored sizes/accounting and protected storage location; a numeric sentinel is not unlimited.

#### L065 — Logical artifact quota blocks writes independently of physical free space

**Current code:** `crates/state/peritus-artifact-store/src/quota.rs::{QuotaSnapshot::new,QuotaPlan::reserve}` and `catalog.rs::record_finalized` both compare totals against mandatory numeric quotas.

**Proposed minimum fix:** Apply quota comparisons only when explicitly selected, retaining checked totals. Do not rely on changing the local-context constant alone: both reservation and final catalog admission must support absence.

#### L067 — Referenced and quarantined artifacts keep consuming the logical quota

**Current code:** `crates/state/peritus-artifact-store/src/catalog.rs::record_finalized` sums all recorded artifact bytes, including quarantined records.

**Proposed minimum fix:** Use the absent imposed-quota policy rather than delete referenced history to admit writes. Preserve reference-safe collection of genuinely unreferenced artifacts; reference/GC integrity is not the blocker being removed.

### Discard state size and interrupted preparation

Repair the existing discard transaction representation and preparation lifecycle. Preserve its checksums and foreign-file protection; no replacement journal is needed.

#### L148 — Managed discard recovery adds a fixed 64 MiB whole-state ceiling

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/transaction/state.rs::{read,save}` imposes 64 MiB on a complete JSON state, inside a header with a checked u32 payload length.

**Proposed minimum fix:** Remove the 64 MiB policy check from both read and save. Stream serialization/checksum where practical and retain atomic save. The existing u32 header remains a real representation limit; widening beyond it requires a versioned header with a compatible old decoder, not an unchecked cast.

#### L149 — Incomplete or changed discard preparation blocks restart cleanup with no repair operation in this module

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/transaction/owned.rs::Asset::check` rejects payload leaves with no saved seal; `own_directory` records ownership before payload preparation.

**Proposed minimum fix:** Record and resume preparation explicitly so an owned unsealed payload is recognized as interrupted work. Preserve it while completing/verifying preparation, or expose explicit abandonment of that owned preparation. Queue/retry owner contention with cancellation. Keep changed markers, foreign leaves and replaced parents untouched.

### Effect receipt capacity, compatibility and outcome reconciliation

Repair the existing receipt ledger and bind its command records to the shared command-owner repair. Never infer a failed command or safely repeat an effect merely because its completion receipt is missing.

#### L165 — Durable effect receipts have fixed per-record and lifetime storage ceilings

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/receipt.rs` defines 2 MiB records/128 MiB ledger; `receipt/storage.rs::{append,read_bytes,decode_records}` enforces them; Applied and Completed duplicate output.

**Proposed minimum fix:** Remove record/lifetime quotas on both append and replay. Stream frames and reference an existing immutable full result for subsequent state transitions rather than duplicating it. Keep checked lengths, sync and action identity; retain control access and recovery evidence if actual persistence fails after an effect.

#### L166 — An interrupted command receipt becomes ambiguous, with an epoch-wide barrier that survives acknowledgement

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/receipt.rs::{begin,prior_command_barrier}` makes Started command outcomes ambiguous and blocks cross-scope effects; `receipt/inspection.rs::acknowledge_uncertain_effect` records Reviewed without proving outcome.

**Proposed minimum fix:** Bind receipts to a stable command-owner/result identity and reconcile Started against that owner before choosing replay or resumption. This depends on the persistent native owner in L086/L094; no durable reattachment endpoint currently exists. Keep genuinely unknown effects blocked and require confirmed inactive ownership for acknowledgement; new user input alone must not manufacture a known result.

#### L167 — Unknown receipt format versions are silently skipped during replay

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/receipt/storage.rs::decode_records` advances past unsupported versions without retaining them; `receipt/codec.rs::decode` accepts a version field.

**Proposed minimum fix:** Stop silently skipping unsupported receipt versions. Preserve their effect identities behind an explicit incompatibility error or decode a known historical version; repair only provably incomplete tails under the existing owner.

#### L183 — Unknown command projection blocks retry and delivery controls without native outcome reconciliation

**Current code:** `crates/app/peritus-daemon/src/product_run/operation.rs::{project,acknowledge_command_outcome}` treats nonterminal Started as Running and terminal unresolved receipts as OutcomeUnknown, offering acknowledgement without native reconciliation.

**Proposed minimum fix:** Reconcile durable receipt identity with the persistent native owner/result from L086/L094/L166. Require proven inactive ownership for acknowledgement rather than inferring it from a terminal run phase; retain independent read/export controls for unaffected candidates while keeping unknown mutations fenced.

### Make patch preparation and cleanup idempotent under existing receipts

Retain owned preparation/completion facts before fallible filesystem work; keep preimages and exact transaction identity.

#### L552 Patch preparation can leave a manifestless blocker and quarantine has only 100 names

**Current code:** `peritus-patch/src/transaction/apply.rs:62` creates the transaction directory before a durable manifest; cleanup errors are discarded and `recover.rs:298` tries only 100 quarantine names.

**Proposed minimum fix:** Persist preparation ownership in the existing manifest before staging payloads and recover provably owned interrupted directories. Retain cleanup errors; use collision-resistant quarantine names with cancellable retry rather than a hundred-name cutoff.

#### L553 Patch cleanup removes the local completion evidence and repeated recovery fails on absence

**Current code:** `apply.rs:139` removes staging completion evidence and returns cleanup_pending; repeated recover requires the old directory before consulting any caller outcome.

**Proposed minimum fix:** Retain completion in the existing durable caller operation receipt before staging cleanup. Resolve repeated removal/parent-sync from that receipt, treating proven absence idempotently without a second ledger.

#### L554 Patch filesystem work is synchronous and has platform and exact-observation restrictions

**Current code:** `transaction/filesystem.rs:34` fully reads files under MAX_FILE_BYTES and synchronous exact-path checks; apply rejects executable modes only on non-Unix platforms.

**Proposed minimum fix:** Apply shared file-size policy removal and streamed hashing, offload blocking work and observe cancellation at transaction safe points. Validate volume/mode support before effects, preserving preimages, disjoint roots, no-follow and unsafe-target checks.

### Artifact validation should follow the artifact contract

Remove file-size policy cutoffs and run large validation as cancellable owned work. Hard acceptance must come from the requested format/behavior rather than a blanket dialect or task guess.

#### L172 — CSV validation has a fixed whole-file ceiling and a single mandatory dialect

**Current code:** `crates/app/peritus-product-runner/src/gates/artifact_csv.rs::{validate_file,validate,CsvParser::parse}` reads a whole 64 MiB-bounded UTF-8 comma-separated rectangular file.

**Proposed minimum fix:** Delete the 64 MiB CSV size rejection and validate incrementally with cancellation. Select delimiter, encoding and row-shape rules from the actual artifact contract rather than imposing one dialect on every CSV.

#### L173 — JSON and YAML structural validation reject files above 16 MiB

**Current code:** `crates/app/peritus-product-runner/src/gates/{json_structure,yaml_structure}.rs::validate_file` applies 16 MiB checks and whole-file parsing; YAML also rejects empty documents.

**Proposed minimum fix:** Remove the 16 MiB JSON/YAML file checks and perform parsing off the async executor with cancellation, using streaming validation where available. Keep syntax requirements; require nonempty YAML only when the task requires it.

#### L174 — SQLite qualification fixes file size, workspace shape, and repeatability policy

**Current code:** `crates/app/peritus-product-runner/src/gates/sqlite_migration.rs::{verify,discover_migrations,execute_file}` assumes schema.sql/migration.sql, a fresh database, two executions, optional rollback and 16 MiB SQL.

**Proposed minimum fix:** Remove the SQL byte ceiling and mandatory second migration execution. Use the task's actual migration paths and initial database, requiring repeatability or rollback only when requested; interpret explicit postcheck assertions and retain foreign-key checks.

#### L175 — Source-readability gates still require whole-file UTF-8 and restrict the eligible ownership set

**Current code:** `crates/app/peritus-product-runner/src/gates/source_layout.rs::run` has no line-count ceiling; it filters metadata failures away, restricts owned source and reads complete UTF-8.

**Proposed minimum fix:** No line-count limiter remains here. Validate the language's actual encoding and report selected unreadable or missing paths instead of silently omitting them; stream or offload reads. Keep qualification scoped to the intended source set.

### Remove Git output/status/diff/history policy exhaustion

Use native path identity and complete retained observations, with paging or artifacts only for materialized views.

#### L525 Git command byte ceilings fail whole operations and do not bound their duration

**Current code:** `peritus-git/src/command.rs:108` drains bounded stdout/stderr and reports a protocol failure after process completion if either overflowed.

**Proposed minimum fix:** Remove policy overflow as whole-operation failure; stream or spool complete output through existing command ownership. Preserve actual exit/mutation identity and cancellable pipe/reap handling; a large diagnostic must not invalidate successful work.

#### L527 Git status rejects complete observations at fixed counts, paths and parser boundaries

**Current code:** `status/porcelain.rs:15` enforces status byte/entry quotas and rejects unborn HEAD; `status.rs:262` turns every write-tree failure into index_tree=None.

**Proposed minimum fix:** Remove status count/byte/path policy caps, parse incrementally with native path identity and represent unborn HEAD explicitly. Preserve actual write-tree failures and distinguish index conflict from corruption.

#### L528 Structured Git diff and history have additional fixed, nonpaged observation ceilings

**Current code:** `diff.rs:141` builds complete name/patch output then rejects by requested capacities; `history.rs:55` imposes a hard maximum with no continuation result.

**Proposed minimum fix:** Remove independent path/patch quotas, stream or reference large patches and add exact commit continuation with a more-results indication. Preserve immutable base/target identity and precise incompatible-entry diagnostics.

#### L529 Candidate creation and restoration scan every directory under a fixed entry limit

**Current code:** `snapshot/support.rs:169` scans every directory and rejects beyond MAX_SCAN_ENTRIES, including unrelated ignored trees.

**Proposed minimum fix:** Remove the 200,000-entry cutoff and restrict candidate inventory to relevant owned content. Keep cancellable inspection of nested repositories/control metadata and explicitly support registered nested repositories; restoration/cleanup must still protect unrelated ownership.

### Git tool results and supported catalog

Share byte-aware continuation with Git observation and filesystem rendering. Retain completed mutation facts before any fallible result formatting.

#### L643 Git observation and rendering ceilings have no continuation protocol

**Current code:** `peritus-tools-git/src/{input,schemas,render}.rs` exposes bounded observations with first-500 items, 32 parents and 48 KiB patch windows but no continuation.

**Proposed minimum fix:** Add exact history/path/parent continuation and patch range or full artifact access, sharing L528/L636's encoded-byte paging. Keep preview completeness explicit and all retained data reachable.

#### L644 Git mutation can succeed before its terminal envelope is rejected

**Current code:** `peritus-tools-git/src/dispatcher.rs` performs candidate/rollback then renders/assembles the result; mutation_outcome is held in an Option and protocol validation remains fallible.

**Proposed minimum fix:** Preflight representable result metadata and retain the exact outcome against the existing operation receipt before rendering. Retry publication/assembly without repeating mutation effects, using L582 recovery.

#### L645 Registered Git merge is permanently unsupported; errors do not provide progress or retry detail

**Current code:** `peritus-tools-git/src/catalog.rs` registers git.merge while `dispatcher.rs::merge_unsupported` permanently rejects it; dispatch errors collapse causes.

**Proposed minimum fix:** Remove unsupported git.merge from advertised capabilities until implemented. Preserve typed Git/workspace recovery details and use owned cancellable execution for long work; avoid inventing merge machinery for this repair.

#### L734 Production tool composition has a hard configured-count gate and closed route catalog

**Current code:** `peritus-daemon/src/component/tools/registry.rs::ToolComponents::build` rejects allowed.len()>MAX_CONFIGURED_TOOLS before checked_names and selects a closed catalog.

**Proposed minimum fix:** Remove the count cap only where it binds implemented catalog entries; keep exact implementation digests and explicit allowlisting. Remove nonexistent merge advertisement and preserve typed causes/lower-layer receipts.

### Page structured review and represent valid Git data without whole-review rejection

Share candidate/diff identities and continuation between server parsing, protocol pages and TUI navigation; keep comment anchors exact.

#### L249 — Structured-review navigation stays on the loaded page and repeat refresh has no in-flight guard

**Current code:** `crates/app/peritus-tui/src/model/product/review.rs::refresh_review` always requests offset zero and has no pending-query guard; keys only navigate loaded collections. `review/state.rs` retains page/selection/drafts separately.

**Proposed minimum fix:** Request next/previous comment and diff pages and coalesce repeated refreshes under one pending query. Retain drafts and explicit stale-anchor rebind. Keep the existing mutation-owner/terminal-boundary check while allowing complete read access.

#### L254 — Structured diff is a whole bounded object, while only comments are paginated

**Current code:** `crates/app/peritus-app-protocol/src/workbench/review.rs` sets 512 files/4096 hunks/32768 lines; review/page.rs::WorkbenchReviewPage::new requires the complete diff on every comment page and rejects larger collections.

**Proposed minimum fix:** Extend the query/page with file/hunk/line continuation bound to the same candidate and raw diff digest; parse only the requested bounded view instead of rejecting the whole diff at collection ceilings. Update daemon page selection and TUI navigation together. Retain per-page memory/frame limits and exact anchor binding.

#### L255 — Valid Git quoted paths and long source lines can disable the entire structured diff

**Current code:** `crates/app/peritus-app-protocol/src/workbench/review/parser.rs::path_from_header` searches literal ` b/`; diff_path accepts only unquoted prefixes. `review/diff.rs` rejects line/header text over MAX_PRODUCT_DETAIL_BYTES or containing controls.

**Proposed minimum fix:** Decode Git C-quoted path bytes exactly before validating workspace-relative paths. Represent oversized/control-containing lines as safe ranged text tied to retained raw bytes and exact anchors, preserving full content for inspection. Reuse the diff paging repair rather than making one line fail the entire review.

### Durable request archives must match admitted requests

Align request encoding, manifest verification and artifact storage. Keep exact immutable request identity and incorporation; a separate archive quota must not strand an otherwise admissible turn.

#### L180 — Durable request admission caps the entire serialized model request at 16 MiB

**Current code:** `crates/app/peritus-daemon/src/product_control/inputs/archive.rs::{new,verify_manifest}` imposes 16 MiB request/state and repeats source/media bounds; `inputs/files.rs::file_context` checks a fixed rendered context; `inputs/manifest.rs::messages` uses production codec limits.

**Proposed minimum fix:** Remove independent request/archive/manifest ceilings together with the shared codec, control-text and media repairs. Store large immutable bodies through existing artifact references and budget only the actual next provider view. Preserve message/media order, byte/digest identity and exact incorporated selections; change readers with writers.

### Project initialization source selection

Use exact source observations and reviewed preimages without imposing a small-file prerequisite.

#### L188 — Project initialization rejects any selected source above 256 KiB and only discovers a fixed root-local set

**Current code:** `crates/app/peritus-daemon/src/product_control/init.rs::{discover_init,read_selected}` uses a fixed root-local SELECTED_SOURCES list and rejects above MAX_INIT_SOURCE_BYTES before reading.

**Proposed minimum fix:** Remove the 256 KiB selected-source rejection and read larger sources through existing ranges. Report discovery and parsing failures per source and allow explicit command selection; retain unverified proposals and exact preimage approval.

### Media admission and discovery

Remove duplicated host selection quotas together, then assemble explicit media against the selected provider's real capabilities and capacity. Preserve exact bytes, pixel validation and owned-path access. Decoder memory policy must be explicit; removing checks without controlling allocation is not a repair.

#### L126 — Image attachments have independent host ceilings

**Current code:** `crates/app/peritus-product-runner/src/attachment.rs::ValidatedImage::decode` and `attachment/decode.rs::{limits,check_dimensions,validate_frames}` enforce encoded, count, side, pixel, frame and decoded-byte limits independently of provider capacity.

**Proposed minimum fix:** Remove fixed selection/count/encoded-size ceilings from host admission and their control-ledger callers. Replace fixed decoder ceilings with an explicit resource policy and incremental frame validation; keep actual decoder allocation failures and image integrity errors. Offer explicit frame selection or downsampling when needed instead of changing input silently.

#### L142 — Managed workspace image discovery silently drops candidates and can stop the whole traversal on depth

**Current code:** `crates/app/peritus-product-runner/src/workspace_media.rs::{discover,discover_paths,attach}` truncates selection to 16 and breaks the traversal at depth/count; `workspace_media/folder.rs::discover_explicit` applies `.take(MAX_IMAGES)` before deduplication.

**Proposed minimum fix:** Skip only an over-depth branch, or remove the synthetic depth policy; remove silent discovery/selection cutoffs and deduplicate explicit paths first. Add continuation for large discovery results—the current traversal has none—and report skipped/unreadable media. Admit the chosen set using actual provider capacity and the shared media policy.

#### L162 — Attachment count limits can block releasing held inputs, while pin overrides use a different selection path

**Current code:** `crates/app/peritus-product-runner/src/control/images.rs::ImageLedger::validate` and `control/files/ledger.rs::validate` cap selected eligible ledgers; `control/record/projection.rs::{eligible_images,eligible_files}` applies Pinned/Excluded preferences differently.

**Proposed minimum fix:** Remove host selection quotas from whole-record lifecycle validation. Compute one effective selection for provider admission after pin/exclusion resolution, using the shared media/text policy. Releasing a held caption must not fail because archived selection counts reached an arbitrary ceiling; retain image integrity and exact artifacts.

#### L261 — External file ranges still require reading a complete source capped at 64 MiB

**Current code:** `crates/app/peritus-tui/src/file_import.rs::read` rejects a source over 64 MiB, buffers/hash-scans all bytes, then applies a 256 KiB selected range; resolve_lines uses u32. image_import uses the media path grouped above.

**Proposed minimum fix:** Stream the complete source digest while collecting only the requested range; remove the 64 MiB whole-source policy ceiling and share selected-text/media admission with host/protocol limits. For a selection too large for one transfer, use chunked artifact/range access rather than whole-file buffering. Keep exact source-change checks and regular-file/path authority.

### Reviewer evidence and text attachment capacity

Select the next provider view using its actual remaining context. Keep the complete underlying material available; a bounded prompt projection must not become a lifetime admission limit.

#### L127 — Initial reviewer evidence is capped independently of available provider context

**Current code:** `crates/app/peritus-product-runner/src/turn/evidence.rs` estimates bytes per token and applies 50/75-percent targets plus a 384 KiB ceiling to weighted evidence.

**Proposed minimum fix:** Remove the independent ceiling and fixed percentages; budget evidence from actual remaining input capacity, including system/tools/output reservation. Preserve source identities and explicit omitted ranges, with archived conversation evidence retrievable rather than assuming fresh filesystem reads recover old conversation text.

#### L128 — Explicit text attachments have separate selection admission ceilings

**Current code:** `crates/app/peritus-product-runner/src/attachment/text.rs` independently caps file bytes, aggregate bytes and reference count; it also validates UTF-8, controls and exact digest.

**Proposed minimum fix:** Remove those admission quotas and update the file-ledger callers consistently. Use range reads or artifact-backed retrieval for a provider view that cannot contain the complete selection. Retain exact source/digest and text validity checks.

### Attachment originals, ranges and paged history

Retain immutable originals and full selection history. Apply actual decoder/provider capacity to selected views and transfer pages, not to the lifetime/source metadata.

#### L707 Brief projection limits exact proposals and rejects empty file observations

**Current code:** `peritus-app-protocol/src/workbench/brief.rs::WorkbenchBriefObservation::new` rejects bytes==0 and >64 MiB; proposal/page constructors add text/source limits.

**Proposed minimum fix:** Allow empty-file observations consistently and remove proposal/source-history quotas. Page exact proposals/observations with explicit user confirmation and disclosed omissions.

#### L708 File and image history pages impose a 256-retained-reference ceiling

**Current code:** `peritus-app-protocol/src/workbench/{files,images}/page.rs` already checks exact 32-row coverage but also rejects total>256 and restricts offsets.

**Proposed minimum fix:** Remove total-reference and matching offset policy ceilings together, keeping exact revision fences, bounded pages and retained selection history.

#### L709 File selection cannot address any source beyond 64 MiB and has a narrower import-label gate

**Current code:** `peritus-app-protocol/src/workbench/files.rs::WorkbenchFileRange::validate` caps byte/line coordinates at 64 MiB; preview/import metadata add source/label limits.

**Proposed minimum fix:** Remove original-source and range-coordinate policy caps, align import/workspace labels and keep selected provider chunks separate. Preserve source digest, exact range and confirmation binding.

#### L710 Raster admission has fixed encoded, geometry, frame and format gates

**Current code:** `peritus-app-protocol/src/workbench/images/metadata.rs::new` caps original encoded size, 64 frames, 8192 dimensions and 16 Mi pixels.

**Proposed minimum fix:** Retain originals in existing artifacts; enforce actual decoder/provider limits only on selected views. Make frame selection/conversion explicit with provenance instead of rejecting original media categorically.

#### L736 File and image consent archives add fixed 16 KiB proof ceilings and full-byte reconstruction

**Current code:** `peritus-daemon/src/product_control/storage/{files,images}.rs` reconstructs archived bytes; image proof verification imposes an independent 16 KiB limit.

**Proposed minimum fix:** Remove independent consent-proof/original-size gates and stream archived-content verification. Preserve exact consent digest, producing position and atomic proof publication.

#### L748 File preview shares image decoder admission and repeats complete verification on confirmation

**Current code:** `peritus-daemon/src/product_run/workbench/files.rs::preview_workbench_file` consumes image_decodes capacity and runs uncancelable blocking prepare_file; confirmation repeats source verification.

**Proposed minimum fix:** Separate text I/O from raster decoder admission with fair cancellable work. Stream selected ranges and retain exact consent comparison plus committed-result replay.

#### L749 Refresh-on-request reobserves every selected source and can repeatedly invalidate preparation

**Current code:** `peritus-daemon/src/product_run/workbench/files/refresh.rs::refresh_request_files` re-prepares each selected source and commits successive revisions.

**Proposed minimum fix:** Capture one coherent source snapshot and apply refresh changes together before request construction. Retry affected sources only and preserve their exact errors instead of restarting all preparation.

#### L750 Brief projection skips larger replies and can reject a legitimate empty file

**Current code:** `peritus-daemon/src/product_run/workbench/brief.rs` skips replies once proposal count is full or WorkbenchInputText rejects their size; file observations inherit zero-byte rejection.

**Proposed minimum fix:** Allow empty source observations and page large exact proposals instead of excluding them at eight replies/8 KiB. Keep explicit acceptance and immutable author/source digests.

#### L798 — Attachment previews select launcher role defaults instead of the active conversation's provider

**Current code:** `peritus-tui/src/model/chat/workbench/{images,files}.rs` chooses writer from product.launch defaults rather than chat_providers/selected run.

**Proposed minimum fix:** Resolve preview/confirmation through the active chat provider/model binding and invalidate on actual binding change. Preserve source digest/workspace/confirmation checks.

#### L800 — Caption edits unnecessarily invalidate file previews, and multiline captions are rejected

**Current code:** `peritus-tui/src/model/chat/workbench/files/keys.rs` discards preview after every edit/cursor key and rejects all control characters in captions.

**Proposed minimum fix:** Invalidate only source path/range/mode or authoritative provider changes, allowing newline/tab captions. Accept a returning preview only when its complete request still matches.

#### L807 — Web attachments are accepted at sizes that the durable message cannot send

**Current code:** `peritus-web/src/files/attachments.rs::stage` accepts 48 KiB snapshots/256 MiB cache; daemon/chat.rs::message embeds contents then rejects combined text above 8 KiB.

**Proposed minimum fix:** Send durable attachment handles through one staging/send admission contract. Add explicit snapshot removal of bytes plus metadata and remove retained-metadata quota as action admission.

### Complete checkpoint and initialization confirmation

Hash exact retained manifests incrementally and page their presentation; transfer capacity must not change what the user confirmed.

#### L712 Checkpoint and rewind coverage use whole 65,535-item lists and production-sized preview hashing

**Current code:** `peritus-app-protocol/src/workbench/checkpoints.rs::validate_lists` limits whole coverage lists to u16; rewind constructor hashes a production-sized encoded preview.

**Proposed minimum fix:** Remove whole-list policy limits and stream the existing full confirmation fingerprint. Page display coverage while binding consent to all retained paths, exclusions/effects and preimage/conflict facts.

#### L715 Initialization uses fixed whole-file source, patch and diff ceilings

**Current code:** `peritus-app-protocol/src/workbench/init/{proposal,render}.rs` embeds complete original/proposed content/diff and caps source/command lists and bytes.

**Proposed minimum fix:** Remove whole-source/diff policy ceilings, using ranged reads and existing exact patch/artifact references. Preserve managed markers, original bytes and user consent; reuse the initialization path.

### Preview capture and behavior evidence

Publish exact capture/output artifacts once, retain their bindings and retry publication/control independently of operation admission. Use bounded output tails only for display.

#### L192 — Preview capture has a 16 MiB PNG ceiling and only an X11/ImageMagick backend

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/launch/capture.rs::capture_window` sets a ten-second helper timeout, enforces MAX_CAPTURE_BYTES, removes the file only after reading, then decodes without the shared guarded decoder; capability discovery is X11/ImageMagick.

**Proposed minimum fix:** Remove the synthetic ten-second helper deadline through the optional shared launch contract and remove the 16 MiB capture ceiling. Use the shared guarded decoder with explicit allocation policy, cleanup guards on every outcome and retry publication of the same capture. Report backend availability accurately; adding a new platform capture backend is separate from this minimum repair. Retain capture consent.

#### L193 — Preview behavior evidence must be found in the retained stdout tail and only after terminal state

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/launch/output.rs::retain_output` retains 128 KiB tails; `launch/evidence.rs::{check_preview_behavior,qualify_graphical_goal}` requires terminal state, tail substring and live preview-map ownership.

**Proposed minimum fix:** Bind behavior checks to full retained output artifacts from the shared output-retention repair (L090), using exact offsets/digests. Permit observations while running where the criterion allows and qualify from durable process/build/capture facts after restart. Keep ordering and consent; the current tail is not a full artifact.

#### L206 — Preview protocol exposes 8 KiB output tails and accepts collections beyond downstream process bounds

**Current code:** `crates/app/peritus-app-protocol/src/workbench/launch/output.rs` exposes 8 KiB tails; `launch/profile.rs::new` admits u16 collections but requires readiness_millis>0 and <=wall_millis, with InheritedHost network.

**Proposed minimum fix:** Keep bounded tails for display and expose full output artifact/range retrieval through L090/L193. Align profile and downstream launch admission, making readiness/wall expiry optional rather than forcing a synthetic execution deadline. Retain exact consent, native dimensions and Network authority for inherited-host execution.

### Retain complete command output and a live tail

Stream all output into the existing spool by default, while maintaining bounded live display/event pages.

#### L090 — Output retention is a finite prefix, and its displayed tail freezes after quota exhaustion

**Current code:** `crates/runtime/peritus-process/src/supervisor/io.rs::accept_output` sends only accepted prefix bytes to spool/window/events. `output.rs::OutputAccounting::observe` drops bytes past stream/aggregate limits; `output/window.rs::RetainedWindow` therefore freezes at that prefix.

**Proposed minimum fix:** Use absent default spool/stream quotas from L087 and update the rolling display from every observed chunk. Keep exact dropped-byte reporting if an explicitly selected quota or physical failure occurs, and preserve publication causes for retry.

### Remove incidental process recovery and argument size caps

Widen matching writer/readers and preserve canonical checksums, native argv/environment semantics and exact identities.

#### L095 — Process recovery records impose a 16-KiB canonical manifest bound

**Current code:** `crates/runtime/peritus-process/src/recovery/manifest/codec.rs::{encode,decode}` imposes 16 KiB and classifies new encoding overflow as corruption. Signal text is bounded at 128 bytes and encoded with u16 length.

**Proposed minimum fix:** Remove the aggregate and signal policy maxima, widening the signal length format compatibly where necessary. Preserve checksum/terminal binding and distinguish new-record admission errors from corrupt persisted data.

#### L096 — Structured command/environment admission has fixed size and portability restrictions

**Current code:** `crates/runtime/peritus-process/src/command.rs::CommandSpec::new` caps executable/argument/count/total sizes. `environment.rs::{allowlisted,finish}` caps variables and folds names on every platform. `working_directory.rs::open` rejects non-Unicode paths.

**Proposed minimum fix:** Remove application argv/environment quotas and defer to real native admission with precise errors. Use platform-correct case handling and native Unix bytes through compatible canonical representation. Keep NUL, allowlist, authority and backend identity checks.

#### L169 — Preview admission permits untimed operation but retains payload bounds and a narrower downstream process contract

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/preview.rs::PreviewCommand::new` currently requires a positive `Duration`, u16 collection admission, 4,096-byte program, 64 KiB argument/value and 512-byte key. This baseline does not yet support `None`.

**Proposed minimum fix:** Change preview timeout to explicit optional duration and carry it through the shared corrected launch/resource contract. Remove preview-only payload/count quotas and align downstream admission with L095/L096. Keep actual OS limits, NUL/name validity, nonzero terminal dimensions and specific launch errors; do not claim deleting a local check makes a mandatory downstream timeout optional.

### Full command evidence with bounded prompt previews

Keep durable full output and retrieve it by identity; prompt previews can remain bounded when omissions are explicit.

#### L170 — Independent review receives a rolling, truncated command-observation window

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/evidence.rs::{record_named,render,merge_rendered}` retains 32/128 KiB preview records and only command/purpose for the unbounded successful list.

**Proposed minimum fix:** Keep full command-result references in the existing evidence records and let review retrieve them. Retain a rolling preview for prompt fit, with framing counted and omissions marked; do not make the preview the only retained evidence.

### Explicit external-reference navigation

Keep explicitly granted read authority while removing traversal dead ends and path-recognition mistakes.

#### L168 — Explicit external-reference reads have fixed discovery and path-recognition bounds

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/reference.rs::{list,explicit_absolute_path,names_match,task_tokens}` cuts off at 512, caps depth at six, strips terminal periods and applies case folding everywhere.

**Proposed minimum fix:** Add continuation to listing, enumerate incrementally and report child failures. Preserve quoted path punctuation and apply filesystem-appropriate case rules. Remove the independent path text ceiling coherently with native representation. Keep explicit external roots and symlink confinement; listing currently has no continuation cursor to reuse.

### Evidence bundle scale, history and completion

Export immutable provenance with its original identity, stream exact content and publish completion atomically. Separate historical evidence from claims of current authority.

#### L621 Evidence and bundle limits reject legitimate scale, sometimes after output has already been written

**Current code:** `peritus-evidence/src/{record,manifest}.rs` and `bundle/{plan,assemble,verify}.rs` impose independent counts/bytes; assemble writes directly to caller output before later failure.

**Proposed minimum fix:** Remove fixed policy ceilings consistently, preflight exact encoded sizes and stream reads/hashes. Use owned temporary publication or explicitly retain partial-output status; preserve artifact digest verification.

#### L622 Admitted causal history across revisions cannot be represented by the current portable bundle policy

**Current code:** `peritus-evidence/src/bundle/plan.rs::{build,plan_bundle}` requires one identical revision and Current freshness for every record, although causality admits historical parents.

**Proposed minimum fix:** Include historical parents at their original revisions as provenance. Require freshness only for records asserting current authority, preserving complete ancestry instead of rewriting history to one revision.

#### L623 Evidence work repeats synchronous whole-history and artifact verification without a resumable work owner

**Current code:** `peritus-evidence/src/admission.rs` scans exported records/references; store and bundle paths repeat whole artifact verification and synchronous reads.

**Proposed minimum fix:** Recognize exact committed retries first, index immutable frame/reference lookups and reuse verification for the same snapshot. Stream cancellable hashing with explicit framed completion instead of unowned EOF waiting.

#### L624 Evidence startup holds a write transaction for a full catalog scan and has no quarantine reconciliation operation

**Current code:** `peritus-evidence/src/sqlite/quarantine.rs::contain_corrupt_records` holds an Immediate write transaction across every active identity; public quarantine APIs expose reads/counts only.

**Proposed minimum fix:** Scan incrementally outside one catalog-wide writer lock and contain each verified corrupt record narrowly. Add explicit digest-checked reconciliation of repaired dependencies, retaining originals and identity fences.

### TUI drafts share durable input admission

Use the same referenced/chunked input contract for editing and sending; retain original intent through queue/order continuation.

#### L784 — TUI composition duplicates whole drafts before its fixed task-size rejection

**Current code:** `peritus-tui/src/input/composer.rs::layout` lays out the complete draft; model/chat/keys.rs gates editing at MAX_PRODUCT_TASK_BYTES after copies.

**Proposed minimum fix:** Use shared referenced/chunked input and visible-range layout, avoiding full-paste copies before admission. Report rejected insertion; preserve UTF-8/sanitization and explicit slash-command intent.

#### L787 — The composer accepts a larger draft than durable chat admission can represent

**Current code:** `peritus-tui/src/model/chat/workbench/conversation.rs::send_workbench_chat` converts the larger composer draft to an 8192-byte WorkbenchInputText and retains submission only in memory.

**Proposed minimum fix:** Use the same referenced-text admission for composer and durable queue. Give queue-to-execution continuation the existing operation identity and reconcile its launch on retry; preserve workspace/unarchive authority.

#### L795 — Queue reorder requires every pending ID but the composer cannot hold the largest valid order

**Current code:** `peritus-tui/src/model/chat/workbench/queue.rs` parses every pending 32-hex ID from /queue order text, exceeding composer capacity for a valid large queue.

**Proposed minimum fix:** Accept an exact referenced order or shared incremental ordering operation against the inspected queue revision. Retain normal reorder receipts and identity-based paged selection.

### Web console custody and resumed output

Reuse owned process execution for console PTYs and retained bindings/output positions; gateway restart/disconnect detaches observers without destroying work.

#### L808 — Web consoles are process-local and counted even after the CLI exits

**Current code:** `peritus-web/src/terminal.rs::start` caps consoles at 24, spawns transient PTY/reader owners and retains only 1 MiB output; consoles.rs lists ended children without automatic retirement.

**Proposed minimum fix:** Remove console-count gate and reap ended children. Use owned process service with saved console bindings/spooled offsets; move input/reaping outside map lock and support explicit close/cancel.

### Web observations and Git work with real continuation

Bound work as well as rendering with stable inventories/ranged reads, and retain Git mutation custody independently of request lifetime.

#### L811 — Web paging and Git actions still wait for whole-content work

**Current code:** `peritus-web/src/files.rs::{list,text}` scans/sorts directories or reads whole files before paging; daemon.rs::runs collects every page; git.rs::execute kills at two minutes.

**Proposed minimum fix:** Return one run page per request, cache stable directory inventories and read requested text ranges. Remove Git's synthetic timer and use owned cancellable command execution with reconciliation; release project observation locks while retaining mutation custody.

#### L812 — Web presentation and HTTP bounds are separate from execution lifetime

**Current code:** `peritus-web/src/config.rs::Preferences::parse` caps aliases/shortcuts=100, font 12–22 and explorer 180–480; title validation differs across API/native preparation.

**Proposed minimum fix:** Use one title type/byte-unit errors and remove arbitrary presentation/count restrictions where widgets support values. Keep operation grammar, chunked transport, origin/token and path confinement; these are not execution lifetime limits.

### Make large inspection and incomplete control parsing recoverable

Page or use wide logical positions for display; finish sanitizer state only at explicit stream/record boundaries, preserving inert output.

#### L260 — Render scroll saturates at 65535 rows; an unterminated control string can hide later output

**Current code:** `crates/app/peritus-tui/src/render/product.rs::content_scroll_limit` saturates to u16; review/state.rs and terminal.rs use u16 scroll. `sanitize.rs::TerminalSanitizer` has no end-of-stream finish and can stay in OSC/String state indefinitely.

**Proposed minimum fix:** Use usize/u64 logical scroll or explicit pages, rendering a viewport without ratatui's u16 offset becoming a total-content ceiling. Add an explicit finish/reset operation at genuine stream/record end that emits an inert incomplete-control indication; retain parser state across ordinary chunks and never let source controls execute.

### Native path admission and per-entry directory failures

Keep relative-path confinement, no traversal, protected metadata, and no-follow access. Apply Windows naming restrictions where Windows actually requires them, and report an unsupported directory child without invalidating unrelated children.

#### L008 — Path representation and all-or-nothing directory inspection

**Current code:** `crates/runtime/peritus-patch/src/path.rs` — `WorkspacePath::new`, `forbidden_byte`, `valid_component`, `windows_device_name`; `verified.rs` — `path_bounds_valid`; `crates/runtime/peritus-workspace/src/inspection.rs` — `list_directory`, `metadata_from`.

**Proposed minimum fix:** Remove portable depth/name restrictions that reject native-valid targets, updating the predicate contract with the implementation. Gate Windows alias/device checks by the target platform. Return supported directory entries plus explicit per-child unsupported-name/type diagnostics instead of propagating the first such child as failure of the entire listing; do not follow links or grant mutation authority.

### Windows selected capabilities and channel preparation

Admit only capabilities actually needed and proved, preflight before taking consumable preparations, and preserve exact native path/channel causes.

#### L605 Windows capability gates rely on synchronous and incomplete probe evidence

**Current code:** `peritus-sandbox-windows/src/probe.rs::supported_features` uses broad baseline/resource gates; `native/probe.rs` reads the entire helper, infers facilities and reports credential_manager=true.

**Proposed minimum fix:** Check selected controls with real native evidence, omitting unused resource requirements. Stream cancellable helper verification and report precise unsupported capabilities before authority consumption.

#### L606 Windows preparation consumes channel owners before later fallible compilation

**Current code:** `peritus-sandbox-windows/src/channels.rs::prepare_network` rejects a configured proxy for deny-all and takes proxy preparation; `preparation.rs` compiles terminal/manifest after channels.

**Proposed minimum fix:** Treat configuration as availability, selecting optional channels only when needed. Preflight terminal/manifest requirements before taking preparations and preserve owners/typed causes through later failure.

#### L607 Windows path and ACL projection imposes workspace-shape and finite-root gates

**Current code:** `peritus-sandbox-windows/src/filesystem.rs::PathPolicy` counts before deduplication and constrains workspace/input shape; `filesystem/path.rs` uses ASCII folding and to_string_lossy for native paths.

**Proposed minimum fix:** Remove arbitrary root count gates after deduplication. Project deny-dominant permissions around protected roots and validate creation through existing parents using lossless native identity; retain reparse and authority checks.

### Optional updates and exact resumable staging

Remove optional update work from critical launch, retaining exact package identity and stage ownership through download, install and verification.

#### L670 Optional startup update discovery remains on the critical launch path without cancellation or durable download continuation

**Current code:** `peritus-launcher/src/app.rs` awaits offer_on_startup; update/download.rs deletes staging and sets 10s connection, 30m total, 1 GiB and 5m extraction caps; update/install.rs sets 15m install/30s verify.

**Proposed minimum fix:** Run discovery independently and cancellably; remove synthetic execution/download deadlines and arbitrary archive ceilings. Retain exact resumable staging where HTTP range support is verified, use existing install receipts and verify the installed CLI/daemon pair.

## 2. Request, reply, protocol, and representation limits

### Caller-selected codec and model capacities

Separate local durable-storage capacity from negotiated message framing and actual provider capacity. Use one matching capacity choice for each encoder/decoder pair, retaining checked lengths and physical wire representation limits.

#### L009 — Canonical codec production ceilings

**Current code:** `crates/foundation/peritus-codec/src/limits.rs` — `CodecLimits::new` already accepts arbitrary supplied capacities; `writer.rs` — `write_bytes`, `write_str`, `write_collection_len`, `reserve_payload`; `reader.rs` — corresponding reads.

**Proposed minimum fix:** No constructor widening is needed. Replace hard-coded `CodecLimits::PRODUCTION` at affected local-storage callers such as patch identity/manifests with their selected storage limits. Keep bounded transport frames and `u32` length overflow checks; route content beyond a physical frame through existing reference/chunk mechanisms.

#### L017 — Model protocol hard ceilings cannot be widened by profiles

**Current code:** `crates/model/peritus-model-protocol/src/bounds.rs` — `ProtocolLimits::new` compares every field to `Self::PRODUCTION` and rejects larger values.

**Proposed minimum fix:** Remove the `*value > ceiling` admission check, keeping invalid-zero validation where the represented operation requires it. Let the actual provider/transport profile supply request and response capacities; do not treat production defaults as universal maxima. Audit provider-specific secondary clamps in their bundles before claiming larger values work end to end.

#### L406 B3 acceptance/amendment decode hard-codes production limits inside caller-selected codec limits

**Current code:** `peritus-protocol/src/acceptance/contract.rs:229` calls domain conversion with PRODUCTION inside caller-limited decoding; amendment decoding repeats the substitution.

**Proposed minimum fix:** Pass the active reader/caller limits through digest verification and conversion, exposing precise capacity failures. Encode, decode and domain reconstruction must use the same selected representation rather than a hidden production ceiling.

#### L407 B3 agent DTO constructors check only opaque payload capacity before admitting complete records

**Current code:** `peritus-protocol/src/agent/wire.rs:12` checks only opaque payload encoding; AgentCommandDto and AgentStateDto constructors then add metadata/frame fields without checking total size.

**Proposed minimum fix:** Validate complete canonical record plus framing overhead at the existing admission boundary and use streamed payload hashing. Preserve exact capacity errors and avoid payload copies; do not claim an accepted DTO is durable until the full record fits its selected representation.

#### L457 C5 request controls and identifier/media fields have additional fixed acceptance ceilings

**Current code:** `peritus-model-protocol/src/request/options.rs:123` caps stop sequences at 64 and requires output tokens; `request/validation.rs:35` independently caps extensions at 128, alongside identity/media field limits.

**Proposed minimum fix:** Remove independent request/identifier policy quotas where the selected provider can represent the value. Use actual provider constraints and output capacity, retain exact IDs and mandatory API semantics, and distinguish advisory controls from enforced limits.

#### L458 C5 JSON has non-widenable depth/member ceilings checked after full parsing

**Current code:** `peritus-model-protocol/src/schema.rs:23,95` hardcodes depth 64/member 65,536, clamps custom bounds to production, and parses complete JSON before structural validation plus a second duplicate-key parse.

**Proposed minimum fix:** Remove nonwidenable structural clamps and enforce selected allocation bounds while parsing. Keep duplicate-key and local-reference rules, avoid repeated whole-value parsing/copying and retain actual syntax failures.

#### L460 C5 canonical request identity has a separate mandatory 512-MiB total ceiling

**Current code:** `canonical.rs:19` independently caps complete canonical requests at 512 MiB and allocates the full representation before fingerprinting.

**Proposed minimum fix:** Stream the existing canonical encoding into its digest and remove the independent total policy ceiling consistently from decode. Fingerprint the selected invocation view and retain full history in existing archives; preserve byte identity rather than inventing a new protocol.

#### L461 C5 nested admission and persistence checks can disagree under selected limits

**Current code:** `request/validation.rs` checks aggregate counts/capabilities but not every nested value under the final selected limits; canonical/event/message readers reconstruct under their own limits.

**Proposed minimum fix:** Revalidate nested request/event values using the same policy as encoding and decoding, including extension-specific limits. Reject incompatible representation before durable publication, preserving exact canonical parity.

#### L462 C5 archives, events and rate observations add compiled ceilings beyond request bounds

**Current code:** `message_codec.rs:6` fixes archives at 64 MiB; `event_codec/primitive.rs:5` separately fixes event size/collections/depth, and rate observations clamp supplied reset values.

**Proposed minimum fix:** Remove independent archive/event policy ceilings across readers/writers, preserving sequence, tags and transfer paging. Retain observed reset/TTL information as telemetry; it must not acquire local session-expiry semantics.

#### L463 C5 reconstruction materializes full bounded content before aggregate request checks

**Current code:** `canonical_decode.rs:35` materializes nested messages/tools/options, then validates the request and allocates a second full canonical representation to compare.

**Proposed minimum fix:** Track cumulative selected admission while parsing and validate byte parity without constructing a duplicate complete buffer. Preserve exact bytes and cancellation in the owning runtime, without adding a new lifetime quota.

### Goal instruction text and criterion counts

Accept user goal text without a second arbitrary byte quota and keep its inert-text validation. Preserve actual completion criteria and counter correctness; elapsed time and usage observations are accounting, not default task budgets.

#### L032 — Goal text/count representation and checked counter exhaustion

**Current code:** `crates/app/peritus-product-runner/src/control/goal.rs` — `GoalCriterion.description`, `GoalRecord.objective`, `reason`; `goal/lifecycle.rs` — `start`; `goal/validation.rs` — `validate`; `control/text.rs` — `ControlText::new`; `goal/accounting.rs` and `goal/execution.rs` — checked usage counters.

**Proposed minimum fix:** Remove fixed user-objective/description byte admission and `u16` criterion-count checks, carrying the same text contract through public/UI consumers. Keep nonempty inert text, the mandatory RunnerAcceptance criterion, and checked arithmetic. Generated reason labels can stay short. This baseline has no lifetime request/tool/token quota in these counters; do not add a replacement budget.

### Align tool schemas and parsers with their actual downstream contracts

Remove independent policy maxima from tool argument admission and use a consistent representation throughout. Preserve declared types, closed schemas and native dimensions.

#### L042 — Tool argument schema, scope, and stdin capacities

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/arguments.rs` separately enforces depth 16 and caches a deterministic catalog result in `OnceLock`. `catalog.rs` caps scope lists at 256 and stdin at 65,536 characters; resize requires positive dimensions. The stdin process contract is in `command_runtime/plan.rs`.

**Proposed minimum fix:** Remove independent scope/depth policy maxima and align stdin admission by bytes with the shared I/O contract. Keep positive resize dimensions unless the backend supports zero. Catalog loading uses embedded static schemas, so no transient external failure is established: retain deterministic caching and fix invalid schemas rather than adding a retry loop.

### Remove task and deliverable policy quotas consistently

Use transport paging/references for large results, with matching writer and reader representation; retain genuine path and outcome validation.

#### L030 — Product task, detail, deliverable, and page admission

**Current code:** `crates/app/peritus-app-protocol/src/product.rs` declares 64-KiB task and 1-MiB detail bounds. `ProductDeliverable::checked` limits both path and command collections through `u16::try_from` and validates relative non-Git paths.

**Proposed minimum fix:** Remove arbitrary text limits and widen collection encoding/decoding together where it currently uses u16. Preserve path authority and required outcome facts. Keep the independent run-query page size.

### Remove incidental artifact metadata length caps

Keep syntax and identity validation independently of text length.

#### L070 — Artifact metadata has narrow textual capacities

**Current code:** `crates/state/peritus-artifact-store/src/metadata.rs::{MediaType::new,EncryptionMetadata::envelope}` caps media types at 255 bytes and algorithm tokens at 64.

**Proposed minimum fix:** Delete those length maxima while keeping nonempty valid token syntax and finalized healthy-reference requirements.

### Align memory-update schema and reducer admission

Remove duplicate synthetic request/text/list maxima together with working-state limits; keep source, permission, freshness and secret-handling constraints.

#### L079 — Memory updates have independent request/text/path admission gates

**Current code:** `crates/app/peritus-product-runner/src/local_context/tools/update.rs::{execute,prepare,label,entry}` imposes byte/list/path limits. `tools/schema.rs` publishes fixed character-count maxima for the same fields.

**Proposed minimum fix:** Remove the independent maxima in both schema and parser, and use the shared widened memory/JSON representation. For any retained explicit limit, advertise the same Unicode/byte unit actually enforced. Preserve revision, source and exact workspace authority checks.

### Use one tool JSON and schema admission contract

Remove fixed parser/schema policy maxima and align advertised numeric/string units with actual parsing. Preserve valid JSON, closed schemas and canonical identity.

#### L103 — Tool JSON envelopes have production ceilings that callers cannot widen

**Current code:** `crates/tools/peritus-tool-protocol/src/limits.rs::JsonLimits::new` only narrows 256-KiB/depth-32/member-4,096/string-64-KiB production bounds. `schema/canonical.rs` accepts only signed i64 integers. `call.rs::CallLimits` still requires finite timeout in this baseline.

**Proposed minimum fix:** Allow caller-admitted JSON capacity without those hard maxima and support schema-promised numeric values. Apply optional timeout through CallLimits/descriptors/canonical bytes with the command horizon repair; do not replace absence with u64::MAX.

#### L104 — Tool schemas add fixed property/enum bounds and byte-based exported string cardinality

**Current code:** `crates/tools/peritus-tool-protocol/src/schema/validate.rs` caps properties/enums/depth and validates string `value.len()` bytes. `schema/canonical.rs::schema_value` exports those fields as JSON Schema character lengths.

**Proposed minimum fix:** Remove independent property/enum/depth quotas and count Unicode characters for exported minLength/maxLength. Preserve ordering, uniqueness and explicit schema compatibility.

#### L108 — Shell schemas advertise inputs that the shared parser cannot admit

**Current code:** `crates/tools/peritus-tools-shell/src/catalog.rs::script_schema` advertises 256-KiB scripts, while shared JSON tokens and `CommandSpec` accept less. `input.rs::ScriptInput::new` currently inserts the entire script as an argv element.

**Proposed minimum fix:** Align parser/CommandSpec admission with the advertised input. When actual OS argv limits prevent it, use an explicitly selected interpreter script-file invocation through the structured interface; there is no generic script-file fallback in the current ScriptInput path to merely enable.

### Keep successful effects recoverable when their previews are large

Charge the actual encoded metadata envelope separately from referenced artifact bodies. Keep authoritative exit/output references even if a display rendering needs a smaller page.

#### L105 — Tool terminal/progress construction can fail after an effect because of rendering and output totals

**Current code:** `crates/tools/peritus-tool-protocol/src/result.rs::build` adds artifact body sizes to structured bytes, checks lifetime progress, and limits renderings. `identity.rs` hard-caps text/key lengths; `artifact.rs::ArtifactReference::new` rejects zero bytes.

**Proposed minimum fix:** Account envelope metadata separately from referenced bodies, remove lifetime progress and incidental text caps, and allow valid empty artifact references. Preserve provenance, canonical identity and checked counters; expose progress as retained pages.

#### L106 — Shell output can exhaust its result envelope after successful command completion

**Current code:** `crates/tools/peritus-tools-shell/src/execution/terminal.rs::build` passes published artifacts and structured metadata into that same result check; `catalog.rs` gives the whole output eight MiB.

**Proposed minimum fix:** Apply the shared envelope correction and preserve authoritative command exit/artifact references on preview errors. A successful process must not become an empty generic failed observation because referenced bodies consume envelope allowance.

#### L110 — Tiny selected shell preview limits cannot contain the mandatory empty-output marker

**Current code:** `crates/tools/peritus-tools-shell/src/render.rs::output` bounds tails before `checked_text` substitutes `(no output)`, which can exceed a tiny selected allowance.

**Proposed minimum fix:** Fit the empty marker and framing inside the selected encoded preview budget, keeping full terminal facts independent of preview construction. Retrieve only the required artifact ranges for product previews.

### Accept long conversations and distinguish repeated source occurrences

Preserve exact provenance in the existing obligations ledger while removing whole-history and incidental collection policy limits.

#### L118 — Whole conversation is capped at one MiB before a persistent run can execute

**Current code:** `crates/orchestration/peritus-obligations/src/limits.rs::production` sets one-MiB source/64-KiB clause limits. `provenance.rs::from_parts` rejects larger source; `ledger/validation.rs` enforces clause/draft/path limits. `crates/app/peritus-product-runner/src/execution/obligations.rs::capture` supplies the full conversation.

**Proposed minimum fix:** Remove whole-source and binding clause/path/schema-field policy caps, with matching specifications/reader contracts. Stream extraction/use existing source references for larger histories. Keep exact ledger/evidence identity; the default evidence count is not an independently reachable smaller bound.

#### L119 — Repeated conversation chunks collide as obligation identities and abort admission

**Current code:** `crates/app/peritus-product-runner/src/execution/obligations.rs::capture` hashes only each chunk's bytes for RequirementId, so repeated chunks receive the same ID.

**Proposed minimum fix:** Include exact source identity and byte offset in occurrence IDs, preserving compatibility with already recorded IDs. Surface detailed duplicate/provenance errors rather than generic admission failure.

### Control text and attachment descriptions must not inherit prompt quotas

Remove independent content policy limits from stored control data and calculate next-view admission separately. Keep exact versions, canonical identity, native representation checks and authority.

#### L154 — Brief, fork, and attachment selection impose fixed text/history representation limits

**Current code:** `crates/app/peritus-product-runner/src/control/{branch,brief}.rs` bounds text/lineage; `control/files/{source,version}.rs` constrains source/range to 64 MiB and selected bytes to attachment limits.

**Proposed minimum fix:** Remove fixed brief/title/objective/caption/label quotas and range rejection based on whole-source size. Use streamed selected ranges and immutable source references, with corresponding control serialization admission corrected. Keep exact digest/version binding and the separate-workspace requirement for writable forks.

#### L156 — Guidance and permission projections retain fixed text/schema bounds separately from authority

**Current code:** `crates/app/peritus-product-runner/src/control/guidance.rs::GuidanceContent` uses `ControlText<8192>`; `control/permissions.rs::{parse,apply}` validates a small canonical schema and host-restricted capability bits.

**Proposed minimum fix:** Remove arbitrary guidance and explanation text limits. The tiny canonical permission record, known capability bits, host ceiling and revision checks need no removal; migrate only a real supported prior schema.

#### L159 — Input admission retains and revalidates all history, with 65,535-item dependency and incorporation lists

**Current code:** `crates/app/peritus-product-runner/src/control/inputs/transition.rs::{apply,validate_revision,validate_bindings}` clones/revalidates history and checks dependency/incorporation lists with u16.

**Proposed minimum fix:** Remove per-input text and u16 list policy restrictions, using existing checked collection representation. Index existing history for revision lookups and validate only affected dependencies; keep corrections append-only and preserve real dependency ordering.

#### L163 — Review comments accept 8 KiB of text but their mandatory prompt wrapper must fit the same 8 KiB

**Current code:** `crates/app/peritus-product-runner/src/control/review.rs::ReviewComment` and `control/review/ledger.rs::prompt` both use `ControlText<8192>`, but the latter adds identity/path/action framing.

**Proposed minimum fix:** Remove the fixed comment/wrapped-prompt ceiling together with shared input text admission, or keep comment and metadata structured until request rendering. Count complete framing against actual provider capacity, not the comment's old allowance. Keep exact anchors and freshness.

### Public reply compaction and request accounting

Keep immutable reply artifacts and compact only the active view. Charge the actual selected replacement and framing, rather than the original historical bytes.

#### L155 — Public-reply compaction has fixed source/focus/replacement bounds and can reject a valid focus

**Current code:** `crates/app/peritus-product-runner/src/control/compaction.rs::{compact,deterministic_handle,validate_c6}` applies equal 1,024-byte focus/replacement caps, skips Capacity sources and configures a 1 MiB C6 source bound.

**Proposed minimum fix:** Remove independent focus/replacement/source/count quotas coherently with reply/control admission. Allow the wrapper in addition to the focus and return explicit per-source outcomes instead of silently skipping capacity failures. Retain digest/size binding, unique invocation identities and strictly smaller replacements.

#### L157 — Request capture checks full reply history before exclusions or compacted replacements

**Current code:** `crates/app/peritus-product-runner/src/control/inputs/capture.rs::capture_with_reply_view` precharges original reply strings; `crates/app/peritus-daemon/src/product_control/inputs.rs` filters eligible replies but still precharges original reference sizes before replacements.

**Proposed minimum fix:** Remove both original-byte precharges; assemble and budget the effective compacted/excluded view. The daemon already filters ineligible replies, so do not claim every historical reply is charged there. Keep immutable history and select user-input history within actual provider context as well.

#### L178 — Public-reply compaction is unavailable while execution remains incomplete

**Current code:** `crates/app/peritus-daemon/src/product_control/compaction.rs::compaction_preview` returns no proposal whenever `execution_complete` is false, even for immutable older replies.

**Proposed minimum fix:** Allow compaction of immutable completed replies while a run is incomplete, without altering its active invocation. Apply L157's actual-view accounting correction so compaction can relieve admission; keep pinned replies and source artifacts.

#### L322 — Legacy deterministic compaction retains an unbounded prompt lineage until it stops shrinking

**Current code:** `crates/orchestration/peritus-agent/src/developer/context.rs::append_message_summary` recopies old lineage and preview lines; compaction_candidate stops at the first incomplete unit.

**Proposed minimum fix:** Replace recursive lineage copying with a retrievable reference to exact archived source plus the current summary. Ensure legacy fallback archives and exposes that source through the existing durable context/trace owner before dropping it; a digest alone is not retrieval. Compact later eligible complete exchanges without crossing unresolved tool protocol obligations and widen the u16 source-count representation compatibly.

#### L323 — Model output limiting can hide the control identities and terminal status of an oversized tool result

**Current code:** `crates/orchestration/peritus-agent/src/developer/observation.rs` replaces oversized projected JSON with generic head/tail metadata, potentially hiding command handle/state; context_port.rs adds source metadata afterward.

**Proposed minimum fix:** Preserve a typed mandatory control envelope containing handle, state, terminal/uncertain truth and recovery fields. Shorten only optional output, keep a usable durable source reference, and include all added metadata in physical prompt admission. Reuse the existing exact trace/context storage rather than raising the preview quota.

### Page context inspection before rendering

Inspection must remain usable when next-request admission is blocked; keep bounded pages without building the complete presentation first.

#### L179 — Context inspection builds the complete next-view rows before pagination

**Current code:** `crates/app/peritus-daemon/src/product_control/context.rs::{context_page,next_rows,image_rows,page}` builds the entire Next/Invocation row vector before slicing; History already selects a page before loading manifests.

**Proposed minimum fix:** Select Next/Invocation rows before expensive rendering and build eligibility sets once. Preserve History's existing early paging and allow retained-history inspection when next capture fails, reporting the precise admission failure rather than making all inspection depend on a new request.

### Remove compiled configuration inventory and resource maxima

Use selectable backpressure settings and optional requested quotas. Keep unique identities, protected storage and actual integer constraints.

#### L202 — Daemon configuration hard-caps queues, connections, workers and shutdown duration

**Current code:** `crates/app/peritus-daemon/src/config.rs::DaemonLimits::{PRODUCTION,validate}` fixes queue/connection/worker maxima, artifact quotas and a required 30-second default shutdown with a ten-minute cap; `startup/runtime/runner.rs` passes them downstream.

**Proposed minimum fix:** Remove absolute configuration maxima and unrequested artifact quotas; permit optional shutdown coordination waits. Keep practical queue capacities as selectable backpressure settings, real integer constraints and protected storage. Do not replace these caps with new lifetime allowances.

#### L203 — Configuration limits project/workspace/provider/tool/direct-folder inventories and fixes storage layout

**Current code:** `crates/app/peritus-daemon/src/config/{catalog,folder,provider}.rs::validate` caps inventories/tool names/protected paths; `config/paths.rs::validate` keeps protected roots nonoverlapping beneath state_root.

**Proposed minimum fix:** Remove fixed inventory and protected-path-count maxima and arbitrary tool-name length caps. Retain unique normalized paths, observed direct-folder identity and protected-root confinement; storage relocation needs no change unless an actual supported deployment requires it.

#### L208 — Startup provider registry admits only 64 profiles despite accepting 256 in config

**Current code:** `crates/app/peritus-daemon/src/component/providers.rs::{ProviderRegistryLimits,ProviderRegistry::build,current_provider}` defaults to 64 profiles despite config admitting 256, and cannot select among multiple revisions; `component/inventory.rs::build` fails the full registry.

**Proposed minimum fix:** Remove the registry's narrower fixed profile ceiling and use the admitted configuration. Designate the active revision explicitly while retaining exact historical lookup; report one unavailable route without disabling unrelated valid providers.

### Provider transport must not kill inference for buffers or hidden deadlines

Use optional explicitly selected deadlines and stream/spool output without cumulative diagnostic quotas. Preserve cancellation, exact executable identity, protocol integrity and HTTP authority boundaries.

#### L209 — Provider transport limits still impose hard byte ceilings even with no execution deadline

**Current code:** `crates/model/peritus-provider-core/src/process.rs::ProcessLimits` currently requires Duration and defaults to ten minutes, 16 MiB stdin/stdout and 64 KiB stderr; `http/limits.rs::HttpLimits::new` can only narrow compiled byte limits.

**Proposed minimum fix:** Make subprocess timeout optional and absent by default, updating stdin/collect waits and native callers. Remove synthetic payload/cumulative output and HTTP upper clamps; use actual admitted representations and streaming storage. The audit's separate `process/limits.rs` is absent here, and the current mandatory timeout must also be repaired.

#### L210 — Generic provider subprocess overflow kills the invocation, including stderr-only overflow

**Current code:** `crates/model/peritus-provider-core/src/process/tokio_transport.rs::{run,collect_output,read_bounded}` terminates on output overflow or timeout; `process.rs` caps argv/environment and `process_containment.rs` kills on drop/parent death.

**Proposed minimum fix:** Spool full stdout/stderr to existing per-run artifact storage and parse incrementally; retain bounded read buffers without cumulative kill thresholds. Remove arbitrary argv/environment quotas and return actual OS launch errors. Use the shared persistent-owner repair for recoverable inference across host death; current kill-on-drop/PDEATHSIG ownership does not provide reattachment.

#### L225 — HTTP pulls reject valid bodies based on upstream chunk boundaries, while default transport has no selected timer

**Current code:** `crates/model/peritus-provider-core/src/reqwest_transport.rs::{new,with_timeouts,send,ReqwestByteStream::next}` requires connect/header/idle deadlines, a request deadline and rejects an upstream chunk above max_chunk_bytes.

**Proposed minimum fix:** Slice upstream chunks into pull-sized pieces and remove independent cumulative body ceilings. Make connect/header/idle/overall timers explicit optional caller selections with no synthetic default or compiled ten-minute maximum, propagating optionality through request/stream waits. This baseline still has timers, unlike the audit's later description; preserve cancellation, headers, TLS and request acceptance facts.

#### L476 Compatible wire and terminal formats narrow otherwise accepted content capacities

**Current code:** `peritus-provider-compatible/src/request.rs:63` materializes JSON before HttpRequest admission; stream terminal forms repeat complete text/arguments under smaller framing limits.

**Proposed minimum fix:** Align encoded request and terminal-frame admission with selected capacities, removing narrower policy clamps. Count Base64/JSON overhead before submission and stream encoding, retaining exact complete-content comparison.

### Native inference envelope and compatibility

Keep native effects disabled and return only host-authorized tool requests. Relax synthetic representation/lexical gates without allowing prose to become executable operations.

#### L214 — Native adapters enforce a single inert inference boundary and transport/control feature restrictions

**Current code:** `crates/model/peritus-provider-anthropic/src/runtime/provider.rs::run_turn` sets --max-turns 1 with native tools disabled; `runtime/request.rs::validate_controls` and OpenAI `runtime/request/schema.rs::result_contract` define the inert envelope.

**Proposed minimum fix:** Keep one inference per host turn and native tool denial. Remove independent stdin policy size admission through L209/L210; allow empty public content with valid host tool calls (Claude output currently rejects it). Return precise unsupported control/media errors instead of silently substituting semantics.

#### L216 — Claude completion admission depends on exact native fields and a lexical prose classifier

**Current code:** `crates/model/peritus-provider-anthropic/src/runtime/output.rs::{decode,direct_turn,classify_reported}` and `output/completion.rs::{public_text,private_candidate}` use exact completion fields and punctuation/prose heuristics.

**Proposed minimum fix:** Use explicit native envelope structure rather than punctuation in public prose to select private decoding. Accept documented compatible completion fields, preserve exact session identity and host-tool authority, and carry typed upstream failures instead of lexical guesses where available.

#### L218 — Codex decoding caps raw events, event size and completed assistant messages, with closed event kinds

**Current code:** `crates/model/peritus-provider-openai/src/runtime/output.rs::{decode_turn,decode_event,decode_item}` imposes 100,000 lines, 4 MiB lines and 1,024 messages; `output/selection.rs::select_turn` rejects multiple messages without final text.

**Proposed minimum fix:** Remove independent line/message/event count limits and align with shared protocol representation. Preserve harmless unknown notifications as opaque records while rejecting native effects and identity/lifecycle contradictions. Select exact terminal evidence when the final file is missing; do not guess a successful turn from incomplete output.

### Provider model catalogs need continuation rather than total quotas

Keep explicit model selection usable independently of catalog discovery. Isolate bad metadata while preserving origin and cursor integrity.

#### L227 — Provider catalog discovery fails entire results at fixed page, byte, frame and entry ceilings

**Current code:** `crates/model/peritus-provider-core/src/catalog/{http,runtime,parse}.rs` caps pages, frames, total bytes, models and waiting notifications; runtime discovery also wraps query in a fixed 30-second timeout.

**Proposed minimum fix:** Remove total catalog/page/model/notification quotas and the synthetic 30-second discovery deadline, using cancellable waiting and returned page continuation. Isolate malformed entries while preserving errors and same-origin/repeated-cursor checks. Metadata-only processes can still be reaped after completion/cancellation; they do not need inference-session persistence. Remove small label/cursor text policy caps coherently with protocol admission.

#### L474 Compatible catalog discovery rejects safe configured queries and custom operation paths

**Current code:** `peritus-provider-compatible/src/config.rs:175` stores an exact operation URL but catalog derivation uses restricted suffix handling; catalog lookup can reject configured queries/custom paths.

**Proposed minimum fix:** Compose catalog URLs from parsed path components or an explicit configured catalog endpoint. Permit exact model-ID selection when discovery is unavailable, retaining reviewed origin/credential rules.

### Align prompt admission and preserve terminal control authority

Derive editor and transfer admission from the negotiated field and owner contract, while retaining signature and exact-process checks.

#### L244 — Prompt and terminal capacities differ across UI, wire and ownership layers

**Current code:** `crates/app/peritus-tui/src/model/interaction/prompts.rs` checks signed decisions against max_frame_bytes; `crates/app/peritus-app-protocol/src/wire/prompt.rs` and schema/fields/flows/prompt_terminal/prompt.rs use max_opaque_bytes. `model/interaction/terminal.rs` sends one input chunk and gates cancellation on can_capture.

**Proposed minimum fix:** Use max_opaque_bytes plus the broker's actual answer allowance for signed payload admission, accounting for Base64 expansion before paste. Share ordinary answer limits through editor/wire/broker. Chunk terminal input under the negotiated chunk limit and recover the same attachment or exact authorized process control when output/attachment is lost; keep native dimensions and signed identity checks.

#### L253 — Editor limits preserve rejected drafts, but signed-approval capacity is checked against the wrong layer

**Current code:** `crates/app/peritus-tui/src/model/editor/input.rs` uses the 8192-byte WorkbenchInputText bound and Base64 size derived from max_frame_bytes. `terminal/line_input.rs` freezes the editable text while pending and silently ignores oversized paste.

**Proposed minimum fix:** Remove the 8 KiB workbench text policy consistently through the shared text/protocol contract, not just the editor. Derive signed paste size from max_opaque_bytes and the broker allowance. Separate submitted pipe input from the next draft, correlate its acknowledgment and visibly reject oversized paste; do not duplicate ambiguous input.

### Align durable SQL admission with public field contracts

Change only mismatched policy widths and units; migrate affected constraints with the API change. Retain native integer, identity and atomicity checks.

#### L250 — Journal schema has narrower durable identity fields and all-history state retention

**Current code:** `crates/state/peritus-journal/src/sqlite/schema.rs` bounds idempotency keys to 256 bytes, prompt answers to 1 MiB and history to level four/16 MiB. `sqlite/append.rs` invalidates replay_generation on every attempt before applying exact CAS/idempotency checks.

**Proposed minimum fix:** Align idempotency key, answer and other field widths with the values actually admitted by their public APIs; use byte units consistently in Rust and SQL. Bundle history constraints with L256 and prompt limits with L244/L259. Keep checked i64 storage ranges and replay reobservation after append failures; no history deletion or new retention system is required.

### Remove collaboration history quotas and false payload accounting

Retain exact causal/task/message history and authorization. Actual transport/storage bounds must describe encoded bytes, not referenced content.

#### L299 — Collaboration has nine immutable positive quotas and retains terminal/acknowledged history

**Current code:** `crates/orchestration/peritus-collaboration/src/limits.rs` requires nine immutable quotas; reducer/apply.rs counts terminal children/tasks and acknowledged messages/artifact handoffs against lifetime admission.

**Proposed minimum fix:** Remove lifetime task/message/fan-out/artifact quotas and arbitrary structural policy ceilings across constructors, reducers and decoder validation. Keep retained history and existing digests; share state/codec expansion where collections outgrow a frame. Any representation/semantics change needs compatibility under the same run identity, not in-place binding substitution or an archival subsystem.

#### L300 — Collaboration byte checks charge external payload sizes to small digest-only records

**Current code:** `crates/orchestration/peritus-collaboration/src/reducer.rs::estimated_command_bytes` charges payload_bytes+768 and state.rs::estimated_encoded_bytes sums referenced bodies. wire/mod.rs::write_message writes only length/digest and metadata.

**Proposed minimum fix:** Remove heuristic metadata-envelope admission and measure actual encoded bytes. Stop charging external payload bodies to digest-only command/state records. Keep any explicit external storage policy with the real content owner; this change must not invent a new metadata quota.

#### L301 — Collaboration checks successor size before adding the command receipt and can persist an unreadable checkpoint

**Current code:** `crates/orchestration/peritus-collaboration/src/reducer.rs` checks ensure_state_bound before advance_cursor; state/validation.rs repeats the heuristic after the new command is present.

**Proposed minimum fix:** Delete the heuristic gate with L300. Validate the exact final post-cursor successor before append using the decoder's remaining invariant rules, retaining atomic rejection and a usable predecessor.

#### L302 — Collaboration confuses message identity order with causal order, rejecting valid successors or saving an invalid chain

**Current code:** `crates/orchestration/peritus-collaboration/src/reducer/apply.rs::send_message` uses task_messages.last() from ID-sorted storage. state/validation.rs instead requires predecessor.ordinal+1==ordinal.

**Proposed minimum fix:** Select the predecessor by greatest causal ordinal for that task, not greatest message ID. Check contiguous ordinal/predecessor before insertion and on restart using the same helper; retain opaque identity sorting separately.

#### L303 — Production framing imposes a hidden 65,535-command lifetime on collaboration, including cancellation and acknowledgments

**Current code:** `crates/orchestration/peritus-collaboration/src/wire/state.rs` writes all used_commands with write_collection_len; durability.rs uses CodecLimits::PRODUCTION, so the shared 65535-item and 16 MiB limits override larger declared collaboration capacities.

**Proposed minimum fix:** Bundle removal of collection/frame policy ceilings with L009 and expansion of journal state with L256. Make writer and reader agree and retain all exact command IDs. Keep checked native length/allocation integrity; do not prune or add another lifetime frontier merely to bypass this count.

#### L305 — Collaboration rereads, clones and validates lifetime history synchronously, with quadratic validation paths

**Current code:** `crates/orchestration/peritus-collaboration/src/state/validation.rs` checks command duplicates against every earlier prefix and scans tasks per task. canonical.rs/encoder.rs allocate full hash bytes; durability.rs loads all events.

**Proposed minimum fix:** Use sets/keyed derived indexes for uniqueness, child/work/causal lookups and reuse validated checkpoint suffixes. Stream the identical canonical hash bytes and remove redundant copies/validation, keeping persistent evidence and integrity.

### Remove independent context quotas and recover the actual model view

Keep durable source identities, trust, protected instructions and dependency obligations; only the materialized model view must fit the provider window.

#### L349 C6 context construction imposes four finite collection/content bounds before planning

**Current code:** `peritus-context/src/content.rs:81` requires finite positive graph/content/dependency/visibility maxima before selection; graph/node constructors enforce them.

**Proposed minimum fix:** Remove arbitrary inventory/content/dependency ceilings and use verified source/range references where full materialization is unnecessary. Stream content hashing, retain graph integrity and authority rules, and make allocation failure recoverable without replacing the session.

#### L350 C6 selection rejects required context on independent finite node and byte ceilings

**Current code:** `selection.rs:39` adds finite node/byte ceilings independent of tokens; planner and certificate enforce them on required closures.

**Proposed minimum fix:** Remove independent node/byte rejection where the actual token window fits. When it does not, use range/derived-view recovery over retained originals; change planner and certificate together, preserving required obligations.

#### L352 C6 an oversized optional estimate can fail the whole selection before omission

**Current code:** `selection/closure.rs:118` and `selection/plan.rs:165` checked-add optional costs before comparing with headroom; `selection/certificate.rs:159` repeats that whole-plan failure.

**Proposed minimum fix:** Compare each optional cost against remaining capacity before accumulation, returning an omission when it cannot fit. Apply the same semantics inside closure accumulation and certificate replay; accepted totals remain exact and required-capacity failures stay explicit.

#### L353 C6 compaction cannot replace required sources and requires an already selected source set

**Current code:** `compaction/validation.rs:108` requires sources already selected, while replacement rejects required sources even when validation succeeded.

**Proposed minimum fix:** Allow capacity-recovery planning from durable sources before a fitting view exists. Permit a checked derived representation of required evidence while retaining exact originals, source lineage and required obligations. Keep protected instructions and authority boundaries intact.

#### L354 C6 reducing compaction can fail on the union of individually admissible dependencies

**Current code:** `compaction/replacement/dependencies.rs:21` unions external dependencies; replacement reconstructs metadata under the original finite per-node ceiling and rebuilds the graph.

**Proposed minimum fix:** Remove that independent dependency ceiling in construction and replacement together, retaining the complete canonical union. Reuse unchanged nodes and validate only changed edges with an equivalent certificate; do not drop dependencies to fit.

### Remove E1 policy ceilings consistently from admission through restore

Reuse the existing harness graph/history/materialization APIs. Remove arbitrary quotas across all representations rather than adding replacement history or batching systems.

#### L355 E1 harness configuration has eleven compiled ceilings that can only tighten

**Current code:** `peritus-harness/src/domain/limits.rs:7` has eleven private finite capacities, compiled ceilings and a tighten-only API.

**Proposed minimum fix:** Remove lifetime history/receipt/diagnostic quotas and arbitrary manifest/component/byte ceilings. Permit explicit operational capacity selection/amendment and align C1/codec acceptance; keep resource backpressure and artifact integrity rather than introducing a new storage architecture.

#### L357 E1 feature lists can pass construction and encoding but fail canonical reconstruction

**Current code:** `domain/component_canonical.rs:128` decodes feature lists using the unrelated dependency-edge limit; constructors/encoder do not enforce that relation.

**Proposed minimum fix:** Delete the edge-based feature rejection and use identical feature semantics at public construction and restore. Keep canonical feature ordering and validity, and ensure the final stored graph is reconstructible.

#### L358 E1 revision history has lifetime count and whole-snapshot byte exhaustion without retirement

**Current code:** `domain/history.rs:65` checks revision count and serializes all history on append. `decode_canonical_snapshot:108` rebuilds by repeatedly appending/re-encoding prefixes.

**Proposed minimum fix:** Remove lifetime revision and combined-snapshot policy ceilings with the shared codec repair. Decode/validate retained history once, index revisions/ancestors and preserve complete lineage; no replacement history system or pruning is required for the minimum repair.

#### L360 E1 full diagnostic history prevents terminal failure or conflict settlement

**Current code:** `aggregate/reducer/settlement.rs:113` rejects retained diagnostics at capacity after removing pending work only in the tentative clone.

**Proposed minimum fix:** Remove the failure-count quota so true terminal failure/conflict always clears its pending plan. Retain fixed-size diagnostic references and the already available successful-receipt retirement path.

#### L361 E1 pending-plan admission omits a limit that checkpoint decoding enforces

**Current code:** `aggregate/reducer/settlement.rs:15` permits plans in distinct workspaces; checkpoint decoding separately borrows receipt-history capacity to reject pending cardinality.

**Proposed minimum fix:** Remove the decoder-only pending-plan cap and make final-state admission/restore agree. Keep one pending materialization per workspace as an effect-serialization rule.

#### L362 E1 sixteen-MiB policy is narrowed by nested eight-MiB codec fields

**Current code:** `aggregate/state.rs:173` encodes full history/state through production opaque fields; `wire/state.rs:52` wraps complete state again. Codec limits are narrower than E1's advertised 16 MiB policy.

**Proposed minimum fix:** Coordinate nested field/collection capacity removal with E1 admission and the shared codec representation. Classify precommit capacity failure separately from corrupted stored input; retain plans, history and retired receipt identities.

#### L367 E1 planner can admit more file operations than its C1 executor permits

**Current code:** `materialization/planner.rs` can emit replacement installs plus old-file deletes beyond C1's 1,024 operations; execution constructs the PatchSet only later.

**Proposed minimum fix:** Apply the shared patch-operation ceiling removal to the executor and ensure plan/executor acceptance agrees. Keep the complete owned atomic replacement; do not add multi-batch recovery solely to work around the arbitrary count.

#### L368 E1 component inventory imposes an undocumented directory-to-file ratio ceiling

**Current code:** `manifest/inventory.rs:24` limits descendant entries to eight times component capacity and reports excess as UnsafeEntry.

**Proposed minimum fix:** Delete the directory/file ratio quota. Enumerate incrementally with the existing no-follow containment and exact declared-inventory checks; ordinary directory count is not an unsafe entry.

#### L369 E1 loader enforces aggregate payload capacity after reading all component bytes

**Current code:** `manifest/loader.rs:61` reads all payloads before LoadedHarness::check validates graph relationships and totals.

**Proposed minimum fix:** Validate declaration relationships and selected operational capacities before payload I/O. Stream content through existing artifact paths with exact size/digest verification; remove arbitrary payload quotas through the common policy change rather than adding another loader limit.

### Remove debugger job and complete-report policy quotas

Keep exact diagnostic evidence and inert proposal validation; model windows and explicitly selected operational capacities remain separate from job lifetime.

#### L415 E2 debugger jobs freeze 24 compiled ceilings that callers can only reduce

**Current code:** `peritus-debugger/src/limits.rs:95` fixes 24 compiled ceilings, including eight attempts, seven retries and accounted 24-hour usage; tightened() cannot increase them.

**Proposed minimum fix:** Remove tighten-only workload/history/retry/accounted-time ceilings and make unrequested budgets absent. Align constructors, model policy and readers with selected operational capacities; the accounted-time field alone is not a live watchdog.

#### L424 E2 complete-report caps make long or recurrent evidence unanalyzable without lossless partitioning

**Current code:** `causal/analyzer.rs:112` rejects completed findings/causes by count; clustering and `report/validation.rs:159` enforce complete member/citation/report size policies.

**Proposed minimum fix:** Remove complete-report/member/citation stopping quotas consistently with debugger policy. Retain every exact citation and contrary fact in existing evidence storage; use continuation for large report views rather than dropping or partitioning away evidence.

#### L426 E2 supported observations are rejected by five English substring heuristics

**Current code:** `report/claim.rs:56` rejects observations containing five English substrings; `causal/candidate.rs:17` caps statements at 4,096 UTF-8 bytes.

**Proposed minimum fix:** Delete substring-based epistemic rejection and arbitrary statement length. Use typed Observation/Inference and exact citation validation, preserving safe display/control-character handling.

#### L427 E2 model schema and semantic statement limits measure different units

**Current code:** `model/plan.rs:21` declares JSON Schema maxLength 4096, while DiagnosticText validates 4,096 bytes rather than Unicode characters.

**Proposed minimum fix:** Remove the inconsistent independent length caps in schema and semantic validation together, or use identical explicitly selected units. Validate actual encoded size under shared protocol admission, retaining strict inert output and provenance.

### Remove stream identity and observation count exhaustion

Preserve exact deduplication/conflict checks and all semantic events; accumulated telemetry must not terminate valid inference.

#### L482 Anthropic streaming has a fixed 4096-entry deduplication ceiling and derived identities can exceed the protocol width

**Current code:** `peritus-provider-anthropic/src/stream/state.rs:128` rejects the 4,097th SSE identity; content/helpers independently use PRODUCTION and derive suffixed IDs.

**Proposed minimum fix:** Remove the dedup-count stopping quota, retain conflicting-identity protection and derive fitting IDs through the shared normalized-ID helper. Pass the selected protocol policy through content/tool/replay construction.

#### L489 Google streams repeat compiled deduplication and terminal-publication ceilings, and repeated cache observations hit a separate 64-event gate

**Current code:** `peritus-provider-google/src/stream/state.rs:124` caps dedup at 4,096; `peritus-model-protocol/src/reducer/events.rs:46` rejects the 65th cache/rate observation.

**Proposed minimum fix:** Remove identity/ancillary count exhaustion, retaining dedup conflicts and useful high-water observations. Apply the shared parsed-batch terminal boundary; optional telemetry history must not fail valid output.

### Align plugin SDK quotas and durable wire acceptance

This is a future extension boundary; repair its existing types and host together without inventing production custody.

#### L506 Plugin SDK requires finite deadlines and lifetime request quotas

**Current code:** `peritus-plugin-sdk/src/manifest.rs:194` requires finite invocation_millis/lifecycle_requests; `protocol.rs:22` requires deadline_millis in every InvocationContext.

**Proposed minimum fix:** Make unrequested deadlines and lifetime request enforcement optional across manifest, invocation context, wire version and host interpretation. Keep selectable frame/output/concurrency capacities and authority-selected validity.

#### L507 Plugin payload and frame constructors disagree with fixed deserialization limits

**Current code:** `payload.rs:89` always Serde-decodes with PRODUCTION; :108 rejects all floating values, while encoding can use selected limits and framing adds separate capacity rules.

**Proposed minimum fix:** Pass the same selected payload policy into encode/decode, remove forced production clamps and support the finite JSON number domain required by tools. Detect duplicates during parsing and validate complete encoded frames consistently.

### Remove plugin lifetime stops and repair owned cancellation

Use the existing process owner and restart lifecycle. No production persistence framework is justified without an actual consumer and custody/identity contract.

#### L509 Plugin host lifetime quota can be exhausted by rejected work

**Current code:** `peritus-plugin-host/src/quota.rs:16` increments lifecycle count before active admission; `host.rs:230` always creates finite response timeout from narrowed manifest quotas.

**Proposed minimum fix:** Remove unrequested lifetime quotas and compulsory response timeout. Reserve active slots only for admitted work, release rejected admission and validate configuration while retaining explicit authority semantics.

#### L510 Plugin cancellation and deadline handling can hang before termination

**Current code:** `transport.rs:91` waits uncancellably for transaction lock and request write; cancellation waits for another cancellation-frame write before terminate(), which suppresses kill/wait errors.

**Proposed minimum fix:** Make lock acquisition and writes cancellation-aware, retaining partial-write uncertainty. Control the owned child without awaiting a blocked cancel-frame write; reap it and report kill/wait failures truthfully. Removing read timeout needs no replacement deadline.

#### L511 Plugin failed-state handling and queued authority cannot support durable recovery

**Current code:** `host.rs:210` checks Ready and authority before transaction serialization, then treats broad exchange failures as instance failure; RestartPlugin has no equivalent recovery transition.

**Proposed minimum fix:** Recheck lifecycle/authority after transaction acquisition and distinguish definitely unsent validation/cancellation from protocol/process failure. Add an actual restart transition matching recovery guidance and preserve terminate/reap errors; do not build speculative durable plugin custody.

#### L512 Plugin discovery cardinality and whole-catalog failure can block unrelated extensions

**Current code:** `discovery.rs:141` aborts the whole catalog on root/entry/identity failures and fixed counts; load_plugin at :187 caps complete manifest/artifact bytes.

**Proposed minimum fix:** Remove fixed discovery inventory/byte quotas, enumerate incrementally and retain valid entries with per-entry diagnostics. Reverify current trusted executable identity before launch, keeping explicit trust and conflicting-ID checks.

### Keep trace recording from blocking execution on incidental observation state

Reuse exact durable causal facts, separating stale proposed input from invalid committed history.

#### L519 Trace observations and sensitive-value handling have fixed collection and byte ceilings

**Current code:** `peritus-trace/src/domain/observation.rs:88` caps collections; `redaction.rs:215` requires a complete SensitivePayload before omission/hash binding.

**Proposed minimum fix:** Permit streaming omission/hash verification of large sensitive values without first admitting a full allocation. Validate selected domain collection shape before allocation, retaining vault privacy, unique keys and actual enum cardinalities.

#### L520 Trace recording treats wall-clock regression as an invalid lifecycle transition

**Current code:** `projection/fold.rs:70` rejects either wall or monotonic time regression as a transition error.

**Proposed minimum fix:** Order lifecycle by durable sequence and monotonic generation; keep wall timestamps descriptive and record clock discontinuity explicitly. A wall-clock adjustment must not reject truthful work observations.

#### L522 Trace causal contracts require the exact latest event and classify missing predecessors as terminal integrity

**Current code:** `projection/fold.rs:23,70` requires exact latest parent/predecessor; `error.rs:197` labels missing causal facts TerminalIntegrity even for proposed uncommitted work.

**Proposed minimum fix:** Return Reobserve for a stale uncommitted parent/predecessor plan and rebind from current facts. Reserve integrity failure for invalid committed history, preserving exact causal bindings and duplicate resolution.

### Sandbox representation and platform semantics

Remove arbitrary contract collection quotas consistently with backend encodings; retain canonical, valid native objects and actual isolation boundaries.

#### L558 Sandbox contracts impose fixed rule, requirement and representation ceilings

**Current code:** `peritus-sandbox/src/{requirements,filesystem,environment,network,secret,process_policy,backend}.rs` applies fixed collection/string bounds; requirements and filesystem count before deduplication.

**Proposed minimum fix:** Canonicalize/deduplicate before admission, remove arbitrary rule/requirement ceilings and align native readers with admitted collections. Preserve actual protocol/native name validity and checked lengths.

#### L559 Logical environment and path normalization can disagree with native lookup semantics

**Current code:** `peritus-sandbox/src/environment.rs::EnvName::new` uppercases every name; filesystem logical normalization is shared across native backends.

**Proposed minimum fix:** Preserve case on Unix; use Windows name equivalence only on Windows. Bind filesystem authorization to the same canonical native path representation used for enforcement, preserving no-follow checks.

#### L560 Public IP-prefix construction bypasses the checked constructor and can panic during matching

**Current code:** `peritus-sandbox/src/network.rs::HostMatcher::IpPrefix` is publicly constructible although only its helper checks prefix width; matching subtracts unchecked widths.

**Proposed minimum fix:** Make the checked prefix value opaque or validate every matcher at contract ingress before matching/encoding; reject invalid widths without panicking.

### Linux plan projection and compatibility

Reject genuinely unrepresentable isolation before effects; make representable admitted plans reach execution without narrower helper quotas or lossy paths.

#### L570 Linux filesystem projection rejects several authorized creation plans

**Current code:** `peritus-sandbox-linux/src/filesystem.rs::project` canonicalizes every target before projection and rejects exact create/remove and exact directories.

**Proposed minimum fix:** For creation, validate the existing parent and authorized child operation without requiring the new target to exist. Check representability before allocating private state; retain no-follow isolation and clearly reject capabilities Landlock cannot provide.

#### L571 Linux native manifests have fixed bounds that can conflict with accepted plans

**Current code:** `peritus-sandbox-linux/src/{canonical,manifest}.rs` limits protocol to 1 MiB, collections to 256 and protected payloads to 128; runner converts paths with to_string_lossy.

**Proposed minimum fix:** Remove narrower helper collection ceilings in concert with L558 and preflight the complete projected manifest. Encode native paths losslessly and use checked framing rather than silently alter accepted paths.

#### L572 Linux syscall admission is a fixed compatibility gate

**Current code:** `peritus-sandbox-linux/src/native/seccomp_policy.rs` compiles a fixed syscall allowlist installed by the helper.

**Proposed minimum fix:** Keep syscall isolation. Add only a concretely required, authorized missing syscall to the existing policy when a workload demonstrates the gap; this finding alone does not justify deleting the gate or adding a policy framework.

### macOS contract and helper representation

Use one preflighted representation from checked plan through helper decode; remove incidental quotas while preserving Seatbelt representability and protected material identity.

#### L590 macOS manifests can encode a body that their decoder rejects

**Current code:** `peritus-sandbox-macos/src/manifest.rs::encode` writes opaque body bytes while `canonical.rs` readers apply their byte/string bounds; manifest codec uses that narrower reader.

**Proposed minimum fix:** Give opaque body framing matching writer/reader admission, including checksum overhead. Remove the narrower decode-only body restriction and preflight the final manifest before launch authority consumption.

#### L591 macOS profile compilation narrows several accepted contract capabilities

**Current code:** `peritus-sandbox-macos/src/filesystem.rs::{compile,validate_filesystem_representability}` has bounded profiles/roots and requires discovery/metadata together; environment/config fields add independent quotas.

**Proposed minimum fix:** Remove arbitrary profile/root/environment quotas after deduplication and preserve Unix case identity. Preflight actual Seatbelt/network representability; retain protected roots and deny-default isolation.

#### L598 macOS public helper-launch assembly has an independent fixed descriptor ceiling

**Current code:** `peritus-sandbox-macos/src/process.rs::HelperLaunch::new` builds a separate descriptor list with MAX_INHERITED_DESCRIPTORS and omits the production exec-status representation.

**Proposed minimum fix:** Route public helper assembly through the production NativeLaunchDescription, including status custody. Remove the separate 64-descriptor policy ceiling; keep native descriptor range/uniqueness checks.

#### L603 macOS protected-secret metadata has separate finite capacities

**Current code:** `peritus-sandbox-macos/src/secret.rs::{SecretHandleDescriptor::new,canonical_secret_handles}` caps labels/count/payload; duplicate labels are checked only on descriptor-sorted neighbors.

**Proposed minimum fix:** Remove independent policy quotas and validate descriptors, labels and destinations globally before allocation. Keep exact payload and protected-handle checks, including actual native descriptor range.

### Windows helper framing and protected material

Make encoding, transport and native delivery agree on accepted data; preserve exact lengths, platform identity and cleanup ownership.

#### L609 Windows manifest encoding and transport disagree at their maximum size

**Current code:** `peritus-sandbox-windows/src/manifest/codec.rs` appends a checksum beyond its frame bound while process transport has a separate maximum; manifest/process/secret/profile constructors add argv/handle quotas.

**Proposed minimum fix:** Share one complete framed capacity including checksum, remove artificial independent collection quotas and normalize all environment identities with Windows semantics. Make framed I/O cancellable and preserve dimension-specific errors.

#### L612 Windows secret reader rejects material exactly at its advertised one-MiB boundary

**Current code:** `peritus-sandbox-windows/src/native/secret.rs::read_bounded` fails when len equals MAX_SECRET_BYTES before testing EOF; stage closes source before checking read result and writes destination directly.

**Proposed minimum fix:** Probe one extra byte to distinguish exact maximum from overflow, retaining source custody until settled. Exclusively create private staged files and register cleanup before writes; propagate failure with exact ownership.

### Application catalog and history pagination

Keep bounded transfer pages while removing whole-history capacity gates. Use stable snapshot identity and keyed validation so scale does not erase authoritative data.

#### L693 Model catalogs and product observations have whole-response capacity boundaries without pagination at those DTOs

**Current code:** `peritus-app-protocol/src/product/models.rs::ProductModelCatalog::new` bounds whole models/error; snapshot/doctor DTOs bound complete observations.

**Proposed minimum fix:** Page catalogs and large observations by actual encoded bytes, preserving retained data. Use sets for doctor duplicate checks; report capacity cannot invalidate the run.

#### L699 Improvement inbox uses whole bounded collections with quadratic duplicate validation

**Current code:** `peritus-app-protocol/src/improvements.rs` limits candidates/evidence to u16 and checks duplicates with nested prefix scans.

**Proposed minimum fix:** Page existing improvement history, remove total-candidate/evidence/text policy quotas and use keyed duplicate validation. Preserve evaluation authority and complete source evidence.

#### L704 Context inspection has fixed source-size and sealed-message ordinal gates

**Current code:** `peritus-app-protocol/src/workbench/context.rs::WorkbenchContextRow::new` rejects metadata bytes over 64 MiB and message ordinal >=4096.

**Proposed minimum fix:** Remove metadata-only source-size/ordinal gates, preserving revision-fenced pages and actual seal-generation validation. Native provider context restoration stays with its owner.

#### L705 User queue text and whole pending-order representations impose independent capacity gates

**Current code:** `peritus-app-protocol/src/workbench/inputs.rs` limits text to MAX_WORKBENCH_INPUT_BYTES and represents complete reorder lists; intent adds independent text bounds.

**Proposed minimum fix:** Remove 8 KiB text/whole-order policy ceilings, using existing artifact text or incremental order operations for large data. Preserve input revisions, holds/dependency order and explicit execution.

#### L711 Conversation-library offset pagination has no revision snapshot fence or exact coverage validation

**Current code:** `peritus-app-protocol/src/workbench/library.rs::ConversationLibraryPage::new` checks limit and forward next_offset but no stable snapshot, exact coverage or uniqueness.

**Proposed minimum fix:** Fence pages to a stable snapshot/cursor and validate complete coverage, unique items and continuation. Keep bounded pages and explicit nonrunning fork authority.

#### L718 Whole launch histories, output and review diffs remain coupled to a single response frame

**Current code:** `peritus-app-protocol/src/wire/workbench_launch.rs` embeds all interactions/captures/feedback; wire/workbench_review.rs includes complete file diff with paged comments.

**Proposed minimum fix:** Page launch/output history and structured diff independently, checking count/payload availability before allocation. Preserve complete history, exact anchors and consent.

#### L819 — Browser improvement capacity claims and text counters disagree with daemon admission

**Current code:** `webui/src/lib/components/Improvements.svelte` claims full at 32 and uses maxlength=4096 UTF-16 units; overlay/title counters have similar mismatches.

**Proposed minimum fix:** Remove hard-coded capacity claims and derive actual daemon admission, validating UTF-8 byte lengths while preserving rejected drafts. Share L807's corrected composer/attachment contract.

#### L821 — Explorer paging lacks consistent request and directory identities

**Current code:** `webui/src/lib/components/{Explorer,FileNode}.svelte` appends offset pages without stable directory/generation identity or deduplication; filter covers loaded entries and recursion follows directory aliases.

**Proposed minimum fix:** Capture project generation/canonical directory/query/cursor with each request, rejecting stale replies and duplicate nodes/pages. Track canonical ancestors for cycle detection and query source-wide filtering; retain bounded pages.

### Application wire admission before allocation

Apply negotiated semantic and collection constraints once before owned allocation; preserve canonical framing and precise causes without a second full validation decode.

#### L700 Wire terminal values are copied before attachment-specific size checks and accept dimensions wider than native PTY support

**Current code:** `peritus-app-protocol/src/wire/terminal.rs` copies bytes before negotiated chunk checks; terminal dimensions fit unsigned wire values but not all native PTYs.

**Proposed minimum fix:** Check borrowed byte length before allocation and validate selected-backend dimensions before launch. Preserve structured recovery fields and useful bounded codec detail.

#### L701 Public application encoding allocates a full validation decode and collapses semantic error causes

**Current code:** `peritus-app-protocol/src/wire/mod.rs::encode_app_message` decodes the complete freshly encoded message; persistence value encoding also performs full decode comparison.

**Proposed minimum fix:** Validate domain fields against selected limits before encoding instead of allocating a second object graph solely for validation. Preserve semantic versus capacity causes and canonical byte identity.

#### L702 Direct 16-bit history counts bypass negotiated generic collection checks

**Current code:** `peritus-app-protocol/src/wire/doctor.rs::read_report` and wire/improvements.rs read raw u16 counts then allocate, bypassing generic collection admission.

**Proposed minimum fix:** Use shared checked counts with remaining-payload validation before allocation. Page large doctor/improvement history; retain positive flow windows and cursor checks.

### Complete guidance registry and retrievable request views

Keep full authoritative guidance and exact host CAS; select explicitly disclosed retrievable views for provider capacity.

#### L714 Complete guidance rendering fails above 64 KiB without a retrieval continuation

**Current code:** `peritus-app-protocol/src/workbench/memory/render.rs::render_guidance_for_request` fails once accumulated text exceeds MAX_WORKBENCH_GUIDANCE_RENDER_BYTES; individual guidance text is separately bounded.

**Proposed minimum fix:** Remove render/per-record policy cutoffs and choose an explicit retrievable view within actual request capacity. Preserve exact page coverage, scope, host CAS and forgotten-state dominance.

### Product review admission, identity and real acceptance requirements

Keep the existing incremental findings ledger. Remove arbitrary response quotas and distinguish defect identity from a policy that makes every heuristic a mandatory gate.

#### L129 — Product reviewer admission caps each submission while conserved findings can keep accumulating

**Current code:** `crates/orchestration/peritus-review/src/product/finding.rs::{ProductFinding,ProductReviewSubmission}` caps text and 128 findings; `finding_id` hashes category/title. `product/ledger.rs::admit_review` already conserves and updates historical findings incrementally.

**Proposed minimum fix:** Remove fixed submission/text quotas; do not add a replacement ledger or require resubmitting history. Distinguish same-title independent defects with a stable occurrence identity/anchor while retaining aliases for existing title-based IDs. Avoid an identity that changes every time lines move.

#### L130 — Product review derives blockers from a fixed category/severity policy

**Current code:** `crates/orchestration/peritus-review/src/product/finding.rs::blocks_when_non_advisory` and the blocking calculation make category or High severity sufficient; `product/ledger.rs` retains finding state.

**Proposed minimum fix:** Bind mandatory gates to explicit task/repository acceptance requirements and allow an explicit, recorded false-positive dismissal through the ledger. Keep unresolved actual defects actionable; advisory findings must remain advisory.

### Make native startup cancellable without a synthetic handshake deadline

Retain the owned startup operation and exact handshake state while waiting for helper readiness or cancellation.

#### L097 — Native helper admission is bounded, but synchronous startup can stall ownership

**Current code:** `crates/runtime/peritus-process/src/native.rs` and `native/protected_handle.rs` impose helper/payload/count caps. Contrary to the historical audit, `platform.rs::verify_helper_record` currently uses `recv_timeout(Duration::from_secs(5))` and terminates on expiry; `write_helper_manifest` still writes synchronously.

**Proposed minimum fix:** Remove the five-second helper cutoff and arbitrary payload/count maxima. Put handshake reads and writes under one owned cancellable startup path with cleanup, exact ready/activated records and no fabricated startup timeout.

#### L100 — Native observation histories have a hard 4,096-entry lifecycle gate

**Current code:** `crates/runtime/peritus-process/src/native/observation.rs::validate_observations` refuses histories above 4,096 and rescans their entire lifecycle.

**Proposed minimum fix:** Remove the lifetime observation ceiling and validate each appended observation incrementally, retaining contiguous sequence and exact plan/backend/lifecycle binding.

#### L098 — Platform-specific process observation/PTY restrictions can reject otherwise selected work

**Current code:** `crates/runtime/peritus-process/src/platform/resource/macos.rs::process_group_count` allocates 16,384 PIDs and `observe_group` fails at capacity. `supervisor/resource.rs` refuses unsupported required resource enforcement.

**Proposed minimum fix:** Grow/retry a full process-group sample instead of failing ownership. Enforce only selected metrics the backend actually supports, keeping real containment/required-enforcement guarantees. A one-result reap handoff needs no removal.

### One typed, uncapped provider retry contract

Carry acceptance certainty, idempotency/resume protection, request identity and provider delay through every retry layer. Backoff is pacing; arbitrary attempts, elapsed totals and transferred-byte exhaustion must not terminate recoverable work.

#### L205 — Some production provider routes retain a three-attempt retry ceiling and 64 MiB policy argument

**Current code:** `crates/app/peritus-daemon/src/config/provider/values.rs::retry_policy` selects three attempts, 100 ms base, two-second maximum/Retry-After, ten-second total elapsed and 64 MiB cumulative bytes for routes in `config/provider.rs`.

**Proposed minimum fix:** Replace these mandatory attempt/elapsed/transfer budgets with explicit optional caller policy and uncapped defaults. Use the shared retry legality checks and cancellable provider-directed delay; retain exact request/session identity.

#### L207 — Provider retry policy forbids more than 16 attempts and rejects long Retry-After observations

**Current code:** `crates/model/peritus-provider-core/src/retry.rs::{RetryPolicy::new,plan,RetryObservation::validate,formally_legal_retry}` requires attempt/elapsed/cumulative budgets, caps them at 16/seven days/1 GiB and rejects Retry-After beyond policy.

**Proposed minimum fix:** Make attempt, elapsed and cumulative-byte termination optional and absent by default. Remove compiled maxima and synthetic observation rejection. Honor valid Retry-After with cancellable waiting, keep capped exponential pacing and actual numeric representation, and preserve exact safe-fresh versus resume legality. Update the verified legality projection with the same semantics.

#### L226 — Anthropic HTTP Retry-After above two seconds becomes InvalidRetry instead of a deferred retry

**Current code:** `crates/model/peritus-provider-anthropic/src/client.rs` passes retry-after to the two-second-bounded policy, drains error bodies before preserving status facts, and parses only decimal seconds.

**Proposed minimum fix:** Apply L207's cancellable Retry-After handling instead of rejecting waits above two seconds. Parse valid HTTP delay forms, preserve status and acceptance facts if reading the error body fails, and keep ambiguous-submission safeguards.

#### L228 — Recovery layers use incompatible retry classifications and can discard provider delay/acceptance facts

**Current code:** `crates/model/peritus-provider-core/src/recovery.rs::from_model_failure` overrides by category/diagnostic; `crates/orchestration/peritus-agent/src/developer/retry.rs` treats CallerDecision/transport as safe fresh retry; `crates/app/peritus-product-runner/src/failover.rs::RoleRecovery` stops after three.

**Proposed minimum fix:** Pass typed acceptance certainty, retryability, exact resume/idempotency identity and Retry-After without category overrides. Remove the three-invocation cap and mandatory elapsed exhaustion in the developer planner (120 seconds on this baseline, L049/L050). Never convert CallerDecision or ambiguously accepted work into an automatic fresh submission.

#### L465 C5 exponential retry backoff can wrap to zero despite a positive minimum

**Current code:** `peritus-model-protocol/src/retry.rs:208` uses checked_shl, which checks shift width rather than lost high bits and can wrap a positive base to zero.

**Proposed minimum fix:** Use saturating exponential multiplication before applying a selected backoff cap. Keep positive delay and cancellation over arbitrarily many safe retries.

#### L470 Compatible API defaults retain finite retry and private production capacities

**Current code:** `peritus-provider-compatible/src/config.rs:189` selects three attempts, two-second delay limits and ten-second total recovery; transport/protocol capacities are private config fields.

**Proposed minimum fix:** Apply the shared uncapped retry/Retry-After policy and expose selected transport/protocol capacities without private production clamps. Preserve actual provider limits, cancellation and exact acceptance/resume facts.

#### L472 Compatible Retry-After parsing silently loses provider scheduling information

**Current code:** `client/metadata.rs:166` parses only integer seconds up to 86,400 and silently drops other valid scheduling values; client retry planning can replace the original HTTP failure.

**Proposed minimum fix:** Parse Retry-After seconds or HTTP dates, retain parse diagnostics explicitly and honor provider delay with cancellable waiting. Preserve normalized status/acceptance facts if scheduling cannot proceed; remove the two-second refusal.

#### L733 Hosted routes retain finite retry counts and model metadata only in process memory

**Current code:** `peritus-daemon/src/component/hosted.rs::build` constructs RetryPolicy with two retries and 64 MiB; HostedProvider retains catalog metadata in memory.

**Proposed minimum fix:** Apply shared untimed/nonexhausting retry policy and remove retry byte exhaustion. Retain or resumably refresh exact model capability/dialect metadata without changing explicit model choice or credential privacy.

### Stream framing, normalized events and ancillary observations

Process chunks incrementally, preserving sequence and terminal facts without making fragmentation or diagnostic volume a malformed model response.

#### L217 — HTTP stream framing enforces nonwidenable 8 MiB frame and 16 MiB pending-buffer limits

**Current code:** `crates/model/peritus-provider-core/src/framing.rs::FramingLimits::new` cannot widen 8/16 MiB; `framing/{ndjson,sse}.rs::push` checks combined input before draining complete frames.

**Proposed minimum fix:** Remove nonwidenable frame ceilings and consume incoming chunks incrementally before checking pending bytes. Retain actual caller resource bounds, valid UTF-8 and parser lifecycle checks; avoid repeatedly copying the remaining buffer.

#### L221 — Native responses are fully buffered, and Codex creates one event per 16 argument bytes

**Current code:** `crates/model/peritus-provider-openai/src/runtime/stream.rs` fragments tool arguments into 16-byte events; both native stream.rs implementations eagerly build a VecDeque and ignore cancellation in next.

**Proposed minimum fix:** Replace Codex's 16-byte argument fragmentation with the existing negotiated event chunk size. Produce events incrementally and honor cancellation while draining them; this needs no new response store or session layer.

#### L222 — Syntax healing has a 64 KiB repair ceiling and a 65,536-fragment argument ceiling

**Current code:** `crates/model/peritus-provider-core/src/healing.rs::parse` caps malformed input at 64 KiB and puts original+repair into one bounded audit event; `healing/buffer.rs::ToolArgumentBuffer::append` limits fragment count to 65,536.

**Proposed minimum fix:** Remove the independent fragment-count ceiling and arbitrary repair-byte cutoff. Count repair provenance separately or reference its original bytes through existing storage; retain unambiguous, reversible syntax repair and reject fabricated values.

#### L223 — Response reduction makes event and ancillary observation limits terminal malformed failures

**Current code:** `crates/model/peritus-model-protocol/src/reducer/stream.rs::push` limits cumulative events; `reducer/events.rs` caps rate/cache/extensions; `reducer/terminal.rs::reject` converts these to terminal malformed failures.

**Proposed minimum fix:** Remove cumulative event and ancillary-observation quotas that make otherwise valid responses malformed. Keep actual provider representation limits and exact sequence, identity and terminal checks; store ancillary observations through existing records instead of invalidating model output.

### Migration admission, contention and retry identity

Reuse the existing migration catalog, backup and transactional recovery. Remove synthetic reserve/lock/lexical blockers while keeping actual capacity, atomicity and registry integrity.

#### L232 — A 64 MiB migration reserve gates startup even when the schema needs no migration

**Current code:** `crates/app/peritus-daemon/src/startup/migration.rs::migrate_existing` always preflights with 64 MiB reserve; `crates/state/peritus-migrations/src/engine.rs::preflight` takes min of database/backup free space; `plan.rs::build` charges reserve even with no steps.

**Proposed minimum fix:** Skip migration-capacity preflight when no migration steps are required. Charge each filesystem only for the backup or scratch bytes actually placed there, without a fixed reserve for zero work; retain real disk and integrity checks.

#### L233 — Migration ownership has immediate contention rejection, and SQL validation uses broad lexical restrictions

**Current code:** `crates/state/peritus-migrations/src/engine.rs::open` try-locks a backup-directory owner file and selects a five-second SQLite busy timeout; `config.rs` caps release text; `registry/validation.rs::reject_transaction_control` searches lexical tokens.

**Proposed minimum fix:** Queue typed migration-owner/SQLite contention with cancellation and scope ownership to the actual database. Remove descriptor SQL/label/release policy caps coherently with stored schema; parse forbidden SQL statement kinds outside comments/string literals. Keep transaction/schema compatibility and native numeric constraints.

#### L234 — A rolled-back migration can require operator restore, after which daemon retry reuses a forbidden identity

**Current code:** `crates/state/peritus-migrations/src/engine.rs::{apply_with_hooks,validate_plan}` checks plan before Applied replay and marks BeforeCommit failures RestoreRequired when backup-required; `engine/restart.rs::reconcile` does likewise. Daemon migration identity is derived only from registry digest.

**Proposed minimum fix:** Treat known nonacceptance and confirmed rollback as retryable without backup restoration. Give a restored attempt a new attempt identity under the existing migration lineage, and check Applied idempotency before old-plan admission; keep exact backup and registry digests.

### Make kernel replay resumable without changing its canonical history

Reuse the journal's existing state/checkpoint storage for a validated kernel checkpoint and suffix replay. Preserve exact event and successor verification.

#### L271 — Kernel recovery replays all retained history and hashes the complete growing aggregate at every step

**Current code:** `crates/state/peritus-journal/src/domain/kernel.rs` sets 64 input references; kernel/capsule.rs repeats it on decode. kernel/recovery.rs replays from genesis; state_digest.rs rebuilds and hashes the complete growing state every step.

**Proposed minimum fix:** Remove the arbitrary reference-count rejection while bounding allocations by actual encoded input. Add a kernel-state checkpoint bound to the exact journal head in existing StateInstall storage; this recovery adapter has no such usable checkpoint today. Replay only the suffix after verification. Reuse unchanged state components and stream canonical hashing without silently changing historical digest semantics; preserve resolver cause instead of replacing it with static text.

### Compute context closure and certificates without redundant full scans

Optimize the existing deterministic relation, preserving independent integrity verification.

#### L351 C6 synchronous selection repeats full closure computation and full-plan validation

**Current code:** `selection/closure.rs:38` always runs graph-count passes; optional selection recomputes closures and insertion-sorts. Constructor/certificate/acceptance revalidate complete immutable plans.

**Proposed minimum fix:** Index node IDs, use a work queue or cached closures that stop at a fixed point, and efficient deterministic sorting. Reuse checked immutable inputs and incremental certificate evidence instead of replaying identical full work at each boundary. Structural termination witnesses are retained rather than treated as quotas.

#### L706 Compaction proposals require strictly smaller byte handles and one whole preview

**Current code:** `peritus-app-protocol/src/workbench/compaction.rs::WorkbenchCompactionPreview::new` caps the whole entry list at u16 and requires strictly smaller replacements.

**Proposed minimum fix:** Remove whole-preview/focus policy caps and stream the existing complete confirmation fingerprint. Keep exact source identity/confirmation and strictly smaller replacements; compaction does not adopt native context.

### Correct E1 identity rules and construction/restore mismatches

Preserve stable identities and authority boundaries; change only incidental layout and arbitrary textual restrictions, with versioned historical digest support.

#### L356 E1 harness textual identities and paths have fixed portable-shape and byte limits

**Current code:** `domain/identity.rs:7` caps ASCII component IDs; `domain/value.rs:158` applies universal Windows filename/length restrictions and separate provenance limits.

**Proposed minimum fix:** Remove arbitrary identity/provenance lengths and apply host-platform path rules using a lossless native representation. Keep canonical identity validity, source containment and protected control paths; retain explicit portability reporting where cross-platform export requires it.

#### L359 E1 protected harness assets freeze configuration and manifest position across successors

**Current code:** `domain/graph_validation.rs:292` includes absolute declaration position in protected inventory equality despite stable ComponentId.

**Proposed minimum fix:** Remove manifest position from new protected fingerprints and compare stable component identity and semantic declaration. Version historical interpretation; actual protected-policy changes still require explicit authorization.

#### L366 E1 public rollback reason bypasses validation and can produce an unreadable durable plan

**Current code:** `materialization/plan_model.rs` exposes raw rollback String fields, planner accepts them and the decoder imposes stricter reason rules.

**Proposed minimum fix:** Remove the arbitrary reason length cap and validate nonempty/control-character rules through one checked text type at every public planner/decoder boundary. Preserve old plan identity decoding; never publish a plan that its decoder rejects.

### Keep evolution campaigns and rollback eligibility continuous

Admit explicitly superseding work within existing lifecycle and preserve immutable activation evidence.

#### L443 F0 silently evicts rollback targets from the long-lived production pointer

**Current code:** `pointer/reducer.rs:313` removes the oldest activation from state history at capacity; rollback admission searches only that retained suffix.

**Proposed minimum fix:** Remove eligibility dependence on the hot suffix: load authorized targets from existing immutable activation journal history. Keep a bounded display/cache if useful, preserving exact target, predecessor and authorization checks.

#### L444 F0 phase progression prevents admitting later diagnosis, variants or replacement evaluations

**Current code:** `campaign/reducer/transition.rs:70` phase guards exclude later diagnosis/variant admission once evaluation advances; collection insertion rejects replacement evaluations.

**Proposed minimum fix:** Add explicit later diagnosis/variant and superseding-evaluation transitions retaining prior history. Track readiness per variant and explicitly exclude unevaluable variants rather than deadlocking the campaign; promotion requirements remain enforced.

#### L445 F0 delta admission cannot represent several legitimate evolvable harness changes

**Current code:** `change/delta.rs:49` requires both declarations and a changed content digest; `change/manifest.rs:86` requires baseline and candidate declarations for every delta.

**Proposed minimum fix:** Add add/remove/update variants to the existing delta representation, comparing full dependency/executable/content semantics. Retain protected-component authorization and explicit migration evidence; harmless metadata-only evolution must not require fabricated content changes.

#### L446 F0 rejects multiline human-authored explanation text

**Current code:** `change/text.rs:16` rejects every control character, including newline/tab, and applies a compiled byte quota.

**Proposed minimum fix:** Permit newline and tab in human explanation text, remove the arbitrary byte limit and retain nonempty meaning plus unsafe terminal-control rejection.

#### L447 F0 accepts mandatory failure-class predictions that its attribution engine can never confirm

**Current code:** `change/prediction.rs:151` admits FailureClass subjects; `attribution/engine.rs:68` always returns UnsupportedFailureClass for them.

**Proposed minimum fix:** Reject unsupported mandatory predictions before freezing the campaign with a recoverable capability error. Add measurement only when that requirement is actually needed; never accept an obligation the current evaluator cannot satisfy.

#### L448 F0 promotion always requires resource measurements, even with unconstrained thresholds and unrelated objectives

**Current code:** `selection/engine.rs:39` reads every resource metric and requires availability even under unconstrained thresholds; promotion policy does not express optional measurement requirements.

**Proposed minimum fix:** Represent measurement requirements explicitly and omit requirements for metrics neither constrained nor ranked. Check required availability at campaign admission and preserve honest unknown values; keep real promotion criteria.

#### L451 F0 recovery can report terminal completion without checking retained artifact/evidence observations

**Current code:** `runtime/recovery.rs:48` checks artifact/evidence only when a publication directive exists, then declares terminal campaign complete from phase.

**Proposed minimum fix:** Verify artifact and evidence owner receipts required by the durable activation even when no delivery row remains. Treat missing delivery as insufficient proof; reconcile existing identity rather than blindly republishing.

### Keep optional provider telemetry from blocking known output or status

Retain honest unknown usage and diagnostic evidence. Optional enrichment must not own inference completion.

#### L466 Provider-core metadata and credential bounds are compiled independently

**Current code:** `peritus-provider-core/src/endpoint.rs:27` and credential constructors impose independent length limits; endpoint normalization can expand the serialized value after admission.

**Proposed minimum fix:** Remove arbitrary credential/endpoint length ceilings where transport permits the normalized value and validate normalized representation consistently. Keep secret isolation, URL authority, header/control rules and bounded display with retained full causes.

#### L467 Optional hosted metadata enrichment blocks return of an already fetched model catalog

**Current code:** `hosted/catalog.rs:31` waits up to ten seconds for optional OpenCode metadata after fetching the authoritative model catalog.

**Proposed minimum fix:** Return the authoritative catalog promptly and run optional enrichment independently under existing cancellation, updating metadata later. Preserve credential-origin separation and unknown fields; remove this incidental wait from catalog ownership.

#### L473 Optional compatible observations can fail an already accepted response

**Current code:** `peritus-provider-compatible/src/client.rs:247` makes optional success-header parsing fatal; `client/metadata.rs:175` errors on malformed mapped integers/text.

**Proposed minimum fix:** Retain malformed optional metadata as diagnostics and mark usage unknown while returning valid output. Preserve typed accepted/submitted facts for genuinely fatal errors and never fabricate accounting measurements.

### Process provider accumulation incrementally

Use existing fragment streams and retain exact complete values without repeated whole-history serialization.

#### L478 Hosted reasoning accumulation repeatedly scans and serializes all retained reasoning

**Current code:** `hosted_reasoning.rs:28` linearly finds prior reasoning details and serializes the entire accumulated map on every fragment to measure size.

**Proposed minimum fix:** Index details by stable identity and track encoded size incrementally. Stream final fragments with cancellation, preserving signature/content boundaries and exact final representation.

#### L484 Anthropic thinking signatures are limited to one delta and tool argument progress is buffered until block closure

**Current code:** `peritus-provider-anthropic/src/stream/content.rs:108` uses a boolean to reject a second signature delta and buffers tool arguments as Heartbeat until block close.

**Proposed minimum fix:** Accumulate signature fragments through explicit block close and emit one valid replay value. Expose argument accumulation through existing progress/fragments while execution remains gated on complete validated arguments.

#### L490 Google thought replay can concatenate complete JSON objects and thought summaries silently drop later array entries

**Current code:** `peritus-provider-google/src/stream/interactions.rs:221` and generate.rs emit complete replay JSON objects per signature delta; `interaction_fields.rs:8` returns only the first summary-array text.

**Proposed minimum fix:** Build one canonical replay object per reasoning item with retained signature boundaries, and include every supported summary member. Do not concatenate complete JSON objects into invalid replay content.

#### L491 Google structured output and tool arguments share fixed buffering and fragment-count limits, delaying semantic progress until closure

**Current code:** `peritus-provider-core/src/healing/buffer.rs:28` caps fragments at 65,536 and rejects empty fragments; Google helpers hardcode production limits and buffer structured/tool content to closure.

**Proposed minimum fix:** Thread selected shared capacities through helpers, remove fragment-count quotas and ignore empty deltas safely. Expose accumulation progress and emit completion incrementally with cancellation, retaining complete argument validation before execution.

### Use one Unicode-safe diagnostic shortening rule

Preserve safe previews and original causes; truncating bytes inside a UTF-8 code point can panic.

#### L505 MCP error shortening can panic at a Unicode boundary

**Current code:** `peritus-mcp/src/error.rs:137` calls detail.truncate(1024) without checking a character boundary.

**Proposed minimum fix:** Truncate at the last valid UTF-8 boundary through the shared small rendering helper, including bridge/JSON-RPC paths, while retaining original causes and active-request cleanup.

#### L508 Plugin SDK identity and diagnostic restrictions include another Unicode panic path

**Current code:** `peritus-plugin-sdk/src/error.rs:34` truncates at byte 512; `manifest/wire.rs:94` reconstructs entrypoint fields without complete checked manifest validation.

**Proposed minimum fix:** Use the Unicode-safe helper and run complete manifest validation after deserialization. Remove arbitrary argument/capability count quotas where transport permits, retaining canonical identity, signatures and confinement.

#### L513 — Plugin diagnostic truncation can panic the host

**Current code:** `peritus-plugin-host/src/error.rs:82` truncates arbitrary diagnostic String at byte 1,024.

**Proposed minimum fix:** Use the same Unicode-safe shortening helper and preserve the existing original error chain; no diagnostic storage subsystem is required.

### Quality policy, discovery and working-directory binding

Make execution timing optional consistently and preserve explicit discovery inputs. Run exactly the selected check in its declared authorized directory.

#### L647 Quality defaults now omit execution deadlines but retain finite output and protocol limits

**Current code:** Contrary to this historical heading, `peritus-tools-quality/src/catalog.rs` still sets ToolLimits timeout=600000; `plan.rs::compile` and `dispatcher/run.rs::validate` require Some(definition.timeout_millis()).

**Proposed minimum fix:** Remove mandatory quality deadlines through the shared optional-time contract and update definition/plan/call validation together. Remove independent progress/artifact ceilings; keep explicit user constraints and complete output artifacts.

#### L648 Quality discovery can reject the whole catalog and consume its retry inputs

**Current code:** `peritus-tools-quality/src/discovery.rs::canonical_catalog` rejects any duplicate; dispatcher/discover.rs takes explicit inputs before inspect and result construction; source readers have fixed limits.

**Proposed minimum fix:** Retain explicit inputs until a result is recoverable, isolate malformed/colliding sources and page encoded catalog output. Remove source/line/recipe cutoffs and discover configured checks rather than impose all-features commands.

#### L653 Quality working-directory declarations are not enforced consistently

**Current code:** `peritus-tools-quality/src/acceptance.rs` hashes declared working_directory, but plan compile uses its supplied directory without resolving that declaration consistently.

**Proposed minimum fix:** Resolve/bind the declared directory within the validated snapshot and require that exact directory across constructors and dispatch. Accept authorized subdirectories rather than force the snapshot root.

### Provider onboarding and retained route intent

Run discovery/status/install work cancellably outside foreground startup. Preserve chosen routes and credentials across transient health failures, with truthful diagnostics.

#### L660 Account status can block foreground setup and misclassify infrastructure failure as sign-out

**Current code:** `peritus-provider-onboarding/src/account.rs` runs native status/login synchronously; parse_status maps every nonzero exit to SignedOut.

**Proposed minimum fix:** Stream status asynchronously with cancellation. Distinguish explicit sign-out from infrastructure/malformed output and preserve safe diagnostic causes; generic nonzero exit must not trigger login.

#### L661 Provider installation has no execution/download timer here; a size gate and nonpersistent effect handoff remain

**Current code:** Contrary to the historical heading, `peritus-provider-onboarding/src/install.rs` sets curl connect=15s, total=120s and max-size=1 MiB; run_command kills at 15 minutes.

**Proposed minimum fix:** Remove these synthetic download/execution deadlines and arbitrary script-size gate, retaining cancellable owned installer execution and approved artifact identity. Preflight nonsecret profile fields before credentials publish and retain cleanup failures.

#### L662 Model onboarding blocks its caller while discovery owns an unreachable cancellation token

**Current code:** `peritus-provider-onboarding/src/models.rs` creates a private cancellation token then joins a scoped thread/runtime around discovery.

**Proposed minimum fix:** Expose the discovery future and caller token directly, retaining manual model selection and typed catalog failures. Cancellation must be reachable by the caller.

#### L669 Provider setup can disable durable routes on transient status failure and block even offline startup

**Current code:** `peritus-launcher/src/provider_setup.rs::repair_if_needed` removes retained providers after failed/unwanted login, then persists the reduced selection; startup probes broadly.

**Proposed minimum fix:** Keep durable route intent when health is unknown, probe only needed routes off offline startup and retain old credentials while active generations reference them. Publish setup/cancellation through existing operation receipts.

#### L671 Hidden credential input has a smaller cap and an incomplete terminal-mode acquisition rollback

**Current code:** `peritus-launcher/src/provider_setup/direct.rs::RawInputGuard::enter` enables raw mode before fallible paste/flush setup and constructs the guard only at the end; input has a separate cap.

**Proposed minimum fix:** Construct the guard immediately after raw acquisition and preserve restoration errors on all later failures. Align credential input with corrected shared store admission.

#### L740 Model discovery freshness can gate start and uses an independent cancellation token

**Current code:** `peritus-daemon/src/product_run/catalog.rs::query_models` creates a private cancellation token and wraps discovery in a 30-second timeout; selection collapses provider causes.

**Proposed minimum fix:** Use caller cancellation, remove the synthetic catalog deadline and let previously verified selections proceed while optional refresh runs where capability validity permits. Preserve explicit choice and typed adapter cause.

## 3. Retained history, inventory, and work-count limits

### Journal storage admission and native database capacity

Remove application-imposed byte quotas from local durable rows, keeping SQLite's real limits and explicit user-selected storage quotas. The checkpoint and conversation bundles must not retain smaller duplicate guards.

#### L024 — SQLite native length and configurable database page ceilings

**Current code:** `crates/state/peritus-journal/src/sqlite/connection.rs` — `configure` lowers `SQLITE_LIMIT_LENGTH` to 32 MiB; `limit_storage_pages` is an explicit opt-in method, not a default invocation here.

**Proposed minimum fix:** Remove the 32 MiB `set_limit` call. No default page quota is installed in this file, so do not delete the explicit quota API on this evidence. Keep attached databases disabled, WAL/FULL durability, store identity, and native SQLite length errors.

### Control-state storage and repeated full replay

Keep the existing C0 journal, immutable operations, state rows, and receipts. Remove local JSON/state admission cutoffs together, and reuse a verified conversation replay by committed head instead of re-reading every archive on each query. A second durable conversation store or replacement event system is unnecessary.

#### L011 — Durable control and request-context size admission

**Current code:** `crates/app/peritus-product-runner/src/control.rs` — `MAX_CONTROL_BYTES`, `MAX_REQUEST_CONTEXT_BYTES`; `control/record/codec.rs` — `encode`, `decode`; `crates/app/peritus-daemon/src/product_control/storage.rs` — `accept_installs`; `crates/state/peritus-journal/src/record/state.rs` — `StateInstall::new`.

**Proposed minimum fix:** Remove the 16 MiB local control encode/decode and state-row guards, coordinated with L024's SQLite limit removal and the storage-frame fixes. Keep the current JSON representation and already separate body/reply rows initially; replacing it with a new small-root persistence architecture is not required to remove this cutoff. Model context remains selected against the actual provider, rather than requiring all retained history in one request.

#### L021 — Every ordinary control read re-verifies the entire growing archive

**Current code:** `crates/app/peritus-daemon/src/product_control/storage/replay.rs` — `load`, `load_replay`, `load_at_revision`, `verify_request_archive`, `load_checkpoint`; `storage.rs` — `accept_installs`; `storage/checkpoints.rs` — `verify_checkpoint_bodies`.

**Proposed minimum fix:** Cache the verified `ConversationReplay` with its committed aggregate head inside the existing store owner. Advance and verify only the newly accepted operation/archive, invalidating on an uncertain commit or changed head. Full replay remains the cold-open/integrity fallback; historical reads need not re-hash unrelated later file bodies.

### Remove the in-place scope ledger lifetime quota

Keep exact scope identity while allowing its history to grow and reconciling only a provably incomplete tail.

#### L046 — In-place scope journal has a lifetime byte ceiling and strict torn-record barrier

**Current code:** `crates/app/peritus-product-runner/src/workspace_delivery/scope.rs::{enroll,journal_size,load}` imposes a 64-MiB total and a 512-KiB record limit. Every scope query reparses the ledger; an incomplete last line rejects loading.

**Proposed minimum fix:** Remove the lifetime-byte ceiling, use a compatible record-size policy, and recover only an incomplete trailing record under exclusive ownership. Keep rejection of conflicting complete records and use the inspection continuation fix for previews.

### Remove causal-parent policy limits while keeping journal query pages

Separate whole-event admission from bounded query windows and preserve canonical causal identity.

#### L018 — Journal causal-parent, frame, and global-query bounds

**Current code:** `crates/state/peritus-journal/src/record.rs::EventDraft::new` rejects more than 4,096 parents and validates ordering/uniqueness. `GlobalEventWindow` is explicitly a query window with retention-gap reporting; `ExactFrame` uses the production codec.

**Proposed minimum fix:** Remove the parent-count policy check with compatible encoder/decoder admission; retain sorted unique parents. Keep the query page size and retention-gap checks. Apply the shared codec repair to frame admission.

### Keep full public history behind bounded live views

Separate bounded presentation from durable retention. A preview cap must not erase the source observation.

#### L025 — Public activity windows and text truncation

**Current code:** `crates/app/peritus-daemon/src/product_run/interaction.rs::{append,stream_text}` evicts activity beyond 256 entries, truncates fields and drains whitespace prefixes above 256 bytes. `crates/app/peritus-app-protocol/src/product/interaction.rs` independently enforces the 256-entry/8,192-byte live projection.

**Proposed minimum fix:** Retain full observations in durable history before projecting bounded live activities, and expose older/full data by paging or exact references. Preserve leading whitespace in the retained source; keep the live window bounded. Do not assume this in-memory window itself provides a full-history endpoint.

#### L122 — Review diff omits large untracked files and truncates the candidate at one MiB

**Current code:** `crates/app/peritus-product-runner/src/bundle.rs::diff` cuts all candidate views at one MiB. `append_untracked_files` and nested equivalent omit files above the fixed per-file limit and silently skip unreadable/non-UTF-8 paths.

**Proposed minimum fix:** Keep a bounded review preview with exact continuation/artifact references, include large untracked material through ranges, and report omissions. Run Git observations through the cancellable owned command path; a prefix must not certify complete review.

### Remove memory lineage and snapshot policy ceilings

Remove fixed lifetime admission checks across the existing reducers, codecs and host projections. Update the corresponding specifications/proofs to describe the widened contract. Keep exact source sequences, lineage binding and graph validity; finite model input capacity remains a view constraint.

#### L058 — Local working-memory defaults and configuration admission

**Current code:** `crates/app/peritus-product-runner/src/context_config.rs::{validate,working_limits}` clamps configurable preferences and constructs hard-coded 65,535-observation/512-entry/32-link limits.

**Proposed minimum fix:** Remove unrelated maximum clamps and fixed lifetime limits. Keep meaningful percentages, nonzero physical reply sizes and configurable prompt preferences.

#### L061 — Working-memory lineage has hard observation and entry-count ceilings

**Current code:** `crates/orchestration/peritus-context/src/working/limits.rs::{standard,new}` hard-caps observations, retained entries, entry bytes, links and operations, including stale/superseded entries.

**Proposed minimum fix:** Remove these hard constructor envelopes and lifetime observation/entry admission; apply real provider-view capacity at rendering. Do not add a new retention subsystem merely to remove a count check.

#### L071 — Working-memory admission counts history that its update API cannot retire

**Current code:** `crates/orchestration/peritus-context/src/working/state.rs::ingest_working_observation` rejects at the observation limit; `working/delta_reducer.rs::upsert_entry` rejects at retained-entry capacity and supersession leaves the prior entry stored.

**Proposed minimum fix:** Remove count-driven rejection, including charging inactive history as exhaustion. Preserve supersession/dependency/source identity. Removing the ceiling is sufficient for this blocker; arbitrary deletion of predecessor entries could break references.

#### L073 — Working-state wire format has a separate eight-MiB envelope

**Current code:** `crates/orchestration/peritus-context/src/working/wire/mod.rs::LIMITS` independently fixes the snapshot envelope at eight MiB and bounds fields/collections.

**Proposed minimum fix:** Align encode/decode with the widened memory contract and remove the independent fixed envelope. Keep version and graph validation; splitting historical snapshots is a separate scaling repair, not required just to delete this admission cutoff.

#### L074 — Working-event replay has a finite suffix size

**Current code:** `crates/orchestration/peritus-context/src/working/event.rs::replay_working_events` rejects a suffix above the observation limit before reducing any event.

**Proposed minimum fix:** Remove the suffix-count refusal and its matching proof precondition, processing through the existing reducer. Preserve first-invalid-event behavior.

#### L064 — Transcript projection, pending operations, and observed-file counts

**Current code:** `crates/app/peritus-product-runner/src/local_context/memory/ingestion.rs::persist_transcript` caps message IDs against protocol limits and pending/files against memory entries.

**Proposed minimum fix:** Remove cumulative projection-count admission separately from model-view limits, preserving exact proposal/tool ordering. Retire only settled current pending state where its existing transition proves settlement.

### Fit derived memory to the actual provider window

Compact derived evidence and use exact archive references before view assembly fails; never silently discard literal instructions or unresolved effect identity.

#### L072 — Required working-memory closure can block prompt assembly

**Current code:** `crates/orchestration/peritus-context/src/working/selection.rs::render_working_state_with_headroom` can enlarge preferred allocation for required closure but errors above actual headroom.

**Proposed minimum fix:** Use concise source references/derived compaction for required evidence before refusing assembly, preserving source access and unresolved identity. Keep the real provider window; distinguish it from graph or binding errors.

#### L075 — Prompt reconstruction has hard failure paths and narrow evidence heuristics

**Current code:** `crates/app/peritus-product-runner/src/local_context/assembly.rs` fails when pinned/required state does not fit; `assembly/working.rs::append_state` collapses renderer errors to a capacity message. `assembly/exchanges.rs::omission_notice` lists only eight result handles, and `assembly/evidence.rs` selects eight prior errors/eight assistant observations.

**Proposed minimum fix:** Preserve the actual renderer error and perform the shared derived-memory reduction before provider-capacity failure. Keep original requirements and exact pending effects; retrieval-specific fixed cutoffs are addressed with paged evidence. Replace fixed discovery cutoffs with a paged archive continuation and read preview ranges without materializing whole artifacts.

### Page exact context inspection and index source validation

Preserve exact checkpoint checks while removing unnecessary duplication and repeated lookup work.

#### L081 — Exact context inspection has a 32-MiB output stop and growing validation cost

**Current code:** `crates/app/peritus-product-runner/src/local_context/storage/inspection.rs::inspect` combines complete source index, readable messages and hex archive, then rejects above 32 MiB. `checkpoint_validation.rs::validate_index` calls the linear `WorkingState::observation` in `crates/orchestration/peritus-context/src/working/state_access.rs` once per source.

**Proposed minimum fix:** Page inspection and make hex export optional. Resolve contiguous source IDs by checked index, or build one lookup for the validation pass, retaining exact locator comparisons and lineage/digest checks.

### Remove command replay and process-record lifetime quotas

Keep permanent one-use and terminal evidence in the existing records. Remove count-driven refusal/deletion rather than discard old identities to admit new work.

#### L082 — Command router eventually exhausts its retained replay ledger

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/construction.rs` selects 64 active/4,096 replay entries. `crates/tools/peritus-tool-router/src/replay.rs::reserve` rejects a full retained replay map.

**Proposed minimum fix:** Remove the lifetime replay-count check and preserve replay identity/result records. Treat active concurrency separately as cancellable backpressure. Apply the shared absent-artifact-quota policy to the command store's one-GiB quota.

#### L088 — Durable process registry can block launch at 16,384 unsettled records

**Current code:** `crates/runtime/peritus-process/src/consumption.rs::{open,consume}` retires settled records under fixed-count pressure and refuses admission when enough records remain unresolved.

**Proposed minimum fix:** Remove the retained-record count ceiling and automatic count-driven retirement. Keep unresolved ownership explicit without making an unrelated old record a capacity barrier for new identities.

#### L093 — Process retention removes settled results and one-use records under count pressure

**Current code:** `crates/runtime/peritus-process/src/registry_storage.rs::retire_execution_record` deletes claim, spool and manifest. `recovery/manifest.rs::ownership_settled` does not require artifact publication.

**Proposed minimum fix:** Stop count-driven deletion through L088. Any explicitly requested future cleanup must preserve one-use/result evidence and wait for required output publication; do not delete unpublished retry inputs.

### Reconnect command ownership across daemon restart

Retained JSON projections cannot recreate an OS process-control owner. Persistent commands need a supervisor/control endpoint that outlives the daemon, bound to the existing process identity and records; this is a real missing capability, not merely a timeout constant.

#### L086 — Reconnect command projections are evicted and active control cannot survive restart

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/projections.rs::{load,record}` evicts at 4,096 entries/64 MiB, ignores long handles and converts every running record to indeterminate on restart.

**Proposed minimum fix:** Remove projection lifetime eviction and rebuild projections from authoritative records when possible. Reconnect active commands through a durable owner endpoint; the current in-process runtime has no such reconnect path. Keep unknown identities indeterminate rather than report completion.

#### L094 — Restart reconciliation terminates exact live trees instead of reconnecting control

**Current code:** `crates/runtime/peritus-process/src/recovery/reconcile.rs::reconcile` terminates ExactLive trees. `platform/inheritance.rs::configure_parent_death` installs Linux parent-death SIGKILL; `platform/watchdog.rs::reap` imposes a finite cleanup window.

**Proposed minimum fix:** Introduce the narrow persistent supervisor/reattach contract using existing identities, then replace restart-time termination and parent-death killing for persistent work. Preserve cleanup of actually abandoned work and exact PID/birth/containment checks. Removing kill hooks alone would leave uncontrolled processes.

#### L099 — Crash-watchdog identity reobservation has a 500-ms cutoff

**Current code:** `crates/app/peritus-daemon/src/cli/process_watchdog.rs::terminate` retries unverifiable identity for 500 ms, then fails; EOF triggers group termination.

**Proposed minimum fix:** Remove the fixed reobservation deadline and keep exact reconciliation cancellable. Align owner-loss behavior with the durable-owner repair; preserve mismatched/unverifiable identity protection.

#### L101 — Native recovery cannot settle certain live or unobservable process identities

**Current code:** `crates/runtime/peritus-process/src/native/probe/windows.rs::terminate` cannot act on ExactLive because no durable Job handle exists. `recovery/reconcile.rs` propagates one probe/termination failure out of the entire scan.

**Proposed minimum fix:** Record recovery outcomes per command so unrelated commands still reconcile. Use the durable-owner/Job identity for reattachment or exact cleanup, distinguishing exited/zombie roots from a live tree without accepting a reused PID.

### Separate review-domain limits and recovery

These findings describe the generic `ReviewRunState` engine. The inspected product review caller (`crates/app/peritus-product-runner/src/turn/reviewer.rs`) uses `ProductFindingLedger`; do not introduce the generic engine into that path as part of removing blockers. Apply the library repairs below where this engine is used, without creating a new production integration.

#### L131 — The separate D2 review-domain engine compiles fixed history and wire ceilings

**Current code:** `crates/orchestration/peritus-review/src/limits.rs` defines fixed cycle, assignment, findings, provenance, payload and state bounds; constructors can only narrow them.

**Proposed minimum fix:** Remove synthetic lifetime/cycle policy checks for an actual consumer and make optional resource policy explicit. Align admission, wire and replay. Keep numeric representation, identity, confidence and ordering validation. No independent production integration is proposed.

#### L132 — D2 oscillation policy can stop on unchanged severity after two candidate bindings

**Current code:** `crates/orchestration/peritus-review/src/oscillation.rs` treats equal maximum severity, repeated sets and cycle exhaustion as oscillation triggers.

**Proposed minimum fix:** Remove automatic termination for equal maximum severity and lifetime cycle counts; retain distinct current findings and allow paused work to resume. Heuristic oscillation should provide evidence, not permanently forbid progress.

#### L133 — Review command-history exhaustion also rejects cancellation and recovery controls

**Current code:** `crates/orchestration/peritus-review/src/reducer.rs::validate_fences` rejects at 65,535 used commands before dispatch; `replay.rs::ReviewReplay::rebuild` requires a checkpoint exactly matching genesis replay.

**Proposed minimum fix:** Remove retained-command exhaustion, including for cancellation/recovery. Rebuild a missing/stale derived checkpoint from a valid event chain, or replay its verified suffix; keep genuine event/identity corruption failures.

#### L134 — D2 persists and replays complete duplicated finding history inside one bounded state

**Current code:** `crates/orchestration/peritus-review/src/state.rs::{estimated_encoded_bytes,validate_inert}` counts complete retained finding/cycle history against state limits; `wire/state.rs` serializes it.

**Proposed minimum fix:** Remove aggregate policy ceilings coherently with wire admission. Reuse stable finding identities to avoid duplicate bodies while keeping durable event history and existing integrity checks; no new persistence subsystem is needed.

#### L135 — D2 quorum requires fresh context and evaluates all retained current reviews together

**Current code:** `crates/orchestration/peritus-review/src/quorum.rs` evaluates retained current submissions and requires fresh context for all of them.

**Proposed minimum fix:** Require fresh context/independence only when the acceptance contract requests it. Select a sufficient valid independent subset and permit replacement of invalid submissions. Preserve writer/user context.

#### L136 — D2 errors truncate diagnostics and do not distinguish exhausted history from correctable input

**Current code:** `crates/orchestration/peritus-review/src/error.rs::{ReviewError::new,reject,truncate_utf8}` silently clips detail to 4,096 bytes and maps several replay failures to Quarantine.

**Proposed minimum fix:** Remove exhausted-history errors with the underlying cap; distinguish reloadable cache drift from corrupt events. Preserve the full diagnostic cause and mark any display truncation; retain evidence/authority failures.

#### L137 — Review reconciliation rejects summed provenance before deduplication

**Current code:** `crates/orchestration/peritus-review/src/reconciliation.rs` sums provenance/evidence/dispositions before `mutation::merge_sources_and_evidence` deduplicates them.

**Proposed minimum fix:** Merge unique source/evidence identities before admission and remove arbitrary retained disposition/waiver quotas. Preserve checked arithmetic, provenance and the existing history.

#### L138 — Review bindings can be encoded with more producers than restart decoding accepts

**Current code:** `crates/orchestration/peritus-review/src/wire/mod.rs::{write_binding,read_binding}` writes producer lists using codec collection admission but reads them with the 1,024-source bound.

**Proposed minimum fix:** Unify constructor, encode and decode producer admission and remove the narrower restart-only quota. Preserve schema/tag validation, producer identities and the five-variant oscillation vocabulary.

### Trace recovery must accept what trace append can retain

Use the existing append format and exclusive torn-tail repair. Ordinary history growth must not invalidate a session.

#### L152 — Trace append has no size limit but local-context recovery refuses traces above 1 GiB

**Current code:** `crates/app/peritus-product-runner/src/trace/local_memory.rs::observations` rejects traces above 1 GiB and scoped frames above 64 MiB, though `trace.rs` appends without those lifetime limits.

**Proposed minimum fix:** Delete the reader-only 1 GiB trace and 64 MiB frame policy cutoffs, matching writer admission and streaming records. Reuse the existing exclusive torn-tail repair and retain sequence, scope and corruption checks.

#### L153 — Trace accounting collapses every accounting error to a generic loop limit

**Current code:** `crates/app/peritus-product-runner/src/trace/accounting.rs::AccountingTrace::account` converts every accounting error to `DeveloperLoopError::LimitExceeded`.

**Proposed minimum fix:** Preserve the original typed accounting error through the trace adapter instead of converting every failure to LimitExceeded. Use the deadline correction in L117; this adapter needs no new quota or recovery subsystem.

### Guidance storage and selected request views

Retain forgotten identity tombstones and exact provenance. Remove lifetime catalog/render ceilings without turning an entire catalog into a mandatory prompt.

#### L195 — Guidance identities accumulate in a single 16 MiB catalog, including forgotten records

**Current code:** `crates/app/peritus-daemon/src/product_control/guidance.rs::{GuidanceCatalog::advance,plan}` installs a complete growing catalog using journal StateInstall.

**Proposed minimum fix:** Remove the fixed catalog state ceiling using the same journal admission correction as L011. Preserve revision and forgotten-identity tombstones; avoid rewriting unrelated entries where the existing store supports individual state updates.

#### L196 — Guidance paging follows full replay, and all eligible guidance must fit a 64 KiB request block

**Current code:** `crates/app/peritus-daemon/src/product_control/storage/guidance.rs::{guidance_page,guidance_rows,guidance_for_request}` loads all rows before paging; `crates/app/peritus-app-protocol/src/workbench/memory/render.rs::render_guidance_for_request` rejects a full eligible block over 64 KiB.

**Proposed minimum fix:** Select/paginate identities before full row loads, using scope metadata to preserve visible pagination, and expose damaged rows independently. Remove fixed text/render ceilings; select relevant guidance against actual provider capacity with retrievable exact references. Keep mandatory current instructions present and preserve tombstone/provenance checks.

### Recover outstanding prompts without filling the broker with settled history

Use the durable prompt target as authority and the broker as a live working set. Explicit signature validity remains an authorization condition.

#### L235 — Prompt capacity includes old terminal responses, while approval time and epoch checks need explicit reissue recovery

**Current code:** `crates/app/peritus-daemon/src/prompt/types.rs`, `prompt/registry.rs` (`prepare_registration`, `retire_terminal`, `correlations_for`) count terminal entries against the 256 default/4096 maximum. `authority/owner/prompt/restoration.rs` restores those entries before settling them. `crates/state/peritus-approval/src/authentication.rs` validates signed validity windows and epochs.

**Proposed minimum fix:** Retire settled broker entries after durable settlement, answer already settled targets directly from their retained receipt, and page prompt enumeration. Remove the compiled broker maximum; keep live-set backpressure recoverable. Explicitly reissue expired or obsolete approval challenges rather than bypassing signatures or changing the conversation identity.

### Scale immutable state history without fixed logical-value ceilings

Retain the existing content-addressed tree and hash validation; extend its representation only as required and avoid repeated verification of unchanged data.

#### L256 — Lossless history is durable, but installation and replay repeatedly traverse complete state/history

**Current code:** `crates/state/peritus-journal/src/sqlite/history.rs` recursively visits every node on install and restores full values. history/node.rs sets MAX_LEVEL=4 and checks MAX_STATE_BYTES; schema.rs repeats both bounds. query/aggregate.rs and aggregate/checkpoint.rs still load complete chains even when returning a checkpoint.

**Proposed minimum fix:** Remove the 16 MiB policy bound across state admission, node construction and SQL. Generalize checked tree height/capacity beyond level four so removing one limit does not hit the next; retain old node encoding/digests where unchanged. Cache verified immutable subtrees under exact store/mutation identity and extend validated replay checkpoints by suffix, rather than treating the existing full-chain checkpoint query as incremental recovery.

#### L267 — Full journal integrity forbids prefix retention and revalidates every historical state value

**Current code:** `crates/state/peritus-journal/src/integrity/scan.rs` loads positions 1..last and requires count==last; validation.rs reconstructs every historical state and resolves commands again. integrity/artifacts.rs checks foreign references before owner filtering; hash_chain.rs expects aggregate count to fit u32.

**Proposed minimum fix:** Stream the existing complete integrity contract and reuse verified immutable nodes/checkpoint suffixes under exact store identity. Restrict journal artifact validation to journal-owned references and report other-owner damage separately. Replace the u32 expect with a typed representational error. Keep no-prefix-deletion integrity; no retention/garbage-collection protocol is needed for this repair.

### Keep sessions persistent and page outstanding work

Session lifetime follows explicit lifecycle intent. Transport replacement recovers pending commands/prompts under the same actor/session.

#### L258 — Durable sessions have no age limit, but closing is irreversible and command recovery has no continuation cursor

**Current code:** `crates/state/peritus-journal/src/application/session_store.rs` has no TTL; advance_application_session reads state then UPDATEs only by session_id. command_store.rs::unsettled_application_commands returns the first 1..4096 records without a cursor.

**Proposed minimum fix:** Keep persistent sessions and explicit monotonic closure. Add observed-state CAS to lifecycle UPDATE and check affected rows. Add command_id continuation to unsettled recovery so later commands remain enumerable even when earlier ones need repair; retain exact receipts and no automatic age settlement.

#### L259 — Prompt targets persist without a timer, but discovery/republication is absent from the durable adapter

**Current code:** `crates/state/peritus-journal/src/application/prompt_store.rs` provides register/look-up/settle only by PromptId, with no outstanding actor/session page. Targets persist in app_prompt_targets; the TUI clears prompt presentation on recovery.

**Proposed minimum fix:** Add paged actor/session outstanding-target enumeration to this existing table and publish current targets during reconnect/startup. Renew obsolete authority explicitly and preserve exact settlement replay. Share broker retirement/answer admission with L235/L244; add no prompt expiry.

### Recover artifact transfer phases under the existing artifact identity

Use the existing catalog, publication receipt and content store; retain ownership through fallible completion. No additional transfer ledger is required for exact full reupload.

#### L262 — Application artifact catalog has no failed-upload transition or recovery enumeration

**Current code:** `crates/state/peritus-journal/src/application/artifact_store.rs` has begin/get/complete but no failed/abandoned transition or outstanding page. Its completion returns conflict when no row exists. `crates/app/peritus-daemon/src/artifact/service.rs` actually uses this catalog for uploads.

**Proposed minimum fix:** Add outstanding-upload paging and an explicit failed/abandoned recovery transition to app_artifacts, preserving identity/digest binding and exact Available replay. Distinguish unknown completion as NotFound from conflicting completion. Use those states to explain/recover interrupted uploads rather than retaining unexplained Uploading forever.

#### L279 — Artifact transfers share finite registries and first-sixteen download polling can starve later transfers

**Current code:** `crates/app/peritus-daemon/src/artifact/client.rs::pump` always takes the first sixteen sorted downloads; service.rs has a separate finite transfer registry.

**Proposed minimum fix:** Add a rotating transfer-ID cursor to the existing client registry so each active download receives service. Reap terminal transfers and report registry saturation as backpressure; preserve existing chunk bounds and one active upload owner per artifact.

#### L280 — Partial artifact transfer progress is ephemeral even though ownership and publication identity are durable

**Current code:** `crates/app/peritus-daemon/src/artifact/service.rs::complete_upload` removes the upload before metadata validation/publication/finalization/catalog completion; upload_chunk advances protocol state before writer success. service/scoped.rs materializes full objects.

**Proposed minimum fix:** Validate completion while retaining the owned transfer. Retain explicit completion phase/result facts so errors after publication or consuming the writer reconcile the existing receipt and finalized digest instead of losing progress. Do not advance accepted offsets ahead of successfully stored bytes. Reuse exact full reupload after restart; add byte resume only for a verified retained temporary file/offset. Add ranged reads where a small selection otherwise requires a full object.

#### L281 — Private artifact claims are permanent exact bindings without adoption, rebinding, or retirement

**Current code:** `crates/app/peritus-daemon/src/artifact/scope.rs` stores a revision-one 80-byte exact actor/conversation/workspace/metadata claim and rejects implicit adoption of unscoped content.

**Proposed minimum fix:** Keep exact private ownership binding and exact-scope reupload. Reuse explicit failed/abandoned upload recovery from L262/L280. No automatic rebinding, expiry, claim retirement or legacy migration is needed to remove a daily-work limiter; authorized import remains a separate explicit action.

### Remove scheduler lifetime quotas across reduction and reopening

Retain immutable history and durable identity. Remove matching policy checks in constructors, reducers, checkpoint decoding and proofs together; share larger state/codec capacity with L009/L256 instead of merely moving the ceiling.

#### L283 — Scheduler capacity and lifetime retention have immutable finite ceilings with no unlimited setting

**Current code:** `crates/orchestration/peritus-scheduler/src/limits.rs::new` requires eleven positive values under compiled maxima; binding.rs binds them into identity and durability/semantics.rs forbids schema drift.

**Proposed minimum fix:** Remove retained-work/history/attempt policy admission and compiled operational maxima. Keep actual selected resource/concurrency backpressure and bounded poll batches. Preserve old serialized bindings for replay; do not mutate their digest in place. Where widening types or changing accepted transitions needs a new schema, provide an explicit same-run transition/compatible decoder rather than resetting the run.

#### L286 — Full scheduler command history blocks cancellation, completion and finalization after 65,535 events

**Current code:** `crates/orchestration/peritus-scheduler/src/reducer/fences.rs::classify` and spec_admission reject at 65535 used commands before kind dispatch; reducer.rs maps it to LimitExceeded.

**Proposed minimum fix:** Delete the lifetime command-count guard in runtime and corresponding specifications/validation. Remove the checkpoint codec's matching policy ceiling through the shared codec repair. Preserve all identities, ordinals, exact replay and terminal authority; new cancellation/completion must remain admissible.

#### L287 — Completed work and dispatch identities consume permanent scheduler retention, while temporary capacity is treated as corrected input

**Current code:** `crates/orchestration/peritus-scheduler/src/reducer/apply/admission.rs` counts all retained work; apply/dispatch.rs rejects 65535 historical dispatches as well as real active reservation pressure.

**Proposed minimum fix:** Remove historical work/dispatch caps through admission and wire/state.rs decoding, retaining the records. Report queue/reservation saturation as retryable backpressure instead of CorrectInput/generic InvalidCommandFrame. Keep dependencies, parent identity and resource feasibility intact.

#### L288 — Every work item has an immutable attempt ceiling and recovery policy that can make exhaustion terminal

**Current code:** `crates/orchestration/peritus-scheduler/src/work.rs` requires an AttemptNumber under immutable limits. reducer/apply/dispatch.rs rejects attempts at u16::MAX or the item maximum; apply.rs maps RetryWork exhaustion to illegal.

**Proposed minimum fix:** Make retryable work's attempt policy optional with no default exhaustion and use a wider checked diagnostic attempt count across records/wire/proofs. Continue the same lineage only after accepted-effect/ownership reconciliation proves retry safe. Preserve explicit ambiguity and genuine terminal failure evidence; changing the number alone must not duplicate effects.

#### L293 — Checkpoint invariants make lifetime history retention part of the schema, so deleting old entries is not a valid repair

**Current code:** `crates/orchestration/peritus-scheduler/src/state/validation.rs` repeats retained-vector/estimate guards and equates enqueue/dispatch/sequence ordinals with complete history counts; wire/state.rs also bounds decoded collections.

**Proposed minimum fix:** Remove policy count/estimate checks consistently while keeping full records and ordinal/digest equalities. Do not prune history or add an archive schema. Validate dependency cycles once per graph change and retain all identity/ownership/dependency invariants on reopen.

### Remove E0 cumulative quotas across counters and codecs

Retain the existing aggregate/history. Widen diagnostic counters and representations compatibly where necessary; preserve selected authority and acceptance requirements.

#### L307 — E0 mandates immutable positive lifetime cycle and retention budgets

**Current code:** `crates/orchestration/peritus-orchestrator/src/limits.rs` requires twelve finite dimensions. state/counters.rs compares cumulative u16 counters against them; role_cycle.rs and acceptance.rs consume those counters.

**Proposed minimum fix:** Remove synthetic cycle/revision/handoff/observation exhaustion, including effective acceptance cycle quotas. Widen checked logical counters and associated wire fields under compatible schema handling so u16 does not become the replacement work ceiling. Keep run identity, acceptance evidence and actual active-resource policy. This separate aggregate is not proof of a live DeveloperLoop cause.

#### L308 — E0 cleanup consumes the same exhaustible budgets as normal work

**Current code:** `crates/orchestration/peritus-orchestrator/src/reducer/apply/directives.rs` charges cancel/pause/resume publication; state/validation.rs and reducer.rs also limit retained commands to u16.

**Proposed minimum fix:** Remove lifetime directive/settlement/command-count gates in reducers, validation and codecs, sharing L307/L009. Keep cancellation and truthful child settlement recordable under operational backpressure; retain full command identities instead of reserving a small cleanup allowance.

#### L312 — Unrelated E0 resources reuse artifact and directive caps, and full history has a byte ceiling

**Current code:** `crates/orchestration/peritus-orchestrator/src/candidate.rs` uses artifact_references for producers/ancestries; handoff.rs uses it for findings/evidence; ownership.rs uses child_directives for reviewers. durability.rs still requires whole state within 16 MiB production framing.

**Proposed minimum fix:** Remove unrelated quota reuse and lifetime collection/byte policy admission through constructor/decoder/writer paths. Retain provenance and exact findings. Widen candidate digest count encoding compatibly when its real u16 representation is exceeded; share larger state storage with L256 rather than silently modifying historical digests.

#### L315 — E0 can commit a valid large D2 terminal observation that its own decoder rejects

**Current code:** `crates/orchestration/peritus-orchestrator/src/canonical/wire/observation/quality.rs::read_review` and read_review_fixer impose 1024 findings while writers use the shared collection limit; reducer/state validation does not impose that decoder-only bound.

**Proposed minimum fix:** Remove decoder-only artifact quotas on finding/fixer sets and share one admissibility contract with the producer/encoder. Retain every finding and conservation binding. Validate the final persisted successor against those same decode invariants before append.

### Remove D0 cumulative turn quotas and align field admission

Keep selected simultaneous concurrency, checked accounting and exact provider/tool evidence; lifetime totals must not terminate authorized work.

#### L316 — D0 requires seven immutable positive limits for a turn's accumulated work

**Current code:** `crates/orchestration/peritus-agent/src/limits.rs` imposes seven positive compiled maxima and checks cumulative transitions/context/provider/tools/output/results. reducer.rs charges duplicates and retained totals.

**Proposed minimum fix:** Remove cumulative work/output/context/transition quota checks and arbitrary compiled operational maxima. Keep explicit active concurrency and diagnostic totals, widening u16/u32 lifetime fields compatibly where required. Duplicate observations must not charge new logical work; share codec/state capacity repairs.

#### L317 — D0's shared transition allowance can prevent cancellation and terminal observation

**Current code:** `crates/orchestration/peritus-agent/src/reducer.rs` increments the shared transition quota before control/settlement, except an irreversible Exhausted escape.

**Proposed minimum fix:** Remove that lifetime transition admission in runtime and replay/spec paths. Keep cancellation and terminal truth recordable without invoking Exhausted as quota escape; preserve unresolved external effects until reconciled.

#### L324 — D0 retained text and evidence fields have independent fixed admission ceilings

**Current code:** `crates/orchestration/peritus-agent/src/identity.rs::SafeText` caps 16384 bytes; completion.rs caps evidence/uncertainties at 1024/64, tools.rs at 256 evidence IDs; command.rs caps provider capsules at 8 MiB.

**Proposed minimum fix:** Remove independent text/evidence/uncertainty/capsule policy ceilings consistently in constructors and canonical decoders. Use existing artifact/source references for large bodies and shared bounded transfer representation. Retain nonempty semantic text, exact ordering/revision/digest and native representability checks.

### Remove D1 lifetime quotas across planning, execution and decoding

Treat counts as accounting and preserve exact attempt identities; coordinate B2/C4 contract changes and versioned D1 representations instead of deleting only the admission check.

#### L332 — D1 immutable gate planning and attempt ceilings terminate retryable work

**Current code:** `peritus-gates/src/descriptor.rs:16` caps gates/dependencies/evidence at 1,024 and gate-count times mandatory u16 attempts at codec capacity; `reducer/apply.rs:90,177` terminates retries at that maximum. Descriptor timeout matching uses the current mandatory B2/C4 timeout contract.

**Proposed minimum fix:** Remove independent gate and attempt ceilings; make requested stopping budgets optional through B2/C4/D1 together. Widen attempt ordinals compatibly, remove matching reader guards, and preserve dependency, revision and recovery-before-retry checks. Couple timeout removal to the shared execution-policy repair.

#### L333 — D1 per-result metadata and diagnostic limits reject or truncate at the domain boundary

**Current code:** `outcome.rs:130,176` rejects more than 16 result artifacts; decoded metadata separately rejects media types over 255 and labels over 256. Error details have a separate bounded display representation.

**Proposed minimum fix:** Remove artifact-count and label policy ceilings in both conversion and decoding. Preserve valid identity/media syntax and uniqueness. Record terminal facts independently of display formatting, retaining the complete diagnostic behind its existing evidence reference.

#### L393 Acceptance completion policy requires finite u16 attempt and review-cycle ceilings

**Current code:** `peritus-spec/src/policy.rs:8` requires finite positive u16 gate-attempt/review-cycle maxima; contracts and reviews bind those limits.

**Proposed minimum fix:** Make unrequested stopping budgets optional across B2/D1/quality-policy callers and compatible codecs. Keep wider checked attempt/cycle identities and actual independent-review quorum; explicit policy amendments must retain valid historical contract identity.

#### L395 B2 acceptance refuses successful observations beyond the finite completion counters

**Current code:** `peritus-quality-policy/src/evaluator.rs:130` passes the contract maxima to gate/review evaluation; `gates.rs:173` and `reviews.rs:192` reject otherwise successful late evidence.

**Proposed minimum fix:** Apply optional stopping policy consistently to evaluation, allowing successful evidence beyond an unselected budget. Preserve gate predicates, freshness, independent review and chronological identity; update ordinal representations with the shared widening repair.

#### L401 B0 counts all review requests against an immutable run-lifetime review allowance

**Current code:** `peritus-kernel/src/reducer/review.rs:35` counts every retained request for the run, including invalidated reviews, against immutable max_review_cycles.

**Proposed minimum fix:** Remove unrequested lifetime review-count rejection through the shared optional completion policy. Preserve all review history and actual acceptance requirements; selected limits must remain explicitly amendable.

### Keep D1 recovery, evidence and command receipts independent of whole-state size

Persist the plan and exact receipts, reuse verified checkpoints and retain evidence references. Preserve irreversible dispatch fencing and historical hashes.

#### L334 — D1 full checkpoints impose a cumulative evidence-size ceiling below advertised per-gate maxima

**Current code:** `durability.rs:159` encodes complete gate state under production codec limits before commit; `wire/state.rs:51` clones retained results, evidence and identity lists into every checkpoint; `canonical.rs:15` serializes them again.

**Proposed minimum fix:** Store immutable evidence/results once by verified reference and checkpoint the active state and indexes. Coordinate shared codec capacity changes for legitimate large records; preserve old state digest interpretation and record post-effect outcomes even if a view cannot be rendered.

#### L335 — D1 progress and recovery synchronously copy complete state and replay complete history

**Current code:** `reducer.rs:58` clones/hashes complete state per command and replays from genesis; `engine/publication.rs:57` reads all aggregate records to find one authoritative result.

**Proposed minimum fix:** Load a verified checkpoint and replay its suffix, index result-event lookup, and avoid repeated full-state copies/hashes through an equivalent versioned checkpoint representation. Keep independent evidence validation and exact event identity.

#### L337 — D1 exact retry requires current successor state and restart requires an external exact plan

**Current code:** `engine.rs:216` requires an external GatePlan to resume; durable state holds its digest. `durability.rs:240` resolves old commits against the latest checkpoint, and `durability/lifecycle.rs:65` permits only last-event lifecycle retries.

**Proposed minimum fix:** Persist the original versioned plan with the existing run record or verified artifact and load it on resume. Resolve historical commands from their exact immutable receipts even after head advances; resolved dispatch never grants a second live effect permit.

### Remove memory history quotas and use its existing indexes

Preserve immutable memory/provenance, separate the active query view from retained history and the model token window, and avoid synchronous full-history work on routine requests.

#### L342 C6 memory has fixed per-record and whole-replay ceilings

**Current code:** `peritus-memory/src/claim.rs:11` caps payload/tokens/features; `index/rebuild.rs:19` rejects raw revisions and tombstones above 4,096 before selecting latest identities. Retrieval also bounds complete input and output counts.

**Proposed minimum fix:** Remove independent content, evidence, feedback and history policy quotas consistently across constructors/readers. Fold latest identities incrementally, and page global retrieval with deterministic ranking; retain token-based model selection without expiring retained memories.

#### L344 C6 index retrieval still scans and copies the whole view synchronously

**Current code:** `index/rebuild.rs:126` cross-scans tombstones and revisions; `retrieval/plan.rs:181` insertion-sorts complete records. Existing posting lists are built but the retrieval planner operates on the full record view.

**Proposed minimum fix:** Use keyed revision/tombstone lookups and existing postings, efficient deterministic ordering and streamed canonical hashing. Keep full validation available for recovery, with bulk work outside the control executor and equivalent ranking/deletion semantics.

### Separate user-selected spending constraints from B1 accounting

Remove mandatory finite work ceilings through the command vocabulary and formal transition relations; preserve descriptive actual usage and exactly-once charging.

#### L345 B1 requires immutable finite budgets and cannot replenish consumed capacity

**Current code:** `peritus-budget/src/limits.rs:8` defines five finite dimensions; `command/vocabulary.rs:18` has Begin, reconciliation, Seal and Close but no amendment/reopen. Attempts/retries are charged as consumed capacity.

**Proposed minimum fix:** Represent unselected ceilings as unlimited, retaining checked usage counters. Add authorized budget amendments and resumable spending-stop state under the existing account identity, preserving ancestor balances and exact receipts; do not reset consumption to make space.

#### L346 B1 one reservation overrun permanently faults the entire ancestor lineage

**Current code:** `transition/accounting/fault.rs:78` recursively faults every ancestor. `transition/reconciliation/observation.rs` treats operation-reservation overrun as that permanent fault even when root headroom remains.

**Proposed minimum fix:** Record a scoped reservation incident and truthful raw usage without permanently disabling siblings. Permit verified final/corrected reconciliation and authorized reservation/budget amendment with explicit accounting adjustments; selected spending constraints remain enforced and recoverable.

#### L347 B1 equal usage with new evidence can reject the final report

**Current code:** `transition/reconciliation/observation.rs:135` rejects unchanged cumulative usage with different evidence before considering Final; `reachability/guards/outcomes.rs:36` and `rejections/reservation.rs:65` specify the same rule.

**Proposed minimum fix:** Allow a correlated final event to advance finality with unchanged amounts and new final evidence, charging zero extra usage and retaining both evidence bindings. Update runtime and formal guards/rejections together; ordinary observations still cannot decrease cumulative usage.

### Remove approval record quotas and expose complete decision details

Keep exact signed scope, one-use binding, cryptographic widths and safe rendering. A short preview is not the whole authority record.

#### L380 B1 approval requests and credential snapshots have frozen collection and byte ceilings

**Current code:** `peritus-approval/src/request.rs:16` caps permissions/participants and preimage bytes; credential/digest/codec constructors impose matching policy capacities.

**Proposed minimum fix:** Remove arbitrary permission, participant, credential and preimage quotas across construction and codecs. Preserve real enum cardinalities, canonical ordering and fixed cryptographic lengths; never omit signed scope to fit.

#### L385 B1 approval rendering omits most maximum-size permission and provenance sets

**Current code:** `render/collections.rs:20` attempts only capped participants/permissions and flags omissions through the builder.

**Proposed minimum fix:** Retain the safe short preview and omission counts, adding paged/full-detail retrieval under the same request digest. Ensure all permissions and provenance are accessible before approval, without widening the preview buffer into an unbounded terminal dump.

### Retire superseded grants explicitly while retaining authority history

Do not weaken deliberately cumulative restrictions or treat an expired restriction as permission.

#### L388 Every overlapping ceiling grant constrains the entire request, including expired grants

**Current code:** `peritus-policy/src/evaluation_constraints.rs:173` intersects every matching ceiling grant; matching does not distinguish a retired historical alternative from a current grant.

**Proposed minimum fix:** Use the existing authority definition/amendment path to explicitly supersede old grants, retaining their history outside the active set. Report the exact blocking grant; cumulative restrictions remain intersected unless the authorized policy explicitly changes them.

#### L389 Policy evaluation repeatedly scans canonical sets and recurses over unbounded input collections

**Current code:** `evaluation_predicates.rs:11` recursively searches grants and permissions; selector/operation/risk helpers repeat canonical membership scans.

**Proposed minimum fix:** Use iterative indexed or sorted-merge membership/subset evaluation and reuse risk/operation lookups. Preserve exact authority matching and formal results without introducing new collection quotas.

#### L391 Primitive capability names have a compiled 128-byte ASCII grammar boundary

**Current code:** `peritus-types/src/capability.rs:109` validates canonical segmented capability identity, with fixed ASCII grammar and a 128-byte boundary.

**Proposed minimum fix:** No daily-driving failure is established for this authority-name grammar. Keep it unchanged; only introduce an explicit external-name mapping if an actual integration requires it, without changing authority identity matching.

### Remove E3 synthetic workload, retry and deadline ceilings

Preserve requested campaign sample counts, real model capacity, isolation and concurrency; workload scheduling must not impose a hidden lifetime stop.

#### L429 E3 campaigns require six positive finite resource ceilings and a finite retry policy

**Current code:** `peritus-eval/src/limits.rs:5` enforces six positive compiled maxima; `profile/policy.rs:23` requires finite attempts and checks them against the chosen rollout limit.

**Proposed minimum fix:** Remove compiled task/rollout/retry and independent report-size policy maxima across constructors/readers. Execute the requested workload through existing scheduling with selected concurrency, widening checked counts where necessary. Preserve requested statistical replicates/confidence as method inputs.

#### L430 E3 execution requires a wall deadline and has no untimed representation

**Current code:** `profile/binding.rs:98` requires positive u64 deadline_micros and hashes it; execution directives propagate that mandatory value.

**Proposed minimum fix:** Make deadline optional throughout binding, directive, execution port and versioned canonical representation. Propagate None to candidate/evaluator execution; preserve cancellation, elapsed usage and isolation.

#### L432 E3 accepts a frozen profile whose component policies disagree with its final limits and provider

**Current code:** `FrozenEvaluationProfile::new` at `profile/binding.rs:250` checks selected rollouts but does not revalidate dataset/retry/metric policies against final limits and provider.

**Proposed minimum fix:** Revalidate all components against the final admitted profile and exact provider binding, with one consistent storage admission rule. Do not add another independent quota to conceal policy disagreement.

### Remove F0 workload/history quotas and align durable representation

Reuse existing campaign, pointer and evidence records; preserve mathematical ranges, fixed schemas and promotion authority.

#### L442 F0 has nine mandatory positive compiled bounds frozen into campaigns and pointers

**Current code:** `peritus-evolution/src/limits.rs:7` freezes nine positive compiled limits; `transition/collection.rs:18` checks capacity before duplicate identity.

**Proposed minimum fix:** Remove compiled population/attribution/text/activation-history quotas and permit explicit workload policy amendments. Resolve exact duplicates before capacity handling, preserving campaign identity and completed evidence.

#### L452 F0's accepted bounds disagree with its attribution and assessment persistence schema

**Current code:** `wire/semantic/attribution.rs:22` applies production codec collection capacity below advertised attribution size; `selection.rs:42` requires exactly 14 criteria and also a configurable maximum.

**Proposed minimum fix:** Remove the narrower attribution policy ceiling with the shared codec repair. Remove configurable count policy for the fixed fourteen-criterion schema and apply identical final-assessment rules before commit and at restore.

#### L454 F0 wraps complete growing checkpoints in a fixed eight-MiB opaque payload

**Current code:** `wire/campaign.rs:318` wraps complete state in one opaque byte field; inner semantic encoders independently use production limits, narrowing growing checkpoints.

**Proposed minimum fix:** Propagate one selected representation policy through inner/outer codecs and remove the independent eight-MiB cutoff. Reuse existing verified evidence references where bodies duplicate; preserve controls and historical digest decoding without a new checkpoint system.

### Keep telemetry buffering observational and retry exact export batches

Reuse queue/batch state; telemetry delivery must not acquire authority over task completion.

#### L514 Telemetry queues impose finite item counts and bounded shutdown flushing

**Current code:** `peritus-telemetry/src/buffer/mod.rs:17` caps capacity at one million; `export/pump.rs:119` already returns Pending after selected flush batches.

**Proposed minimum fix:** Remove the compiled queue maximum while retaining selected loss/backpressure policy. Preserve truthful Pending shutdown and provide exporter cancellation; no durable spill system is needed unless delivery is required.

#### L515 Failed telemetry export batches are rebuilt rather than pinned for retry

**Current code:** `export/pump.rs:83` recreates a batch from current queue for each retry; `buffer/state.rs:105` can evict its records after a failed/lost acknowledgement.

**Proposed minimum fix:** Pin the exact in-flight batch and identity until acknowledgement resolves, excluding its records from overflow eviction. Use existing queue/batch ownership rather than rebuilding a different batch.

#### L732 Telemetry export has fixed replay and spool-entry gates on its recovery path

**Current code:** `peritus-daemon/src/telemetry/runtime.rs::open` recovers all traces and flushes pending export synchronously; local_file.rs scans bounded directory entries/quarantines temporaries.

**Proposed minimum fix:** Remove spool entry quotas, reconcile exact incomplete files and replay incrementally. Mark real telemetry gaps and keep optional telemetry failure off primary startup/progress.

### Remove unrequested proxy lifetime quotas and deadlines

Keep selected active concurrency, destination authority and cancellation; cumulative usage is accounting unless explicitly budgeted.

#### L534 Managed networking requires finite connection and owner lifetimes

**Current code:** `peritus-network/src/plan.rs:34` requires finite connection/owner millis and cumulative bytes/connections; `proxy/owner.rs:92` exits on total duration and connect.rs clamps sockets to 30 seconds.

**Proposed minimum fix:** Make durations and unrequested cumulative quotas optional through plan/canonical/accounting/socket setup. Preserve selected concurrency, cancellation and explicit credential validity; untimed mode must remove the socket fallback too.

#### L535 Proxy acceptance quota counts rejected connections and never refills

**Current code:** `proxy/owner.rs:103` increments accepted before rejecting busy sockets, permanently exhausting maximum_connections; observations have a separate fixed capacity.

**Proposed minimum fix:** Remove lifetime connection exhaustion and use active admitted workers for backpressure. Busy rejection must not consume permanent allowance; retain descriptive counts and a rolling diagnostic view with omissions disclosed.

#### L537 Connection duration accounting uses different origins for different phases

**Current code:** `proxy/worker.rs:41` starts elapsed accounting after headers, while tunnel/relay paths use other origins.

**Proposed minimum fix:** Remove implicit duration enforcement through the shared network policy. When a deadline is explicitly selected, carry one origin through headers/DNS/connect/both relays; cancellation remains independent of elapsed accounting.

#### L540 HTTP header effects occur before network byte ceilings are charged

**Current code:** `proxy/worker.rs:50,91` sends CONNECT/HTTP headers before charging downloaded/uploaded bytes, and shared/local charges can diverge.

**Proposed minimum fix:** For selected byte budgets, reserve local/shared amounts atomically before sending and reconcile actual charge after outcome. Otherwise keep descriptive counters without lifetime enforcement; retain uncertainty once a remote request may be accepted.

### Workspace action admission and atomic markers

Preflight effect-free failures before one-use authority consumption. Publish exact action/namespace markers atomically, retain idempotency and reconcile partial writes under the same identity.

#### L579 Workspace action consumption has a hard 1,024-action per-revision ceiling

**Current code:** `peritus-workspace/src/consumption.rs::{commit,restore}` enforces MAX_ACTIONS_PER_REVISION=1024 on both paths.

**Proposed minimum fix:** Remove both lifetime count gates, retaining action digest uniqueness and checked count arithmetic. Do not turn restart decoding into a second hidden ceiling.

#### L580 A failed workspace action-marker write can poison later opens

**Current code:** `peritus-workspace/src/consumption.rs::commit` writes directly into a create_new final marker; restore treats every entry as a complete marker.

**Proposed minimum fix:** Write/sync a temporary marker then atomically publish without replacing a different action. Distinguish/recover this writer's incomplete temporary files and validate fixed marker length before reading; preserve unrelated files.

#### L581 Workspace authority is consumed before several effect-free failures

**Current code:** `peritus-workspace/src/gateway.rs::authorize_in_condition` commits consumption before `mutation.rs` plans the patch and `rollback.rs` checks revision and lineage.

**Proposed minimum fix:** Move deterministic patch/lineage/counter preflight before consumption. Once effects may have started, recover the same consumed intent instead of granting a fresh action.

#### L585 Workspace namespace binding writes have no interrupted-write repair path

**Current code:** `peritus-workspace/src/transaction_namespace.rs::establish_binding_bytes` writes create_new final binding directly; subsequent opens reject a partial binding.

**Proposed minimum fix:** Atomically publish the binding after syncing a temporary file; reconcile incomplete publication in the exact namespace using expected identity. Retain overlap/symlink protection and never silently choose another directory.

### Web operation settlement, observation and stop

Retain original effect identity through transport recovery and settle parent/child facts together. Observation stays read-only; explicit controls remain independently reachable.

#### L805 — Web operation capacity can be consumed forever by already reconciled transport children

**Current code:** `peritus-web/src/state.rs` admits at most 4096 operations and prunes completed dedup facts; operations.rs settles recovered parents while daemon receipt children can remain unresolved.

**Proposed minimum fix:** Atomically settle the native transport child when parent recovery proves the result. Remove lifetime record admission cutoff/sole dedup eviction, page completed history and route file saves through the same insertion path.

#### L806 — Web stop shares the send wait, while supposedly observational recovery can start execution

**Current code:** `peritus-web/src/api.rs::action` holds mutation_guard across dispatch/wait; GET operation calls operations::observe, whose send recovery calls daemon/chat.rs::drive and can create/queue/start work.

**Proposed minimum fix:** Release conversation lock during transport wait while retaining fenced transition ownership so stop reaches the operation. Make GET receipt inspection read-only and route explicit retry/resume through guarded original-command recovery.

### Browser persistent intent, console control and draft history

Persist exact pending payloads and editing state using existing browser storage, scoped to workspace/session. Control requests must remain independent of stalled input.

#### L815 — Browser recovery promises do not match recovery effects or persistence

**Current code:** `webui/src/lib/api.ts` retains only operation metadata; operations.svelte.ts uses sessionStorage and suppresses storage errors; file drafts are memory-only; recovery GET can drive effects through L806.

**Proposed minimum fix:** Apply L806's read-only observation and truthful explicit retry/acknowledgement labels. Save exact payloads/file drafts in persistent browser state by workspace/session and surface storage read/write failures.

#### L816 — Browser console interruption shares an unresolved input queue

**Current code:** `webui/src/lib/components/Console.svelte::input` serializes all keys including Ctrl+C on one Promise chain; output offset after=0 resets on mount.

**Proposed minimum fix:** Send interrupt on a separate control request, retain unacknowledged input identity and restore output at acknowledged spool offset from L808. Keep scrollback/poll cadence as display choices.

#### L818 — Alias depth and editor undo impose additional synthetic boundaries

**Current code:** `webui/src/lib/workspace.svelte.ts::dispatch` rejects depth>8; TextEditor.svelte retains undo only locally and evicts after 200 changes/2M UTF-16 units.

**Proposed minimum fix:** Detect alias cycles using visited identities instead of depth cutoff. Save undo with the draft across remounts, remove synthetic change/unit quotas and always retain an accepted edit's inverse.

#### L823 — Settings save can overwrite edits made while the request is unresolved

**Current code:** `webui/src/lib/components/Settings.svelte::save` overwrites current preferences from awaited reply; Reset can mutate them concurrently; import calls file.text without admission.

**Proposed minimum fix:** Capture submitted values/draft revision and apply replies only to that revision, preserving later edits. Serialize Reset through the same operation settlement, expose pending recovery and inspect import size before whole read.

### Keep retryable provider recovery alive under the same identity

Remove finite recovery quotas at the developer, role and protocol layers together. Keep cancellable backoff, exact acceptance/resumption rules and genuine nonretryable errors. Backoff limits control spacing, not permission to continue.

#### L049 — Developer-loop admission and generation ceilings

**Current code:** `crates/orchestration/peritus-agent/src/developer/types.rs::DeveloperLoopLimits` caps model turns at 128, tool calls at 2,048, attempts at eight and output at 32,768. `developer/execution.rs` returns `SegmentExhausted` before dispatching a batch that crosses the selected boundary.

**Proposed minimum fix:** Remove arbitrary constructor maxima and retry quotas. Select output from the actual model capacity. Preserve segment scheduling only with transparent continuation, retaining a pending tool batch instead of dropping it at the boundary.

#### L050 — Retry attempts remain finite while elapsed retry horizon is absent

**Current code:** `crates/orchestration/peritus-agent/src/developer/retry.rs::DeveloperRetryPlanner` currently supplies both a finite attempt count and `MAX_ELAPSED_MILLIS = 120_000`. The audit's statement that elapsed retry time is already optional does not describe this baseline.

**Proposed minimum fix:** Remove both the two-minute recovery horizon and finite retry stop for retryable failures. Reuse the retained request/context, with cancellable delay and exact acceptance safeguards.

#### L051 — Same-provider recovery has a three-invocation cap without material progress

**Current code:** `crates/app/peritus-product-runner/src/failover.rs::RoleRecovery` allows fewer than three failed invocations. `turn/provider.rs` resets it on a workspace change, then advances the configured fallback chain or errors.

**Proposed minimum fix:** Remove the three-invocation ceiling and workspace-change condition. Keep retrying the same eligible provider/context; retain explicit fallback policy and do not resubmit an ambiguously accepted request without a documented safe mechanism.

#### L464 C5 retry algebra still requires a finite attempt and delay policy

**Current code:** `crates/model/peritus-model-protocol/src/retry.rs::{RetryInput,plan_retry,validate}` requires positive finite `max_attempts` and `max_elapsed_millis`. It also refuses a provider Retry-After above `max_delay_millis`. Both finite fields are mandatory in this baseline.

**Proposed minimum fix:** Represent an absent attempt/elapsed policy explicitly, update callers, and honor provider Retry-After with cancellable waiting. Keep delay overflow checks, cancellation and the existing ambiguous/partial-response protections.

### Resume from retained phase and valid completed evidence

Persist serializable evidence facts with their existing bindings, not private live execution handles. Reacquire only invalid or unresolved execution evidence.

#### L115 — Durable continuation is a whole-state payload with an exact version gate

**Current code:** `crates/app/peritus-product-runner/src/execution/resume/durable.rs::encode` embeds the full baseline, summaries, diff and evidence in one JSON record; `decode_inner` accepts exactly its current version.

**Proposed minimum fix:** Remove enclosing state policy ceilings through the shared control-state repair, and reuse existing references for accumulated material. Decode a historical version only when it actually exists and is needed; preserve exact candidate and source bindings.

#### L116 — Restart resets every post-writing phase to checks and cannot restore private gate state

**Current code:** `crates/app/peritus-product-runner/src/execution/resume/durable.rs::restored_phase` maps all tags 3..8 to Checking; `decode_inner` explicitly drops gate_report.

**Proposed minimum fix:** Retain the next phase and serializable completed gate/evidence facts, revalidating existing freshness bindings on reopen. Rerun only invalidated checks; never deserialize private process handles as renewed authority.

### Rehydrate saved runs before assembling another provider request

Load durable execution identity and saved evidence independently of next-view admission. Preserve incompatible or blocked records visibly rather than making them disappear into quarantine.

#### L181 — Restart invalidates execution-qualified candidates without comparing current execution facts

**Current code:** `crates/app/peritus-daemon/src/product_run/recovery.rs::reconcile_restored_candidates` marks any candidate with `execution_digest().is_some()` stale even when repository/content match.

**Proposed minimum fix:** Compare the retained execution dependency facts with current facts instead of treating any execution digest as stale. Reacquire only changed or genuinely unverifiable evidence; keep the qualified candidate actionable when its bindings still match.

#### L184 — The complete persisted run record is capped at 16 MiB and crossing it cancels live work

**Current code:** `crates/app/peritus-daemon/src/product_run/persistence.rs::{persist_record,write_record}` rejects at 16 MiB and cancels on persistence failure; `persistence/workbench.rs::load_workbench_records` quarantines larger files.

**Proposed minimum fix:** Remove the 16 MiB run-record ceiling from both writes and startup reads. Use existing artifact references for duplicated large payloads and preserve a visible recoverable persistence error rather than canceling live work solely for record growth.

#### L185 — Recovery depends on admitting the current full context, and ordinary quarantined executions are not retried by the loader

**Current code:** `crates/app/peritus-daemon/src/product_run/persistence/workbench.rs::load_workbench_records` calls `capture_execution` before restoring, retries quarantine only for StartGoal, and `projection_paths` treats read_dir failure as empty.

**Proposed minimum fix:** Rehydrate from the saved execution boundary before attempting a new context view. Keep failed context admission visible as a blocked run, reconsider ordinary StartExecution records after their cause is fixed, and report directory-read failure instead of an empty inventory.

#### L186 — Run persistence accepts only format 6 and requires full governed context even for terminal records

**Current code:** `crates/app/peritus-daemon/src/product_run/persistence/record.rs::into_record_with_context` requires the exact format and unconditionally unwraps governed context before optional resume decoding; `persistence/types.rs` retains all run fields.

**Proposed minimum fix:** Decode actual supported prior record formats with their existing identities, and keep unsupported ones visible as incompatible. Load terminal history without requiring a newly admissible request context; preserve indeterminate-effect and lineage safeguards.

#### L187 — The staged-start recovery projection is unreachable when the start receipt is absent

**Current code:** `crates/app/peritus-daemon/src/product_run/persistence/workbench.rs` passes no capture for an unaccepted start, but `persistence/record.rs::into_record_with_context` requires governed context before its staged projection can be returned.

**Proposed minimum fix:** Construct the existing staged, unaccepted-start projection without requiring accepted governed context. Expose retry of the exact original operation while keeping it unprivileged; acquire execution authority only after acceptance.

### Shutdown waits must not substitute for settled ownership

Use optional explicit shutdown coordination policy and retain unresolved owners/results. A response wait expiring does not establish that work stopped.

#### L182 — Shutdown uses a finite join allowance and marks recovery without proving every task has stopped

**Current code:** `crates/app/peritus-daemon/src/product_run/lifecycle.rs::shutdown` discards join timeout/results and persistence errors, drains task handles, then marks interruption.

**Proposed minimum fix:** Handle join outcomes and persistence errors explicitly. Keep an unjoined task's ownership unresolved and reconcile the existing native owner before marking it inactive; a shutdown allowance must not become proof of completion.

#### L199 — Shutdown applies finite join bounds separately, while product-run cleanup failures are not counted

**Current code:** `crates/app/peritus-daemon/src/startup/runtime/teardown.rs::shutdown` ignores the product-run shutdown outcome and can deliver ShutdownComplete before joining the server.

**Proposed minimum fix:** Propagate product-run cleanup outcomes and actual remaining work into shutdown accounting. Publish ShutdownComplete only after required joins and ownership reconciliation; optional shutdown waits must not falsely settle an unjoined owner.

#### L200 — Worker admission and result retention share a fixed ceiling, and cleanup grace is mandatory and capped

**Current code:** `crates/app/peritus-daemon/src/worker/limits.rs` caps tasks/results at 4,096 and mandatory grace at ten minutes; `worker/supervisor.rs::{reap,drain_results,shutdown}` ties collection to result capacity but already retains abort-resistant tasks as ShutdownIncomplete.

**Proposed minimum fix:** Remove compiled task/grace maxima and make shutdown wait/abort policy explicitly optional. Drain authoritative results independently of new admission so a full result buffer cannot strand finished ownership. Preserve the existing ShutdownIncomplete behavior for abort-resistant tasks and report remaining observations until settled.

#### L231 — Shutdown diagnostics are capped at 16 failures and clean status ignores recorded failures

**Current code:** `crates/app/peritus-daemon/src/shutdown.rs::{record_failure,complete}` caps failures at MAX_SHUTDOWN_FAILURES and derives clean outcome from remaining counts; `shutdown/work.rs` summarizes fixed work categories.

**Proposed minimum fix:** Remove the 16-failure diagnostic cap and count unresolved recorded failures when choosing Clean/Unclean. Preserve exact work-category summaries and ordered stages; ensure failures reconciled successfully are distinguished from unresolved cleanup failures rather than making historical diagnostics permanent blockers.

### Isolate startup catalog failures without relaxing authority

Remove arbitrary catalog payload quotas, keep failed entries visible through recovery IPC and preserve exact signed authority/trusted workspace identity.

#### L229 — Approval registry size and exact-successor policy can block the entire daemon at startup

**Current code:** `crates/app/peritus-daemon/src/startup/registry.rs::{load_configured,reconcile_current}` requires a bounded canonical payload and exact successor; `crates/state/peritus-approval/src/authentication/credential.rs::CredentialRegistrySnapshot::new` caps credentials at 4,096.

**Proposed minimum fix:** Remove credential inventory/preimage byte policy ceilings consistently with digest/codec and durable storage. Keep the eleven-role vocabulary bound, canonical signatures, revocation, exact-successor authority and native integers. A failed registry must expose authenticated recovery diagnostics rather than reset authority or prevent all diagnostics.

#### L230 — Workspace startup checks every retained registration, including removed and unconfigured rows

**Current code:** `crates/app/peritus-daemon/src/startup/workspace.rs::install_and_reconcile` decodes each retained workspace row before checking whether it is configured/Removed; `crates/runtime/peritus-workspace/src/registration.rs` constrains encoding/paths.

**Proposed minimum fix:** Skip removed and unconfigured registrations before decoding their payloads, and isolate a failed workspace instead of rejecting the full catalog. Remove registration-size policy ceilings consistently; keep exact trusted-folder, Git and transaction-root identity checks.

### Keep scheduler cancellation monotonic and worker capacity live

Separate retained evidence from operational occupancy. Preserve explicit terminal results and record late ownership truth without undoing user cancellation.

#### L291 — Removing scheduler workers does not free registration capacity, and drain/terminal lifecycles have no reopening path

**Current code:** `crates/orchestration/peritus-scheduler/src/reducer/apply/worker_control.rs` counts removed workers for registration and only changes their phase. state/mutation/scheduler_phase.rs has Drain but no undrain; terminal aggregates are fenced.

**Proposed minimum fix:** Count live registrations for selected operational capacity, retaining removed-worker evidence. Use explicit replacement identities linked to existing lineage; no implicit resurrection of removed owners. Add explicit quiescent undrain if continued admission is requested, with versioned command/replay support. Keep deliberate finalization terminal and start successor work explicitly rather than resurrecting completed effects.

#### L294 — CancelWorkTree refuses a terminal root even when live descendants remain

**Current code:** `crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation/command.rs` rejects a terminal root before calling cancel_retained, although traversal can follow terminal ancestors.

**Proposed minimum fix:** Allow a retained terminal root for descendants=true, traverse its subtree and cancel only live descendants under existing reservation identities. Leave the root's terminal result unchanged and preserve ordinary single-item terminal behavior; update the explicit old terminal-root rejection expectation.

#### L295 — A late start acknowledgment can erase a requested cancellation, outside the existing non-resurrection proof

**Current code:** `crates/orchestration/peritus-scheduler/src/state/mutation/reservation_command.rs` accepts an unstarted active reservation; acknowledge_start.rs marks it started then unconditionally writes Running, overwriting Cancelling.

**Proposed minimum fix:** Record the start bit while retaining Cancelling when cancellation was already requested. Carry that monotonic intent through completion/loss/recovery and update the actual readiness/start specifications. Preserve historical replay via explicit semantics compatibility; no second cancellation store is required.

### Accept valid knowledge graphs and refresh only stale dependencies

Use existing snapshots and delta packets, retaining stable source/candidate identities and explicit freshness status.

#### L372 S1 snapshot admission conflates stable section identity order with dependency order

**Current code:** `peritus-run-knowledge/src/snapshot/validation.rs:115,146` requires both canonical opaque-ID order and dependencies earlier in that order.

**Proposed minimum fix:** Keep canonical storage order, validate dependency presence independently, and compute a separate topological traversal that rejects true cycles. Update the matching executable/formal topology relation without reassigning stable IDs.

#### L373 S1 knowledge capacities are finite admission policies without an unlimited mode

**Current code:** `limits.rs:29` permits arbitrary positive usize capacities with no compiled maximum; nested sections bind their own constructor policies.

**Proposed minimum fix:** Remove arbitrary production caller quotas where imposed, or select capacities fitting the actual snapshot; no compiled ceiling needs raising here. Align nested admission and mandatory-section requirements, retaining source digest and role checks.

#### L374 S1 delta packets require a completely fresh current snapshot

**Current code:** `delta.rs:37,71` first validates freshness of every current section before returning any packet.

**Proposed minimum fix:** Return fresh sections with explicit stale/refresh-required entries and block only obligations depending on those stale sections. Preserve exact candidate binding and never mark stale material as current evidence.

#### L375 S1 freshness and delta planning repeatedly scan complete canonical vectors

**Current code:** `plan/evaluation.rs` performs repeated source/dependency lookups over canonical vectors, and delta planning clones and rescans prior/current views.

**Proposed minimum fix:** Build source/section/prior-material indexes once and traverse actual edges for transitive invalidation. Keep the same deterministic freshness and packet semantics with incremental host work where necessary.

### Evidence/projection SQLite contention

Retry typed Busy/Locked under cancellable operation ownership instead of treating elapsed contention as terminal failure. Keep database integrity and actual native representation constraints.

#### L620 Evidence storage retains a five-second contention timer and fixed SQLite representation bounds

**Current code:** `peritus-evidence/src/sqlite/store.rs::EvidenceStoreOptions` defaults to five seconds; `sqlite/connection.rs` installs it and a 32 MiB SQLITE_LIMIT_LENGTH.

**Proposed minimum fix:** Replace the contention cutoff with cancellable Busy/Locked retry and remove the explicit value-size policy clamp where legitimate content needs it. Preserve defensive mode, attachment isolation and native integer validity.

#### L626 Projection storage repeats the five-second SQLite contention cutoff

**Current code:** `peritus-projection/src/sqlite/store.rs::StoreOptions` also defaults to a five-second busy timeout and encodes metadata as SQLite integers.

**Proposed minimum fix:** Use the same cancellable contention retry. Check native integer representability before installing a generation and preserve typed overflow rather than classify valid history as corruption.

### Daemon instance lock and IPC admission

Preserve exclusive identity while reporting real filesystem failures and retaining accepted connections through backpressure.

#### L723 Instance admission can misreport filesystem failures and strand temporary publication files

**Current code:** `peritus-daemon/src/instance/lock.rs::acquire` maps every try_lock failure to AlreadyRunning; publish_record uses PID-only temporary names and Windows remove-then-rename.

**Proposed minimum fix:** Map only true contention to AlreadyRunning; reconcile exact stale temporary publication and atomically replace the instance record. Preserve state-root exclusion and birth identity.

#### L724 Windows IPC has an independent fixed 65-instance native ceiling

**Current code:** `peritus-daemon/src/ipc/windows.rs` configures MAX_PIPE_INSTANCES=65 and disconnects the accepted pending pipe if replacement creation fails.

**Proposed minimum fix:** Remove the independent 65-instance policy, count pending accept capacity with configured/native capacity and retain accepted custody under recoverable backpressure. Keep principal/buffer validation.

### Separate unresolved effect identity from derived failure history

Keep exact pending effects pinned; resolved derived failures should not exhaust persistent-memory admission.

#### L077 — Derived failure records, pending handles, and view-binding capacities

**Current code:** `crates/app/peritus-product-runner/src/local_context/memory/ingestion/facts.rs` upserts failure facts and marks later non-error outcomes Resolved. `ingestion/pending.rs` separately rejects handles above 256 bytes; `local_context/view_binding.rs` fixes another 128-MiB codec envelope.

**Proposed minimum fix:** Use the shared removal of retained-entry quotas; keep resolved facts optional at rendering rather than delete referenced history. Remove handle text maxima and align view-binding encoding with enclosing admitted capacity. Preserve unresolved command identity and exact checkpoint binding.

### CLI exact command replay and durable receipt lookup

An uncertain command retry reuses all original identities and bytes and consults durable receipt state before any effect.

#### L685 Exact command retries require original request and correlation identities, which a fresh CLI invocation cannot supply

**Current code:** `peritus-app-protocol/src/command/binding.rs::CommandBinding` digests session/request/correlation/key/revision/frames; peritus-cli operation generates fresh request identity.

**Proposed minimum fix:** Retain and reuse the full binding and exact submitted bytes on uncertain retry. Resolve the existing receipt before resending under the original session/key.

#### L686 Pure idempotency window has a 4096-entry production ceiling and explicit oldest retirement

**Current code:** `peritus-app-protocol/src/command/idempotency.rs::IdempotencyWindow` is an insertion-ordered bounded pure cache; production limit is 4096.

**Proposed minimum fix:** Back cache retirement with the existing durable application receipts, removing lifetime admission dependency on cache capacity. Reject conflicting final facts and preserve exact replay after eviction.

## 4. Process, build, and sandbox resource limits

### Make unrequested process ceilings absent throughout launch and supervision

Represent optional selected resource and cumulative-I/O limits explicitly in process/tool policies, canonical plans, launch validation and enforcement. Keep real backend constraints and bounded physical chunks/windows.

#### L083 — Command caller applies fixed resource, output, and shutdown policies

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/plan.rs::compile` selects eight-MiB output, 16-MiB stdin, 12-GiB memory, 50-GiB disk and fixed process/descriptor limits. Wall and CPU resource ceilings are mandatory in this baseline.

**Proposed minimum fix:** Select no cumulative/resource ceiling unless requested, using the widened shared contracts. Keep bounded display/chunk sizes and truthful physical failure reporting. Remove fabricated CPU time derived from an unrequested wall timeout.

#### L087 — Process policy constructors require bounded I/O and impose hard maxima

**Current code:** `crates/runtime/peritus-process/src/io_policy.rs` requires cumulative stdin/output values and fixed maxima. `resource.rs::ProcessResourcePolicy::new` requires every ceiling, including wall/CPU, to be nonzero. `DeadlinePolicy::new` currently also caps selected wall time at `MAX_DURATION_MILLIS`.

**Proposed minimum fix:** Support absent cumulative/resource limits and remove policy constructor maxima, including the wall-duration maximum. Keep nonzero physical buffers and checked duration encoding. Update matching canonical/backend contracts together; None in DeadlinePolicy alone is currently insufficient.

#### L089 — Resource sampling can stop execution on observation failure and repeatedly walk the workspace

**Current code:** `crates/runtime/peritus-process/src/supervisor/resource.rs::{start,sample}` scans disk synchronously; `resource/disk.rs` errors on unreadable descendants. `supervisor/owner.rs::tick` treats any sample error as supervisor failure.

**Proposed minimum fix:** Skip unselected enforcement and keep incidental probe failure advisory/retryable. Move full-tree sampling off the control/output owner and report incomplete measurements. Do not claim an unobservable explicitly selected mandatory constraint is enforced.

### Sandbox resource policy and native enforcement

Make time limits optional across the admitted contract, feature negotiation and native installation. Keep explicitly selected resource containment; do not confuse simultaneous gauges with lifetime consumption.

#### L556 Sandbox time limits are optional but six resource dimensions still require finite bounds

**Current code:** `crates/runtime/peritus-sandbox/src/resource.rs::{ResourceLimits::new,ResourceUsage::charge}` requires all eight positive finite quantities; there is no optional `ResourcePlan` in this baseline. `terminal.rs` and reference accounting consume these bounds.

**Proposed minimum fix:** Represent wall/CPU limits as optional in this existing contract and update consumers/encoding together. Separate releasable process/handle/concurrency gauges from cumulative usage; retain selected memory, disk and output policy.

#### L557 Untimed sandbox plans still require backend wall and CPU timer features

**Current code:** `crates/runtime/peritus-sandbox/src/plan.rs::derive_features` always requires WallTime and CpuTime alongside the other resource features.

**Proposed minimum fix:** Require timer features only when a corresponding limit is selected, coordinated with L556 and admission/verified predicates. Keep containment features required by the actual plan.

#### L564 Linux cgroups impose a fixed CPU throttle alongside configured resource limits

**Current code:** `crates/runtime/peritus-sandbox-linux/src/cgroup.rs` installs `cpu.max = 100000 100000` and requires CPU/memory/PID controllers; `native/rlimit.rs::install` always installs RLIMIT_CPU and treats disk bytes as RLIMIT_FSIZE.

**Proposed minimum fix:** Remove the unconditional one-CPU throttle; require/install controllers and rlimits only for selected policies. Treat per-file size separately from aggregate disk use rather than claim RLIMIT_FSIZE enforces both.

### Recover provider availability and incidental resource observations

Separate transient availability observations from terminal run state, using the existing run accounting and circuit APIs.

#### L033 — Optional run horizon and provider-circuit lifetime

**Current code:** `crates/app/peritus-product-runner/src/budget.rs::RunAccounting` already accepts `max_elapsed: None`, exposes `close_provider_circuit` and keeps a last snapshot. Its resource observation remains fallible. Ordinary daemon launch in `crates/app/peritus-daemon/src/product_run/execution/launch.rs` passes no horizon.

**Proposed minimum fix:** Keep the optional user-selected horizon. Retry incidental resource probe failures using a visibly stale last observation, and close/reprobe a provider circuit when that same provider becomes usable. Do not invent another horizon or a duplicate circuit API.

#### L120 — Resource observation scans the whole managed tree and stops measuring at two million entries

**Current code:** `crates/app/peritus-product-runner/src/resource_probe.rs::workspace_bytes` returns u64::MAX beyond two million entries and skips descendant errors; process-only/unavailable observations use zero.

**Proposed minimum fix:** Remove the traversal-count stop, represent partial/unavailable measurements explicitly and move blocking scans off the async owner with cancellation. Preserve typed observation errors instead of Budget/Deadline.

### Make build parallelism recommendations overridable

Use observed host resources as guidance; explicit requested parallelism must not be refused by a heuristic.

#### L040 — Build concurrency heuristic is enforced as an execution ceiling

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/resources.rs::{observe,authorize,environment_bindings}` caps recommendations at eight, rejects higher flags, and injects `CARGO_BUILD_JOBS` capped at two.

**Proposed minimum fix:** Remove the heuristic admission refusal and the fixed Cargo cap. Preserve explicitly selected environment/job settings; supply overridable defaults only where absent. Keep actual OS/cgroup enforcement.

### Use existing command ownership for deterministic gates

Run external gates through the same cancellable command lifecycle, with permissions based on the selected command's actual needs.

#### L114 — Every production gate phase requires network authority, even for local checks

**Current code:** `crates/app/peritus-product-runner/src/execution/cycle.rs` requires Read, Write, Process and Network before every gate phase, including local checks.

**Proposed minimum fix:** Require Network only for gates that use it and pass the selected authority to launch. Settle denied phase admission through retained-work recovery instead of losing the candidate.

#### L121 — Exact-target gates execute synchronously outside durable command ownership

**Current code:** `crates/app/peritus-product-runner/src/gates.rs::run_with_ownership` still calls synchronous `Command::output()` and forces Cargo jobs to two.

**Proposed minimum fix:** Use the existing owned command runtime with cancellation, streamed output and exact gate identity. Remove the forced Cargo worker cap and stream executable hashing. Preserve gate results separately from bounded report previews.

### Keep optional semantic inference optional and untimed by default

Use an optional selected timeout throughout auxiliary process configuration and the shared launch contract.

#### L059 — Optional local semantic subprocess envelope

**Current code:** `crates/app/peritus-product-runner/src/context_config.rs::LocalProcessConfig::validate` currently requires 1..60,000 milliseconds, plus fixed input/output/memory ranges. Default semantic backend is Disabled.

**Proposed minimum fix:** Allow no timeout and remove the 60-second and arbitrary payload/memory maxima. Preserve explicitly selected resource constraints, executable/model identity and actual sandbox/cgroup admission.

#### L112 — Optional local inference still requires elapsed/CPU ceilings and fixed resource limits

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/compactor.rs::run_compactor` always supplies configured wall/derived CPU limits and fixed scratch/process/descriptor limits. `compactor/backend.rs` selects a 250-ms macOS probe.

**Proposed minimum fix:** Thread optional elapsed/resource selection from configuration through the shared process contract. Remove the fixed probe cutoff using cancellable native startup; preserve explicit sandbox isolation and deterministic optional-inference fallback.

## 5. Artificial deadlines and retry exhaustion

### SQLite contention retries without a terminal wait deadline

Retry transient SQLite contention for the same operation until it succeeds or is explicitly cancelled. Keep exclusive daemon/store ownership and real corruption/CAS failures distinct. Do not turn the existing global control mutex into an indefinitely blocking retry owner.

#### L019 — Control journal has a 250 ms lock-wait deadline

**Current code:** `crates/app/peritus-daemon/src/product_control/storage.rs` — `ControlStore::open` selects 250 ms and `accept_installs` resolves the original command after append errors; `crates/state/peritus-journal/src/sqlite/connection.rs` — five-second default and `configure`.

**Proposed minimum fix:** Replace terminal failure on busy-timeout exhaustion with a cancellable retry path outside the async executor/global control lock. Resolve the exact command receipt before retrying uncertain append results. Preserve `owner.lock` exclusion, original busy/storage causes, and genuine stale-head or integrity errors; do not simply raise 250 ms to another deadline.

#### L056 — Local-memory ownership has a separate 250 ms acquisition deadline

**Current code:** `crates/app/peritus-product-runner/src/local_context/storage.rs::{open_folder,lock_owner}` sets a 250-ms SQLite timeout and retries file ownership for only 250 ms before collapsing all lock errors.

**Proposed minimum fix:** Remove both contention deadlines. Wait cancellably for handoff contention, preserve live-owner exclusion, and distinguish contention from actual I/O failures; reuse the shared journal contention policy.

#### L066 — Artifact catalog adds a separate five-second lock deadline

**Current code:** `crates/state/peritus-artifact-store/src/catalog.rs::open` separately sets five seconds of SQLite busy waiting; catalog operations open immediate transactions.

**Proposed minimum fix:** Use the same cancellable contention handling around the exact catalog transaction. Preserve Busy separately from corruption and other I/O; removing the SQLite timeout without adding retry would cause immediate failure.

#### L085 — Command ordinal allocation adds another five-second artificial deadline

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/ordinal.rs::reserve` uses five-second SQLite busy timeout before its immediate allocation transaction.

**Proposed minimum fix:** Use cancellable transaction contention retry rather than a five-second failure. Preserve committed uniqueness, legacy occupied-path checks and SQLite integer representation.

### Remove unrequested command deadlines and completion reserves

Use one optional, explicitly selected horizon throughout command parsing, execution plans, authority and phase admission. Absence must mean no elapsed cutoff. Remove both reserve calculations rather than moving the cutoff to another layer.

#### L037 — Selected horizons synthesize a completion reserve and clamp command execution

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor/command.rs` still defaults omitted `timeout_seconds` to 120. `command_budget.rs` subtracts a fifth of a selected horizon; `command_runtime/mod.rs::StartCommand` requires `Duration`, and `command_runtime/plan.rs::compile` turns it into mandatory call/resource limits and `DeadlinePolicy::new(Some(...))`. This differs from the later state described by the original audit. `developer_tools/catalog.rs` also advertises the 120-second default and reserve; its schema and instructions must change with the parser.

**Proposed minimum fix:** Delete the 120-second default and completion reserve. Carry an optional selected deadline through these types and the shared process/tool authority contract; set no deadline when none was requested. Retain duration representation checks and explicit cancellation.

#### L054 — Selected run horizons also impose a separate phase-finalization reserve

**Current code:** `crates/app/peritus-product-runner/src/execution/deadline.rs::{active_window,require_phase_window}` separately subtracts a tenth of the horizon, capped at 60 seconds.

**Proposed minimum fix:** Remove this second reserve and its early phase refusal. Use the same selected overall horizon, if present, without an earlier synthesized cutoff.

### Wait for a consistent candidate observation

Keep the before/after identity check while removing its fixed retry count.

#### L052 — Candidate freshness capture retries exactly three times

**Current code:** `crates/app/peritus-product-runner/src/execution/checkpoint.rs::capture_candidate_axes` loops exactly three times around checkpoint/digest/checkpoint capture.

**Proposed minimum fix:** Retry with cancellable yielding/backoff until those observations agree, or the caller explicitly cancels. Keep exact candidate identity validation.

### Align launch authority with optional elapsed policy

Preserve exact launch capability, lease and one-use binding. Finite logical admission envelopes do not themselves prove a running-command timer.

#### L084 — Command authority still mints finite validity, lease, and budget envelopes

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/{authority,lease,kernel}.rs` derives lease/budget envelopes from mandatory wall milliseconds and authorizes at fixed logical ticks.

**Proposed minimum fix:** Adapt those admission calculations to the optional selected wall policy alongside L037/L087. Keep identity/one-use checks. Do not remove capability validity or invent an elapsed lease kill merely from these fixed logical declarations.

#### L102 — Product command acceptance contracts still declare a synthetic one-millisecond gate timeout

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/contract.rs::command_contract` supplies one millisecond. `crates/foundation/peritus-spec/src/gate.rs::GateExecutionPlan` currently stores mandatory u64 timeout; unlike the audit's later state, there is no optional constructor here.

**Proposed minimum fix:** Make the existing gate timeout optional and update canonical encoding/readers/callers compatibly, then select None for this internal contract. Preserve legacy finite values; do not claim this is only a one-line caller change on the current branch.

#### L113 — Registered-folder authority has a fixed minute window expressed on a logical clock

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/command_runtime/folder_patch.rs` commits a fixed authority window; `command_runtime/journal.rs::open` uses 250-ms SQLite busy timeout.

**Proposed minimum fix:** Use the shared typed contention retry for journal writes. Align the logical allowance only where optional command policy requires it; preserve runtime/lease identity. This declaration alone does not establish a one-minute wall-clock patch kill.

### Remove implicit IPC deadlines and keep idle client control alive

Make deadlines explicitly optional across client constructors/callers. Give a retained connection an IO owner that handles heartbeat/control even while no request is active.

#### L240 — Shared clients have no default request deadline, but idle requests have no heartbeat owner

**Current code:** `crates/app/peritus-app-client/src/connection.rs` stores required Duration and times connect/negotiation; `request.rs` times each exchange; `events.rs` times control writes. `frame.rs` has one Windows pipe-open attempt. The audit's optional-timeout implementation and timeout.rs are absent here.

**Proposed minimum fix:** Change the shared client and CLI adapter/callers to an explicit optional deadline with no default cutoff, including control writes. Add idle heartbeat service with correlated response routing. On interrupted framing, replace the socket and reconcile the same session/operation receipt. Retry Windows pipe-busy with explicit cancellation; preserve negotiation and frame bounds.

### Keep preview Stop available and remove the TUI's fixed launch lifetime

Coordinate UI selection/admission with the shared preview command contract. Exact launch and observed source identities remain necessary.

#### L248 — Preview mutation shares one unresolved receipt gate, including Stop; launch readiness is fixed at two seconds

**Current code:** `crates/app/peritus-tui/src/model/product/preview.rs` selects readiness 2000 ms and wall 600000 ms, not the audit's None. An unresolved Workbench request blocks dispatch before Stop; latest_preview_launch takes the last launch. `preview/profile.rs` uses split_whitespace.

**Proposed minimum fix:** Remove the fixed ten-minute lifetime by making the shared launch profile's wall deadline optional with none by default, through callers/codec/host as grouped with L189. Make readiness explicitly selected and observable rather than killing a slow starting preview. Give Stop separate authorized admission and target an explicitly selected running launch while reconciling the existing receipt. Parse literal argv with quoting or use structured fields; retain source/build digest validation.

### Retry durable outbox delivery without synthetic exhaustion

Preserve message identity and a live fenced lease until acknowledgment or genuine expiry. Attempt counts are diagnostics rather than a reason to discard delivery.

#### L251 — Outbox attempts cannot be unlimited, and exhaustion invalidates a still-valid final lease

**Current code:** `crates/state/peritus-journal/src/outbox.rs` requires positive u16 max_attempts and counts attempts in u16. `sqlite/outbox_store.rs::claim_outbox` marks even actively leased final attempts Exhausted; acknowledge_outbox accepts any fence once state is Acknowledged.

**Proposed minimum fix:** Remove mandatory max_attempts/exhaustion from default retryable delivery and widen checked diagnostic attempts with a compatible schema/type migration. Claim only pending or actually expired rows; never revoke an active lease because another worker asks for work. Preserve exact fencing even for acknowledgment replay. Migrate existing Exhausted rows to explicit retryable recovery without duplicating acknowledged deliveries.

#### L309 — Every E0 directive requires a finite immutable delivery-attempt allowance

**Current code:** `crates/orchestration/peritus-orchestrator/src/directive.rs` requires u16 maximum_deliveries; mark_published consumes it before runtime/driver.rs invokes the publisher. durability passes it to the C0 outbox.

**Proposed minimum fix:** Make retry stopping explicitly optional with no default cap through directive encoding, claims and C0 rows, sharing L251. Preserve the exact directive/outbox identity and reconcile the existing row's claim/effect outcome before republication. Keep publication-before-effect intent; remove finite retry allowance and widen checked diagnostic counts compatibly.

#### L365 E1 materialization outbox delivery is limited to sixteen attempts without replenishment here

**Current code:** `peritus-harness/src/durability/commit.rs:27,263` fixes materialization delivery at 16 claims; reconciliation Retry changes aggregate status without replenishing the directive.

**Proposed minimum fix:** Remove the claim-attempt stopping policy through the shared outbox repair and redrive the same directive/effect identity after reconciliation. Preserve fencing, backoff and terminal operator control.

#### L421 E2 model and report directives have a separate compiled sixteen-delivery ceiling

**Current code:** `peritus-debugger/src/durability/commit/outbox.rs:14` fixes all model/report directives at 16 deliveries.

**Proposed minimum fix:** Use the shared uncapped retryable-delivery policy for these destinations, retaining exact job/directive/claim/outcome identity and terminal controls.

#### L435 E3 hard-codes sixteen deliveries for every effect lane

**Current code:** `peritus-eval/src/durability/commit.rs` fixes every scheduling/execution/publication outbox draft at 16 deliveries.

**Proposed minimum fix:** Remove that separate exhaustion limit through shared C0 outbox policy, retaining pending intent until confirmed completion, explicit cancellation or resolved integrity failure.

#### L449 F0 publication is permanently bounded to sixteen delivery claims

**Current code:** `peritus-evolution/src/durability/directive.rs:249` applies compiled MAX_ATTEMPTS to publication/activation drafts.

**Proposed minimum fix:** Apply shared uncapped recoverable outbox delivery, retaining exact artifact/directive/activation identity until confirmed outcome or explicit cancellation.

### Remove fixed clarification and evaluator recovery restrictions

Preserve failure ownership and the candidate; unresolved information or unavailable infrastructure is continued waiting/recovery.

#### L370 S2 failure disposition imposes one clarification and forbids evaluator recovery

**Current code:** `peritus-obligations/src/failure.rs:76,93` maps ambiguity after one question and every external-evaluator failure to Settle in both specification and runtime.

**Proposed minimum fix:** Remove the one-question rule and add evaluator recovery when available. Retain the same task in waiting/recovery when information or infrastructure is missing; only genuine candidate defects request a fixer.

#### L371 S2 qualification repeats full path and alternative scans without a resumable work boundary

**Current code:** `qualification/verdict.rs:203` searches observed paths separately for each requirement; `evaluation/alternatives.rs:74` repeatedly collects/scans groups and branches; canonicalization copies the ledger.

**Proposed minimum fix:** Merge sorted paths, build alternative membership once and stream canonical hashing. Preserve qualification semantics and formal relations; no new timeout or durable evaluation engine is needed.

### Launcher daemon readiness and configuration ownership

Reconnect to the exact compatible shared daemon and refresh retained configuration before replacement. Readiness is an authenticated protocol fact, not elapsed time or endpoint reachability.

#### L663 Interactive daemon startup now has no synthetic readiness deadline, but readiness is only endpoint reachability

**Current code:** Contrary to the historical heading, `peritus-launcher/src/app.rs` constructs DaemonSupervisor with 30 seconds; daemon.rs kills startup at that bound and applies it to shutdown; endpoint_ready establishes reachability only.

**Proposed minimum fix:** Remove mandatory readiness/shutdown cutoff, make waits cancellable and perform an exact daemon protocol/configuration handshake. Move synchronous version/shutdown helpers off the UI executor.

#### L664 Immutable launcher snapshots can repeatedly replace the same shared daemon

**Current code:** `peritus-launcher/src/daemon.rs::ensure_ready` replaces a reachable daemon when immutable prepared configuration differs; marker writes truncate directly and bind config path plus binary hash.

**Proposed minimum fix:** Refresh/adopt compatible current configuration on reconnect; serialize necessary replacement under the daemon owner. Atomically publish a marker bound to actual configuration contents and process identity.

#### L667 Generated provider ceilings are fixed by route kind rather than the selected model

**Current code:** `peritus-launcher/src/bootstrap/configuration.rs::{render_provider,direct_route}` hardcodes token/features by route kind.

**Proposed minimum fix:** Use retained discovered or explicitly configured model capacities/features; keep user policy distinct. Preserve legacy ignored timeout compatibility fields without reactivating them.

### CLI session and explicit timing options

Keep session choice through interactive launch and make default request/operation waits untimed, with checked arithmetic for explicitly chosen duration policy.

#### L672 CLI default deadlines are removed, but interactive launch ignores supplied session and timeout options

**Current code:** Contrary to this historical heading, `peritus-cli/src/args.rs` still defaults timeout to 30 seconds; runner's Open/Resume paths forward endpoint but discard session/timeout.

**Proposed minimum fix:** Make the shared CLI default optional/None and forward the requested durable session. Forward an explicit timeout consistently or reject it on unsupported interactive paths; use checked Instant arithmetic.

### Preview readiness and optional process timing

Represent readiness as observable progress until ready or canceled, independently of explicit process duration policy.

#### L717 Preview profiles still require a finite readiness deadline even when the process wall deadline is absent

**Current code:** `peritus-app-protocol/src/workbench/launch/profile.rs::WorkbenchLaunchProfile` has mandatory u64 readiness and wall fields and requires readiness>0 and <=wall; wire profile encodes both as u64.

**Proposed minimum fix:** Make both default timing policies optional consistently with the process contract; propagate None through wire and daemon. Preserve explicit user-selected deadlines and exact process ownership.

### Installed service restarts and durable upgrade handoff

Remove service restart exhaustion/forced stop expiration and retain exact upgrade backup/phase custody until installation or rollback actually succeeds.

#### L857 — Installed service templates impose restart and stop limits

**Current code:** `packaging/linux/peritus.service` limits five starts/300s and stop=40s; Windows task permits five restarts but already ExecutionTimeLimit=PT0S; macOS throttle is relaunch delay.

**Proposed minimum fix:** Set Linux StartLimitIntervalSec=0 and TimeoutStopSec=infinity. Replace Windows finite restart allowance with a persistent restart loop under the service launcher, exiting on explicit stop. Keep relaunch delays, which are not work lifetime limits.

#### L858 Upgrades use disposable rollback state and lack a durable session handoff

**Current code:** `packaging/{linux,macos}/Upgrade-Peritus.sh` uses temporary backup/traps; Windows Upgrade-Peritus.ps1 deletes backup in finally even after failed restore. Install scripts replace binaries without durable run handoff.

**Proposed minimum fix:** Retain backups plus a small phase marker in persistent installation state until verified install/rollback success, resuming interrupted phase before another upgrade. Checkpoint/stop/reconnect through the existing owner and retain the same saved run/provider binding.

## 6. Session persistence, reconnect, and operation recovery

### Retry an unresolved restore under its existing identity

An uncertain restore remains recoverable. Preserve the original preparation and filesystem transaction, and record subsequent proven outcomes as later settlement events rather than overwriting old evidence or starting another restore.

#### L014 — RecoveryRequired restore can become a persistent recovery barrier

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/lookup.rs` — `restore_receipt` resumes only `Prepared`; `recovery.rs` — `recover_prepared_restore` suppresses errors; `rewind.rs` — `rewind_preview` blocks on unresolved restores; `crates/app/peritus-product-runner/src/control/checkpoint/restore.rs` — `settle`; `control/record/transition/checkpoint.rs` — `settle_restore`.

**Proposed minimum fix:** Allow reconciliation and settlement from `RecoveryRequired` as well as `Prepared`; preserve the underlying inspection/C1 error. Keep the original preparation ID, use distinct deterministic IDs for later settlement events, and resolve the latest settlement for that restore instead of always using the first fixed settle ID. Let conversation-only branching proceed without claiming filesystem recovery, while conflicting filesystem effects remain fenced.

### Keep accounting storage failures from cancelling useful execution

Goal accounting remains durable and cumulative, with the same reservation and completion identities. Retry transient persistence failures; only operations requiring an unresolved accounting decision wait. Explicit user pause/cancel and actual ambiguous effects remain binding.

#### L028 — Accounting failures cancel the active run

**Current code:** `crates/app/peritus-daemon/src/product_run/execution/goal.rs` — `with_goal_clock`, `record_goal_clock_failure`, `observe_goal_clock`; `interaction/live.rs` rejects work while `persistence_failed` is set.

**Proposed minimum fix:** Do not cancel the provider or set an irreversible global persistence failure for a retryable clock-observation save. Retry that monotonic progress observation with the existing goal identity, and clear the recoverable admission state after successful reconciliation. Retain actual terminal durability failures and user cancellation. The 100/1,000 ms polls are cadences and need no removal.

#### L034 — Goal continuation requires lifecycle/evidence admission

**Current code:** `crates/app/peritus-product-runner/src/control/goal/execution.rs` — `settle`, `observe_graphical_evidence`; `goal/lifecycle.rs` — `restart_eligible`, `resume`, `requirements_changed`; `goal/validation.rs` — `boundary`.

**Proposed minimum fix:** Keep pause, cancellation, achievement, stale-evidence, and missing-external-evidence checks. The existing `resume` already accepts Blocked/WaitingForUser/Paused. Apply transient recovery before terminal `GoalSettlement::Failed` through the owning execution/accounting repair; do not delete goal lifecycle states or automatically override an explicit user pause.

### Reconcile legacy goal accounting without duplicate debits

Use the existing host-goal operation archive and exact receipt lookup. Resolve the legacy semantic identity before admitting a v2 reservation/completion, and retain old usage. Never make migration appear successful by forgetting the old debit.

#### L031 — Legacy goal-tool replay is a schema barrier

**Current code:** `crates/app/peritus-daemon/src/product_control/goal/replay.rs` — `apply_versioned_tool_goal`, `goal_tool_key`, `equivalent_host_intent`; `product_control/goal.rs` — `reserve_goal_tool`, `complete_goal_tool`, `apply_host_goal`.

**Proposed minimum fix:** Replace the unconditional legacy-ID `UnsupportedSchema` return with reconciliation against the retained legacy operation and receipt. Alias a proved equivalent operation to its existing debit; create a v2 debit only when definitely absent. Missing original role/invocation facts must remain visible and require their exact reconciliation, not a fabricated mapping.

#### L738 Legacy goal operation identities intentionally block newer host accounting without migration

**Current code:** The production guard is the same `apply_versioned_tool_goal` in `crates/app/peritus-daemon/src/product_control/goal/replay.rs`; current host operations are retained by `ControlStore::host_goal_operation` in `product_control/storage.rs`.

**Proposed minimum fix:** Use the same legacy-receipt repair as L031. This is not a separate migration framework or another accounting implementation; preserve the existing cumulative usage and original receipt when a legacy role/invocation is proved, and expose the specific missing fact otherwise.

### Keep advisory progress failures from canceling accepted work

Treat notice delivery as a retryable observation separate from ownership of the provider operation.

#### L053 — Failure to publish a waiting notice cancels provider work

**Current code:** `crates/orchestration/peritus-agent/src/developer/execution/provider_turn/progress.rs` emits a waiting notice every 20 seconds, and cancels the provider token if that observer returns an error.

**Proposed minimum fix:** Report and retry the advisory observer failure without canceling the provider future. Keep the notice interval; it is not an execution deadline.

### Retain valid grounding across provider and loop continuation

Give grounding/enrolled-path evidence the same lifetime as the retained role context. Revalidate content when it changes rather than resetting all evidence at each executor construction.

#### L043 — Grounding evidence can reset at tool-executor reconstruction

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor/construction.rs` initializes `GroundingEvidence::default()` in `with_ownership`. `grounding.rs::{required_tool_name,validate}` then requires a new listing/read. `turn.rs` reconstructs the executor on outer role invocations.

**Proposed minimum fix:** Carry grounding and enrolled-path state across those reconstructions, bound to workspace/content identity. Invalidate only affected observations when files change; preserve an existing-target read requirement for genuinely unseen or changed targets.

#### L047 — Writer/fixer loop parameters and error-to-budget mapping

**Current code:** `crates/app/peritus-product-runner/src/turn.rs` selects 48 model turns/512 tools and maps both `SegmentExhausted` and `LimitExceeded` to Budget. `turn/provider.rs` already continues segment exhaustion, while `failover.rs::correction` tells each continuation to list/read again.

**Proposed minimum fix:** Keep transparent segment continuation, remove unconditional re-grounding instructions when retained evidence is valid, and avoid classifying unrelated representation/trace failures as exhausted user budgets. Share the retry-policy repair rather than adding another continuation loop.

#### L038 — Inspection-loop feedback has a finite observation window and does not enforce convergence

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor/effects.rs::observe_delivery_progress` limits nudges, and `executor/inspection_progress.rs` keeps 16 recent fingerprints. Both issue feedback and continue.

**Proposed minimum fix:** No blocker removal is needed for these advisory counts. Keep them advisory; do not replace them with a hard convergence quota.

#### L123 — Mandatory design shape and unlimited validation retries can repeat grounding forever

**Current code:** `crates/app/peritus-product-runner/src/design.rs` reconstructs read-only tools on every correction. `normalize` requires a byte range, title and four sections; `design_scope` silently falls back to Source on malformed/unknown metadata.

**Proposed minimum fix:** Remove mandatory design shape/size minimum for tasks that do not require a design document. Retain valid grounding across corrections, return precise validation errors and diagnose invalid scope metadata. Preserve the ordinary default when no explicit manifest exists.

#### L125 — Reviewer segments and output are bounded while invalid submissions can trigger unlimited fresh reads

**Current code:** `crates/app/peritus-product-runner/src/reviewer_turn.rs` rebuilds tools on every outer retry, selects 4,096 output tokens and explicitly demands fresh listing/reads even for syntax rejection.

**Proposed minimum fix:** Keep the same grounded review context and repair only malformed submission fields. Use actual model output capacity, and transparently continue segments without forcing new reads of unchanged evidence.

### Reuse checkpoint history and distinguish committed state from trace delivery

Reuse the existing checkpoint/event/artifact representation; avoid repeated complete source-index snapshots and full replay on every operation.

#### L063 — Local checkpoints repeatedly serialize the complete growing archive index

**Current code:** `crates/app/peritus-product-runner/src/local_context/memory/checkpoint.rs::publish` writes full state/transcript/source-index artifacts each time, commits roots, updates in-memory checkpoint, then performs fallible trace publication. `memory/recovery.rs::recover` loads all events before skipping the checkpoint prefix.

**Proposed minimum fix:** Reuse unchanged artifacts, represent a growing source index through existing checkpoint references plus suffix changes, and query/replay only uncovered events. Retry post-commit trace delivery against the committed checkpoint instead of reporting it as uncommitted.

### Repair only the incomplete trace tail under its existing owner

Continue the retained lineage from a validated prefix rather than create a new session.

#### L060 — Recovered torn trace data still blocks new context work

**Current code:** `crates/app/peritus-product-runner/src/local_context/memory.rs::open` restores valid trace observations but returns an error when the reader reports a torn tail.

**Proposed minimum fix:** Under exclusive trace ownership, truncate only the proven incomplete trailing write and continue the same lineage. Preserve errors for corrupt complete records and conflicting effects.

### Finish exact artifact publication after catalog contention

Preserve the exact published object and finish its existing catalog transition; keep actual size/digest and failed-writer integrity.

#### L068 — Artifact writer has bounded temporary-name allocation and fatal write admission

**Current code:** `crates/state/peritus-artifact-store/src/writer.rs::create_temporary` tries only 1,024 names. Finalization publishes bytes before `catalog.record_finalized`, and returns failure after publication on catalog error.

**Proposed minimum fix:** Retry name collisions cancellably without the fixed count. Reconcile/retry registration of the same verified published object after transient catalog failure, rather than recreate the effect or delete useful published evidence.

#### L069 — Artifact recovery fences the store on missing bytes or malformed layout

**Current code:** `crates/state/peritus-artifact-store/src/recovery.rs::recover` already contains some corrupt objects, but aborts the scan on missing recorded bytes and several malformed/duplicate layouts.

**Proposed minimum fix:** Report/contain failures per artifact so healthy unrelated objects remain available. Keep access to missing referenced evidence explicitly failed; never manufacture its bytes or identity. Preserve published orphans long enough to reconcile their exact owning transition.

### Preserve deterministic memory when optional inference fails

Keep optional inference outcomes separate from mandatory archive integrity.

#### L080 — Optional local inference still depends on durable-capacity admission

**Current code:** `crates/app/peritus-product-runner/src/local_context/semantic.rs::compact_with` swallows process failure but propagates refresh/update/artifact/failure-receipt errors. `compactor_input` includes eight recent tool observations within selected input capacity.

**Proposed minimum fix:** Keep deterministic fallback and report/retry advisory inference receipt errors without stopping main provider work. Apply the shared archive-admission repair; do not swallow genuine mandatory state corruption or partially committed update errors.

### Retain process-tree ownership through actual settlement

Separate root exit, explicit cancellation escalation and completed tree ownership. A cleanup observation interval must not become a false terminal outcome.

#### L091 — Root exit and owner loss end child-process ownership; cleanup uses finite reap deadlines

**Current code:** `crates/runtime/peritus-process/src/supervisor/owner.rs::tick` calls tree cleanup on root exit. `supervisor/ownership.rs::ensure_tree_quiescent` force-kills descendants and stops waiting at reap_millis. `owner/finalize.rs` separately times root, tree and output cleanup.

**Proposed minimum fix:** Continue observing an intentionally surviving owned tree after root exit. Kill only for explicit cancellation/selected policy, and keep unresolved cleanup owned/reconcilable without a synthetic terminal reap failure. Preserve escalation on a real cancellation.

### Retain command ownership across recoverable adapter errors

Separate rejected controls and retryable publication/rendering failures from authoritative process termination.

#### L107 — Shell finalization consumes recovery ownership before fallible result construction

**Current code:** `crates/tools/peritus-tools-shell/src/execution/active.rs::finalize` takes `owner` before `wait_and_publish` and fallible `terminal::build`, storing the result only afterward.

**Proposed minimum fix:** Retain/cache the authoritative terminal process result as soon as obtained, before fallible tool rendering, and retry publication/rendering from that same result. Do not require a consumed process owner to exist for a second finalization.

#### L111 — Router turns recoverable active-control errors into terminal closure and owner loss

**Current code:** `crates/tools/peritus-tool-router/src/router.rs::{drive,recover}` removes active ownership and terminalizes or marks indeterminate on adapter errors, including control rejection.

**Proposed minimum fix:** Restore the same active entry on transient poll/control/publication errors and expose a typed retryable rejection/backpressure. Close ownership only on authoritative terminal/canceled/lost facts; keep exact replay/epoch checks.

### Report actual interruption causes and retain candidate recovery

Deadline means an explicitly selected elapsed policy expired. Preserve other error categories and committed work.

#### L117 — Settlement converts all budget-category errors into deadline reports

**Current code:** `crates/app/peritus-product-runner/src/execution/settlement.rs::{fatal,cause_from_error}` maps every Budget error to Deadline and bypasses normal settlement for InvalidPrecondition/InternalInvariant.

**Proposed minimum fix:** Distinguish elapsed expiry from counter overflow, resource observation and context/storage failure with typed causes. Retain an existing candidate/fallback handoff for failed phase admission while preserving genuine invariant failure details.

### Reconcile context pins with queue edits and historical forks

Transform queue lifecycle and context preferences atomically, preserving historical bindings while keeping the next view valid.

#### L158 — Pinning a queued input prevents later edit, hold, or withdrawal until the old pin is explicitly cleared

**Current code:** `crates/app/peritus-product-runner/src/control/context.rs::{set,validate_target}` requires exact latest Queued/Incorporated input pins; `control/record/transition/mutations.rs::{apply_queue,apply_brief}` does not reconcile them.

**Proposed minimum fix:** Update or clear the affected pin atomically with the user's edit, hold or withdrawal while retaining its historical binding. Allow an explicitly targeted stale-pin removal without first requiring that same pin to validate.

#### L161 — A historical fork with a pinned queued input fails its own child-state validation

**Current code:** `crates/app/peritus-product-runner/src/control/record/seed.rs::historical_seed` clones preferences while `InputLedger::historical_snapshot` changes queued inputs to Held.

**Proposed minimum fix:** Translate affected pins into valid historical context selections when creating the seed; retain their source binding without granting child execution. Reconcile seed and queue transformation in one operation rather than making the user clear pins before a valid fork.

### Explicit successor goals in the same conversation

Retain old goal/execution history while allowing explicit replacement after settlement; reuse conversation context.

#### L160 — Clearing a goal cancels its record but does not permit a new goal in the same conversation through these controls

**Current code:** `crates/app/peritus-product-runner/src/control/record/transition/goal.rs::{start_execution,start_goal}` rejects any retained binding, while ClearGoal only cancels. `crates/app/peritus-daemon/src/product_run/lifecycle.rs` already permits follow-up input in WaitingForUser/Complete.

**Proposed minimum fix:** Add an explicit successor binding after cancellation/settlement, preserving the predecessor in existing history and refusing overlap with an active owner. This repairs replacement StartGoal/StartExecution; ordinary follow-up continuation already exists and should remain intact.

### Preview operation completion and ownership across restart

Keep launch/control operation identity pending until a confirmed outcome, and reconnect through the shared persistent command-owner repair. Do not relaunch to obtain a lost handle.

#### L189 — Preview stop waits five seconds, then returns even if the process is still Running

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/launch/process.rs::{stop_preview,wait_for_preview_terminal}` ignores stop errors and stops observing after five seconds; `launch/aggregate.rs::admit_preview` replays admission receipts.

**Proposed minimum fix:** Keep an accepted stop operation pending until the native owner reaches a confirmed outcome, and let the same operation continue observing or retrying control. Surface stop errors; the five-second response wait must not finalize an unconfirmed stop.

#### L191 — Restart marks active previews Failed and drops their live runtime ownership map

**Current code:** `crates/app/peritus-daemon/src/product_run/persistence/preview.rs::recover_preview_page` rewrites Accepted/Running to Failed; `launch/process.rs::{preview_process,refresh_previews}` requires an in-memory map.

**Proposed minimum fix:** Persist and restore stable command-owner/result bindings, reconnecting through L086/L094. Keep unknown launch state unresolved rather than Failed; exact operation retries must resume ownership/completion instead of returning only admission or launching again.

### Keep recovery IPC available and scope unresolved ownership

An affected owner must remain fenced, while diagnostics and unrelated work stay accessible. Reuse verified projections rather than exporting/replaying the full journal on every startup.

#### L197 — Any unresolved native process record puts the entire daemon in read-only mode

**Current code:** `crates/app/peritus-daemon/src/startup/recovery.rs::reconcile_processes` returns one diagnostic for any unresolved record; `startup/runtime/runner.rs` turns it into global read_only and disables outbox.

**Proposed minimum fix:** Scope unresolved-process restrictions to the affected run or workspace instead of making the entire daemon read-only. Keep unrelated work and recovery IPC available while exact ownership is reconciled; preserve conflicting-identity protection.

#### L198 — Projection inspection exports the entire journal before readiness, and recovery errors prevent IPC startup

**Current code:** `crates/app/peritus-daemon/src/startup/projection.rs::ensure_current` integrity-exports the whole journal; `startup/runtime/runner.rs` propagates recovery errors before LocalEndpoint::bind.

**Proposed minimum fix:** Bind recovery IPC even if projection inspection or replay fails, exposing the existing read-only recovery state. Reuse valid projection checkpoints and replay only their suffix; keep full integrity verification available without making it a prerequisite for diagnostics.

### One durable native session binding per run and role

This baseline has no runtime-session helper or persisted native thread binding. Add only the host run/role/provider binding and retained native ID needed for exact continuation, stored with existing durable run state; remove ephemeral inference flags and use the supported exact resume path. Preserve the one-inference boundary with all effects controlled by Peritus. Record definite nonlaunch separately from ambiguous acceptance and never create a replacement thread silently.

#### L211 — Native runtime profiles forbid protocol resumption while Codex privately resumes its retained thread

**Current code:** `crates/model/peritus-provider-openai/src/runtime/provider/invocation.rs::{run_turn,arguments}` uses a temp directory and `--ephemeral`; `crates/model/peritus-provider-anthropic/src/runtime/provider.rs::run_turn` uses `--no-session-persistence`. Both runtime/config.rs validate StatelessReplay/Unsupported public resume.

**Proposed minimum fix:** Implement the shared durable native session binding at the runner/provider request boundary and resume the exact retained Codex/Claude native ID. Preserve host-only tools. Public protocol resumption labels should change only if their defined meaning requires it; private thread continuation must not be confused with replay-safe resubmission of an accepted inference. Remove final-response policy admission together with transport/codec repair.

#### L212 — A pre-spawn Codex started marker can permanently fence a session without a native thread ID

**Current code:** The audit's `runtime/provider/invocation/sessions.rs` is absent. Current OpenAI `invocation.rs::run_turn` has no started marker and creates an ephemeral turn.

**Proposed minimum fix:** No started-marker dead-end exists to patch on this branch. In the shared persistence repair, save launch acceptance state and exact native ID, allowing retry only after proven nonlaunch; discover retained IDs incrementally with conflict detection, without a 64 KiB scan cutoff.

#### L213 — Claude marks transient capacity refusal as Never retry and derives resume from a pre-spawn marker

**Current code:** `crates/model/peritus-provider-anthropic/src/runtime/provider/result.rs` classifies Capacity as DefinitelyNotAccepted, but `runtime/provider.rs::runtime_failure` assigns Never. The audit's provider/session.rs is absent and current launch has no resume marker.

**Proposed minimum fix:** Classify definite nonaccepting transient capacity as safe same-route retry and preserve typed errors before success decoding. Add exact Claude UUID persistence/resume through the shared binding, distinguishing proven nonlaunch from accepted/ambiguous work. Do not add the audit's pre-spawn-marker-as-acceptance bug.

#### L215 — Native persistence depends on host metadata, while route changes create a separate namespace

**Current code:** The audit's `crates/model/peritus-provider-core/src/runtime_session.rs` is absent; `ModelRequest` in `peritus-model-protocol/src/request/model.rs` has no host session binding and both native invocations create temporary working directories.

**Proposed minimum fix:** Pass a narrow typed host run/role/provider binding through the existing request boundary and persist native session custody under the owned run state. Serialize access with typed cancellable Busy handling. Keep exact prior namespaces retrievable; explicit route/model changes need a declared context transition, not cross-model native-thread reuse.

#### L219 — Codex usage recovery inspects 64 KiB tails and lets old malformed evidence block new inference

**Current code:** The audit's `runtime/provider/invocation/usage.rs` and `runtime/output/usage.rs` are absent. Current OpenAI `runtime/output.rs::decode_usage` reads turn terminal usage directly; no 64 KiB usage-tail recovery exists.

**Proposed minimum fix:** No tail-reader repair is needed on this baseline. When adding persistent native continuation, keep a durable established usage high-water mark or read complete incremental evidence; unavailable accounting must be explicit without resetting the session or manufacturing usage.

#### L224 — Writer follow-ups use a role that enabled local memory rejects, and disabled memory selects another native context

**Current code:** `crates/app/peritus-product-runner/src/execution.rs` passes `writer-follow-up`; `turn.rs::complete_developer_turn` forwards it to working memory and `local_context/port.rs::open_scoped` rejects that role.

**Proposed minimum fix:** Use stable `writer` identity for working memory and the new native binding, carrying follow-up only as invocation purpose. Reuse the original writer context. Disabling local memory does not currently select another native thread—the baseline has no native persistence at all—so do not describe that later behavior as current.

### Recover subscription gaps and keep flow-control windows nonterminal

Keep finite in-flight windows as backpressure. Recover state at an authoritative cursor and create a replacement subscription without changing durable session identity.

#### L239 — Discovery and subscriptions use fixed concurrency windows, and gap recovery is only admitted before progress

**Current code:** `crates/app/peritus-daemon/src/session/request/discovery.rs` retains up to four JoinSet tasks; `subscription/pump.rs` only detects an initial gap. `crates/app/peritus-app-protocol/src/subscription/transitions.rs` forbids `declare_gap` after progress and `resume` only accepts Paused.

**Proposed minimum fix:** Reap completed discoveries before capacity admission and allow explicit cancellation of stalled reads; return backpressure without disconnect. Extend gap declaration to the current recoverable cursor, with exact handling of already delivered events. Use snapshot plus replacement subscription: there is no transition that resumes `SnapshotRequired` in this baseline.

#### L243 — A subscription gap has no TUI snapshot-recovery transition

**Current code:** `crates/app/peritus-tui/src/model/protocol.rs` turns SubscriptionGap into a notice and no effect. `crates/app/peritus-app-protocol/src/subscription/state.rs` explicitly requires a replacement subscription for SnapshotRequired; no snapshot-install transition exists in transitions.rs.

**Proposed minimum fix:** Add a TUI recovery state that fetches authoritative views bound to one journal cursor, installs them, cancels/replaces the gapped subscription and subscribes from that cursor under the retained session. Add a cursor-bearing snapshot query contract where existing view responses cannot establish that boundary. Keep recovery failures visible until resolved; retain display window limits.

### Make TUI connection cleanup and queues recoverable

Use one cancellable connection owner. A rejected submission must not destroy accepted pending work or cancel the product run.

#### L241 — TUI cleanup uses nested fixed eight-second deadlines; reconnect retains the durable session

**Current code:** `crates/app/peritus-tui/src/client.rs` has eight-second write/close/socket-shutdown timers; `runtime.rs` and `runtime/connection.rs` add fixed connect/close timers. Reconnect already requests `model.retained_session()` before the configured session.

**Proposed minimum fix:** Remove fixed connect/write and nested cleanup deadlines from the normal path, supporting only explicit caller-selected deadlines. Keep one cancellation-aware cleanup owner and report undelivered detach/unsubscribe frames. Preserve the already implemented exact-session reconnect and keep UI cleanup separate from run cancellation.

#### L242 — TUI send-queue exhaustion forces disconnect and drops the remaining transport outbox

**Current code:** `crates/app/peritus-tui/src/runtime/connection/outbox.rs` rejects at 128 queued frames; `runtime/connection.rs` treats send errors as disconnect and clears the outbox. `client.rs` has a separate 256-command writer queue.

**Proposed minimum fix:** Return typed backpressure for the new submission and leave the connection and accepted outbox intact. Reserve/prioritize control delivery independently of user submissions. Retain per-operation definitely-unsent versus possibly-written status so recovery reconciles receipts rather than replaying ambiguous mutations.

### Expose recoverable journal cursors without weakening integrity

Use bounded pages and exact store/head bindings. Never silently skip corruption or substitute a new session after cursor drift.

#### L247 — Global event pages validate every row and reject interior gaps; future cursors are silently empty

**Current code:** `crates/state/peritus-journal/src/sqlite/query/records.rs::global_events_after` returns an empty page when cursor >= latest, including ahead-of-head cursors. `load_aggregate_records` materializes the complete chain.

**Proposed minimum fix:** Return a distinct cursor-ahead recovery outcome bound to the observed store/head; fetch a consistent snapshot before resetting that cursor. Add sequence continuation for aggregate reads that need paging. Keep 4096 as a page bound, contiguous positions, frame digests and hash-chain validation.

### Preserve storage causes and scope recovery to the affected owner

Differentiate resource availability from integrity/identity conflicts so corrective action can resume the same operation.

#### L257 — Journal recovery categories can turn unrelated resource/identity failures into terminal blockers

**Current code:** `crates/state/peritus-journal/src/error.rs::sqlite` distinguishes Busy/ReadOnly but otherwise Storage; recovery maps ReadOnly/MissingArtifact to Terminal. Display hides the SQLite detail although source() retains it and is_storage_exhausted detects DiskFull.

**Proposed minimum fix:** Carry typed resource conditions and actionable causes through callers. Make corrected read-only/storage/artifact availability recoverable for the affected operation without a daemon-wide permanent fence. Preserve same-command resolution after ambiguity and integrity/identity fences for actual corruption or conflicting facts.

### Supervise durable delivery and keep failures scoped

Coordinate this with removal of attempt exhaustion in L251. A lease is ownership, not an execution deadline or evidence of effect absence.

#### L264 — Outbox delivery has a fixed 30-second lease and configured shutdown abort, with no renewal during delivery

**Current code:** `crates/app/peritus-daemon/src/outbox/pump.rs` selects a 30-second lease, never renews while router.deliver awaits, and aborts without joining on configured shutdown timeout. outbox/clock.rs supplies epoch-scoped ticks.

**Proposed minimum fix:** Renew the exact live claim or retain its owner until reconciliation; expired ownership never proves the effect absent. Make stop observable during delivery and settle/reconcile that claim. Replace unconditional timed abort with explicit cancellable shutdown ownership; if an explicitly forced abort is used, join it and retain an unresolved outcome.

#### L265 — One unsupported destination or exhausted message can force the whole daemon read-only

**Current code:** `crates/app/peritus-daemon/src/outbox/pump.rs` calls enter_read_only and exits on exhausted attempts or CorrectRequest/ReadOnly/Operator delivery errors. router.rs requires a configured handler for a closed destination.

**Proposed minimum fix:** Retain a typed blocked/retryable result on the affected message/owner, keep it observable and avoid repeatedly selecting it ahead of unrelated work. Continue other destinations instead of fencing the daemon. Remove automatic attempt exhaustion with L251; preserve integrity fences for the affected corrupt authority.

#### L266 — Outbox claim/acknowledgment errors terminate the worker without retry or live health publication

**Current code:** `crates/app/peritus-daemon/src/outbox/pump.rs` propagates claim/clock/ack errors out of its JoinHandle. startup/runtime.rs watches server completion and signals, with no outbox completion branch.

**Proposed minimum fix:** Observe outbox completion during operation and publish its typed health state. Retry Busy/transient storage errors under the same claim/receipt, resolving ambiguous acknowledgment first. Restart only after reconciliation, and separate stop/control observation from a stalled delivery so supervision does not depend on a new timeout.

#### L269 — Normal daemon startup installs four handlers for a fourteen-destination outbox protocol

**Current code:** `crates/app/peritus-daemon/src/startup/runtime/runner.rs` installs production_children with four routes. outbox/router.rs::production can install ten more adapters but requires a DurableOutboxPort; outbox.rs defines that port contract rather than supplying the missing native resources.

**Proposed minimum fix:** Make durable admission reflect installed executable capabilities. Reject unsupported destinations before committing work; wire the complete router only where an actual owner/resource-bound port exists. Do not treat the generic port trait as a ready production consumer or build ten future integrations merely to remove a blocker. Recover already queued unsupported rows visibly and locally through L265.

### Resolve original committed commands before mutable admission checks

Separate replay of an already authorized committed result from authorization of a new effect. Retain immutable inputs and postcommit recovery facts.

#### L268 — Credential-registry retry checks stale head before it can recover its exact committed command

**Current code:** `crates/state/peritus-journal/src/authority/registry_commit.rs::commit_credential_registry` checks the old expected head before deriving the deterministic command and reaching append's replay branch.

**Proposed minimum fix:** Derive the exact command/request binding first and resolve it. Return its original committed batch on exact replay; apply current-head preconditions only when definitely absent. Preserve conflict detection and authorization for new installs.

#### L270 — Domain commits can fail after becoming durable and reject retries against changed registry facts

**Current code:** `crates/state/peritus-journal/src/domain/commit.rs::commit_state` reads history after append succeeds. domain/approval.rs and approval_use.rs check the current registry before that append/replay path.

**Proposed minimum fix:** Retain original immutable commit inputs and distinguish durable commit from failed postcommit observation. Resolve the exact command before mutable-registry rejection on retry, then recover its original state revision. Align combined state-envelope admission with L250/L256. Do not extend expired authority or remove activation/conservation checks.

#### L276 — Native child retry recovery only recognizes the most recent committed command and repeatedly replays full history

**Current code:** `crates/app/peritus-daemon/src/authority/owner/orchestrator/children/collaboration.rs::exact_collaboration_retry` and scheduler.rs::exact_scheduler_retry inspect split_last and require current state equality.

**Proposed minimum fix:** Find the command's exact event and original predecessor/successor anywhere in retained history, validate that historical transition and return its original result before checking a new current fence. Settle the same outbox claim after effect recovery. Reuse validated child state/prefix observations to avoid repeated full replay.

#### L284 — Scheduler replay reuse covers one aggregate and persistence recovery loses actionable error distinctions

**Current code:** `crates/orchestration/peritus-scheduler/src/durability.rs::resolve_existing` resolves the command then compares the current checkpoint, returning replay error after advancement. session.rs caches one run; journal_error loses Busy/corruption distinctions.

**Proposed minimum fix:** Validate the original historical successor/receipt rather than requiring it equal today's checkpoint and return the original committed batch. Preserve typed recovery causes. Reuse a verified per-owner state/checkpoint suffix when valid; a bounded cache can remain and is independent of provider session persistence.

#### L306 — Collaboration exact retries require the current checkpoint to remain the original successor

**Current code:** `crates/orchestration/peritus-collaboration/src/durability.rs::resolve_existing` compares a committed command's expected successor with the current checkpoint and errors after later progress.

**Proposed minimum fix:** Load/validate that receipt's historical successor revision and return its original committed result. Refresh the current frontier separately and preserve typed storage causes; stale new commands remain fenced.

#### L314 — E0 exact committed retries still depend on the latest checkpoint

**Current code:** `crates/orchestration/peritus-orchestrator/src/durability.rs::resolve_existing` requires the latest checkpoint to match; runtime/driver.rs::step reduces before attempting resolution.

**Proposed minimum fix:** Resolve exact immutable command bytes before reducer fences and validate the original historical successor/receipt independently of today's checkpoint. Reconcile current driver state separately and retain journal error causes.

#### L320 — D0's durability boundary cannot return even an immediate exact committed retry

**Current code:** `crates/orchestration/peritus-agent/src/runtime/durability.rs::commit_agent_transition` checks today's head before encoding/resolving the command; runtime/driver.rs constructs commands from current state.

**Proposed minimum fix:** Resolve original command bytes and historical receipt before current-head admission. Preserve those bytes for exact retries instead of rebuilding the same ID against a later state. Return historical outcome and refresh current frontier separately, without reinstalling an old checkpoint or repeating effects.

#### L363 E1 exact older commit retries require replay instead of returning prior acceptance

**Current code:** `peritus-harness/src/durability/commit.rs:214` finds an exact accepted batch but requires its successor to equal the latest checkpoint; direct decisions apply current fences first.

**Proposed minimum fix:** Resolve historical exact acceptance before new-command fences from its own retained successor receipt, returning the original acceptance without authorizing stale or duplicate effects. Validate current frontier separately.

#### L382 B1 exact approval replay still depends on current credential state before returning idempotence

**Current code:** `peritus-approval/src/state.rs:109` recognizes a matching resolved digest but runs current registry/observation validation before returning Idempotent.

**Proposed minimum fix:** Validate the exact retained historical decision binding and return historical acceptance before live registry/time validation. New authority use must still check revocation, expiry and consumption; replay must never grant a fresh use.

#### L404 B0 rejects exact command replay rather than returning a prior logical result

**Current code:** `peritus-kernel/src/reducer.rs:305` applies current revision/head checks and rejects DuplicateCommand; it stores accepted identities rather than result receipts.

**Proposed minimum fix:** Resolve exact accepted requests in the existing durable receipt layer before calling the pure reducer. Return historical results without granting new effects; keep pure reducer duplicate/conflicting-ID rejection for unresolved new requests.

#### L422 E2 exact durable retry depends on the current checkpoint and the original claim fence

**Current code:** `peritus-debugger/src/durability/commit.rs:196` compares exact acceptance with current checkpoint; claimed commits also bind the original fence.

**Proposed minimum fix:** Resolve the original operation and claim receipt before rebuilding under a new delivery fence. Return historical completion independently of latest state, then continue the current frontier without granting duplicate effects.

#### L436 E3 committed-command replay fails after the aggregate advances and binds effect retries to a claim fence

**Current code:** `peritus-eval/src/durability/commit.rs:363` resolves a batch but compares it to latest checkpoint; bound_digest at :408 includes the delivery fence in request identity.

**Proposed minimum fix:** Resolve historical command/effect completion against original receipt independently of current head. Reconcile the original effect before accepting replacement delivery ownership, keeping logical effect identity separate from claim generation.

#### L450 Ordinary F0 committed-command replay fails after later checkpoint progress

**Current code:** `peritus-evolution/src/durability/campaign.rs:192` and `pointer.rs:165` compare old accepted commands against latest checkpoint.

**Proposed minimum fix:** Return exact historical campaign/pointer receipts from their original producing transaction and successor, independently of later progress. Never regress current state or quarantine benign replay, and retain conflict rejection for reused IDs.

#### L532 Some Git effects lack an idempotent completed-cleanup recovery result

**Current code:** `peritus-git/src/worktree/lifecycle.rs:119` and `snapshot/operations.rs:247` validate old resources before cleanup; repeated recovery can fail solely because completed removal made them absent.

**Proposed minimum fix:** Resolve exact cleanup through the caller's retained intent and expected resource identity. Return idempotent success for proven prior removal, including outstanding parent sync, while rejecting replacements/conflicts; reuse existing durable receipts.

### Resume admitted intent instead of turning interruption into rejection

Retain exact intent at durable admission, resolve effects first, and dispatch definitely absent supported work under its original identity.

#### L277 — An interrupted command between durable admission and dispatch is permanently rejected on exact retry

**Current code:** `crates/app/peritus-daemon/src/command/service.rs` separates admission/dispatch/settlement. authority/owner/storage.rs and startup/recovery.rs reject every DefinitelyAbsent pending command as UnsupportedFamily; the ledger currently retains digests rather than complete dispatch intent.

**Proposed minimum fix:** Persist the exact encoded submission or an immutable existing-state reference with admission, sufficient for dispatch after restart. Reconcile committed commands first. For definitely absent supported work, resume that intent under the same command/idempotency key; retain transient failures as recoverable. Use UnsupportedFamily only for actual unsupported ingress and change the old conformance expectation with this lifecycle contract.

#### L278 — Application dispatch admits a closed subset and erases the reason for semantic or capacity rejection

**Current code:** `crates/app/peritus-daemon/src/domain/dispatch.rs` recognizes a closed family set, collapses decoder errors to MalformedFrame and reducer failures to InvalidCommandFrame, and wraps persistence/replay in generic Reconcile.

**Proposed minimum fix:** Carry typed capacity/stale-state/unsupported causes and owner-scoped recovery through DomainOutcome/AppProtocolError. Reject genuinely unsupported capability before expensive replay, and reuse validated aggregate state. Keep the existing vocabulary unless its native owner is actually wired; removing blockers does not require implementing future domain features.

### Settle collaboration delivery and descendant cancellation truthfully

Retain exact task/message ownership while providing a terminal outcome for unavailable delivery.

#### L304 — Collaboration finalization can wait forever on a delivery, and subtree cancellation refuses an already-terminal root

**Current code:** `crates/orchestration/peritus-collaboration/src/reducer/apply/terminal.rs` rejects terminal cancellation roots and finalization requires all messages acknowledged; apply.rs supports acknowledgment only.

**Proposed minimum fix:** Allow authorized subtree cancellation from a retained terminal parent, changing only live descendants. Add an explicit failed/abandoned delivery outcome to the existing message lifecycle for undeliverable work and let finalization require settled delivery rather than fabricated acknowledgment. Version the event/decoder change and preserve retained delivery identity.

### Continue E0 after empty pause and repairable candidate feedback

Correct state transitions while keeping child reconciliation and acceptance fail-closed.

#### L310 — Pausing with no active children creates an unrecoverable resume-in-progress state

**Current code:** `crates/orchestration/peritus-orchestrator/src/reducer/apply/acceptance.rs::resume` leaves paused_reconciliation set even when active/paused children are empty; directives.rs clears it only after ResumeChildren acknowledgment.

**Proposed minimum fix:** Clear the resume barrier immediately when no child acknowledgment remains. Preserve the recorded paused phase and exact child heads, including pending publication reconciliation; add no timeout or replacement run.

#### L313 — E0's fixed lifecycle ends the run on candidate gate failure or B0 NeedsChanges

**Current code:** `crates/orchestration/peritus-orchestrator/src/reducer/apply/role_cycle.rs::observe_gates` makes CandidateFailed terminal; acceptance.rs::observe_kernel makes NeedsChanges terminal. An existing D2 NeedsFix route already models a fixer handoff.

**Proposed minimum fix:** Route repairable gate failure/NeedsChanges into the existing fixer/revision transition with a complete typed handoff, extending the observation command payload where needed. Preserve invalid-candidate rejection from acceptance and explicit final user cancellation/rejection. Keep historical terminal-event replay compatible; no assumption that this route drives the installed product.

### Restore D0 ownership and admit settlement independently of stop intent

Retain intent-before-effect ordering, exact capabilities and uncertainty evidence. Use the existing coordinator and durable batch state.

#### L318 — D0 cancellation changes phase before it can record outstanding effect settlement

**Current code:** `crates/orchestration/peritus-agent/src/control.rs` changes phase to Cancelling and clears paused_from; reducer.rs::complete_tool requires ExecutingTools. finish_cancellation only checks the phase.

**Proposed minimum fix:** Keep stop intent separate from the effect-observation frontier. Admit authoritative tool/provider terminal observations during Paused/Cancelling, and require settled ownership or explicit durable unresolved obligations before CancellationFinished. Update the existing phase/spec contract together; never redispatch an uncertain effect.

#### L319 — D0 commits dispatch intent before local capacity and ownership admission

**Current code:** `crates/orchestration/peritus-agent/src/runtime/driver/tool_steps.rs::dispatch_tool_once` commits ToolDispatched before checking self.tools or coordinator capacity; runtime/tools.rs::dispatch checks local capacity later.

**Proposed minimum fix:** Perform effect-free coordinator/capacity/authority admission first and retain that admitted slot/capability through the durable intent commit. Dispatch externally only after commit. Record definitely-unstarted errors separately from ambiguous acceptance while preserving mutation serialization.

#### L321 — D0 restores durable tool phases without a way to reattach unstarted authorized batches

**Current code:** `crates/orchestration/peritus-agent/src/runtime/driver.rs::restore` sets tools=None; recovery_report lists only Dispatched/Active. tool_steps.rs::attach_prepared_tools accepts only ProposedToolCalls.

**Proposed minimum fix:** Reconstruct/rebind the exact inert batch in AwaitingAuthorization and recoverable ExecutingTools phases, matching each proposal and current authority. Report missing runtime reconstruction before dispatch; reconcile dispatched/active effects independently instead of manufacturing dispatch intent to classify loss.

#### L325 — D0 rejects an entire multi-call batch whenever any proposal mutates the workspace

**Current code:** `crates/orchestration/peritus-agent/src/tools/batch.rs::new` rejects any multi-call batch containing a workspace mutation; runtime/tools.rs already serializes unsafe dispatch.

**Proposed minimum fix:** Remove the whole-batch rejection and retain the complete ordered proposals. Use the existing coordinator to execute mutations serially with per-call authorization and exclusivity. Keep ordinal/identity/revision checks and align pure/runtime concurrency admission.

#### L327 — D0's fresh-start API can discard an exact provider-resume prefix before admission succeeds

**Current code:** `runtime/driver.rs:389` clears `model_prefix` before fingerprinting or durable admission; `restore_model_once:491` separately enforces exact continuation, but `start_model_once` only checks whether a live model exists.

**Proposed minimum fix:** Reject fresh start while exact resume is pending. Prepare request and successor first, preserving the retained prefix on any failure, and clear it only after a genuinely fresh request is durably admitted.

#### L329 — D0 cannot redrive its pending provider envelope after a failed durable commit

**Current code:** `runtime/driver.rs:423` always calls `pull_one`; `runtime/model.rs:160` rejects a second pull when the prior envelope remains pending after commit failure.

**Proposed minimum fix:** Retry the retained envelope and exact durable command identity before pulling again. Resolve ambiguous commits, acknowledge once after confirmed durability, and leave pending content intact on failure.

### Restore D0 from its original binding and one verified event pass

Persist the original replay inputs once; use a verified state plus event suffix rather than reconstructing the same history twice.

#### L326 — D0 durable restart requires the caller to reconstruct its original binding and limits

**Current code:** `peritus-agent/src/protocol_bridge.rs:110` emits an empty Started payload and `recover_protocol_events:171` requires caller-supplied binding/limits. `canonical.rs:15` hashes their identities and limits, but does not recover them.

**Proposed minimum fix:** Version the genesis payload to retain the original binding and limits, then restore from those values. Preserve old decoding with an explicitly supplied historical binding. Private deterministic role profiles need no new subsystem; persist any future variable profile if introduced.

#### L328 — D0 restart and per-envelope preview perform repeated synchronous full reconstruction

**Current code:** `protocol_bridge.rs:171` reduces each decoded event; driver restore subsequently replays those events. `runtime/model.rs:207` clones the entire C5 reducer for every pending-envelope preview.

**Proposed minimum fix:** Return the verified final state from the decode/replay pass and retain a checkpoint plus suffix. Validate pending C5 changes incrementally without cloning assembled response history, while still committing before acknowledgement.

### Let D1 pause stop new dispatch while accepting effect settlement

Separate control authority from the synchronous external execution call; keep the same gate run and effect identity.

#### L336 — D1 pause freezes effect observations and its effect shell is synchronous

**Current code:** `reducer/apply.rs:38,55,121` excludes Paused from result/recovery/evidence transitions. `engine.rs:231` blocks commands while a permit exists except cancellation; synchronous `GateExecutor::execute` holds the mutable engine.

**Proposed minimum fix:** Allow truthful result, recovery and evidence observations while paused, preserving the saved resume phase. Permit lifecycle controls without losing a dispatch permit, and move blocking executor work outside the mutable control owner so cancellation/pause can be processed. Do not redispatch uncertain effects.

### Resume logical work after a delivery waits or requires recovery

Keep immutable delivery observations, but do not mistake them for permanent termination of the enclosing task.

#### L341 — Run settlement makes user wait and recovery immutable terminal decisions

**Current code:** `peritus-run-settlement/src/reducer.rs:21,47,67` rejects observe/settle after any terminal observation, including user-wait and recovery causes.

**Proposed minimum fix:** Keep this reducer immutable and resume the enclosing logical run from its checkpoint after answer/reconciliation. Restrict final task completion to the actual final boundary; do not rewrite earlier delivery truth or create a new context identity.

### Refresh memory policy without discarding stable lineage

Expiry is already optional; preserve deliberate invalidation and make freshness recovery explicit.

#### L343 C6 optional expiry is immutable across review; freshness crosses an epoch barrier

**Current code:** `record/transitions.rs:90` preserves the old expiry during release/review; `retrieval/filter.rs:39,89` excludes expired records and treats a different review epoch as stale.

**Proposed minimum fix:** Keep expiry absent for persistent context. Add an explicitly reviewed successor that can amend/remove expiry under the same lineage. Represent cross-epoch age as unknown rather than semantic invalidity, retaining visibility, trust and deliberate invalidation rules.

### Keep lease authority recoverable without resetting logical work

An authority lease is distinct from a task deadline. Remove unsolicited local expiry while preserving generation fencing and safe recovery of uncertain holders.

#### L376 B1 mutation leases always have finite expiry and cannot renew after the boundary

**Current code:** `peritus-leases/src/scope.rs:88` requires positive LeaseDuration; `transition/lifecycle.rs:79,171` always calculates expiry and refuses renewal after it.

**Proposed minimum fix:** Support explicitly untimed local leases where no validity policy was selected, updating claim/codec/formal relations. For issuer-selected timed leases, fence/reconcile then reacquire valid authority under the retained intent; never reset task or provider context.

#### L377 B1 a dirty or indeterminate lease inspection makes quarantine unrecoverable

**Current code:** `transition/reconciliation.rs:44` only accepts Reconciling; dirty/indeterminate evidence at :94 transitions into Quarantined with no reinspection route.

**Proposed minimum fix:** Allow correlated reinspection from Quarantined. Return to availability only with current workspace safety and holder-quiescence evidence, retaining original identity and monotonic generation fences.

#### L378 B1 authority-clock discontinuity recovery is unavailable in several nonactive phases

**Current code:** `transition/lifecycle.rs:65` validates the old clock floor before acquire; explicit epoch reset in `reconciliation.rs:245` is tied to ClockDiscontinuity fencing unavailable in several phases.

**Proposed minimum fix:** Add explicit clock rebinding for Available and reconciliation/quarantine states with correlated safety evidence. Do not create a new lease identity to evade the old floor; update phase guards and formal transitions together.

#### L379 B1 expired-claim recovery guidance does not name the required fencing transition

**Current code:** `failure.rs:166` maps ClaimExpired to Reauthorize, while renewal rejects expired claims and fencing is needed before availability.

**Proposed minimum fix:** Return the specific fencing/reconciliation recovery action for expiry. Preserve pending use and context, reconcile the holder, then reacquire authority instead of repeating an impossible renew/reauthorize path.

### Separate authority validity from the lifetime of pending work

Keep explicit issuer-selected validity and parent constraints, while making local untimed authority and renewal/replacement representable.

#### L381 B1 approval waiting and approved use are limited by the earliest of five validity deadlines

**Current code:** `peritus-approval/src/authentication.rs:251` validates request, scope, requirement, credential and decision windows; resolved lifecycle expiry uses the same retained clock epoch.

**Proposed minimum fix:** Retain signed expiry as an authority boundary and renew/replace expired approval under the same pending action. Use untimed local authority only where issuer policy permits, preserving one-use action binding and task/context identity.

#### L383 B1 approval clock changes have no continuation path in the examined resolved reducers

**Current code:** `state/lifecycle.rs:120` cannot expire a resolved approval in a changed epoch; use/authentication paths reject that same mismatch.

**Proposed minimum fix:** Allow explicit cancellation/replacement of an unconsumed approval after clock discontinuity, retaining the original decision and pending intent. Renew authority rather than rewriting signed times or resetting work context.

#### L386 Policy validity is obligatorily finite even when logical uses are unlimited

**Current code:** `peritus-policy/src/validity.rs:86` requires a finite half-open window even when use limits are unlimited; scopes, ceilings and capabilities share that type.

**Proposed minimum fix:** Add explicit untimed validity where allowed by the authority issuer, updating containment/intersection and dependent approval/lease codecs. Preserve selected expiry and parent restrictions; otherwise renew authority under the same session/action.

#### L387 Policy clock-epoch recovery guidance cannot repair the retained authority floor

**Current code:** `peritus-policy/src/time.rs:52` rejects changed epochs against the retained floor; `failure/metadata.rs:36` classifies epoch mismatch, regression and overflow alike as Reobserve.

**Proposed minimum fix:** Distinguish epoch change from same-epoch regression. Route changed epochs to authority renewal/rebinding, and same-epoch catch-up to reobservation, retaining the rejected request and avoiding retries against an unchanged impossible capability.

#### L390 Policy can create an approval challenge whose approval window has already expired

**Current code:** `evaluation.rs:192` validates effective scope time but `evaluation_approval_result.rs:54` constructs the constrained challenge without checking current membership in the approval requirement's window.

**Proposed minimum fix:** Before issuing a challenge, check the effective approval window at the current observation. Return authority-refresh recovery for elapsed windows and waiting for future windows, preserving the pending action instead of publishing an unusable challenge.

### Advance B0 work without losing session, phase or historical decisions

Keep the enclosing durable session and exact child ownership; explicit successor transitions replace incidental lifetime rejection.

#### L400 B0 session aggregate cannot advance its revision or acceptance-contract binding

**Current code:** `peritus-kernel/src/aggregate.rs:20` binds one revision/contract globally; command vocabulary has no rebinding transition and reducer preflight checks exact retained binding.

**Proposed minimum fix:** Add a checked successor run/contract binding under the same enclosing session, reusing host continuation where already available. Invalidate only evidence affected by the changed binding, retaining historical contracts and provider context.

#### L402 B0 pause loses the fixer phase and excludes pending/reviewing runs from session pause

**Current code:** `reducer/run.rs:85` collapses Running/Fixing into Paused and resumes to Running; session pause requires every nonterminal run already Paused, excluding Pending/Reviewing.

**Proposed minimum fix:** Persist the pre-pause phase and restore it exactly, including Fixing. Admit pause for Pending/Reviewing while preserving live attempt/review ownership and controls, updating state/codec/formal relations together.

#### L405 B0 permits only one waiver request per finding for the entire aggregate lifetime

**Current code:** `reducer/waiver.rs:53` rejects any existing finding waiver; the retained lookup does not distinguish denied/invalidated historical requests.

**Proposed minimum fix:** Allow an explicitly authorized successor waiver request for the same finding/current revision with its own request identity. Retain prior denial/invalidation; never silently override a denial or manufacture another finding identity.

### Resume the same debugger model attempt and preserve typed failure facts

Reuse existing model progress, directives and provider continuation; avoid repeating start or retrying permanent configuration failures.

#### L417 E2 model plans forbid provider context continuation even with storage opt-in

**Current code:** `model/plan.rs:99` rejects every continuation even with allow_provider_storage and otherwise enforces a tool-free structured analysis request.

**Proposed minimum fix:** Allow exact validated provider continuation metadata bound to this analysis job/profile. Keep tools disabled and strict proposal validation; persistent context does not grant mutation authority.

#### L418 E2 restart advertises an already-started attempt resume that its executor cannot perform

**Current code:** `runtime/recovery.rs:43` returns ResumeModelAttempt for ModelRunning; `runtime/model.rs:75` only admits ModelPending and always commits a start transition.

**Proposed minimum fix:** Distinguish initial start from recovery, retaining the same model/attempt identity and skipping a duplicate start transition. Reconnect exact continuation or reconcile its uncertainty; report temporary claim contention as waiting rather than quarantine.

#### L419 E2 collapses permanent provider failures into retries and records zero failed-attempt usage

**Current code:** `model/runner.rs` converts provider errors to ModelProtocol/Retry; `runtime/model.rs:211` emits failed-attempt usage as zero.

**Proposed minimum fix:** Carry typed provider permanence, acceptance certainty and observed usage through failure settlement. Retry recoverable failures without a synthetic attempt ceiling, retain deterministic fallback and require configuration correction for permanent profile errors.

#### L420 E2 retry eligibility is persisted as a bare clock tick without epoch or bounded-delay proof

**Current code:** `aggregate/types/model.rs:73` and `runtime/model.rs:150` use bare monotonic ticks and a finite max_delay_ticks policy; durable retry commands retain no epoch/unit.

**Proposed minimum fix:** Add clock epoch/unit to existing retry scheduling state and an explicit rebasing transition. Remove arbitrary delay maxima and make runtime/reducer admission agree; retain checked arithmetic and backoff rather than treating elapsed accounting as a watchdog.

### Select and validate debugger evidence without whole-export barriers

Reuse journal/provenance indexes and published owners; retain requested causal closure and exact evidence validation.

#### L416 E2 narrow evidence queries still index all C0 records and validate/clone all same-subject observations first

**Current code:** `selection/engine.rs:27` indexes all exported records and validates same-subject candidates before direct query filtering; incomplete unrelated run bindings can reject selection.

**Proposed minimum fix:** Filter before cloning/validating unrelated observations and use indexed journal/provenance retrieval. Include requested causal ancestors, validate selected evidence precisely, report unrelated incomplete traces separately and keep cancellation responsive.

#### L423 E2 published-report reconciliation has no matching publication execution branch

**Current code:** `runtime/recovery.rs:71` requests reconciliation of Published when owner facts are missing; `runtime/publication.rs:56` accepts only ReportReady.

**Proposed minimum fix:** Add owner re-verification for the existing published artifact/evidence receipts without replaying terminal publication. Distinguish temporarily unavailable owner observations from confirmed missing data; interrupted ReportReady work continues through the existing executor.

#### L425 E2 analysis and validation repeatedly scan complete evidence before checking most output caps

**Current code:** `timeline/builder.rs:172` rescans manifest entries for each subject; causal/clustering/report validation repeat searches; `durability/replay.rs:73` checks and applies each event separately.

**Proposed minimum fix:** Index events/subjects/citations once and reuse validated citations. Reduce each durable event once, resume verified checkpoints, stream report hashing and isolate/cancel bulk analysis; no new analysis persistence layer is required.

### Keep E3 cancellation and retry delivery connected to actual work

Reuse campaign/rollout identities, existing directives and partial observations; settle old claims atomically with durable next work.

#### L431 E3 blocking stage effects cannot observe cancellation through the execution-port contract

**Current code:** `execution/port.rs:56` checks cancellation only around synchronous execute_candidate/execute_evaluator calls; the ports do not receive the probe and failures can lose stage usage.

**Proposed minimum fix:** Pass cancellation into the owned execution stage or use the existing controlled command runtime. Retain partial results and failed-stage usage; reconcile late observations while preserving cancellation intent.

#### L433 E3 refuses cancellation once deterministic analysis begins

**Current code:** `aggregate/reducer.rs:285` refuses cancellation in Analyzing/ReportReady although publication is not final yet.

**Proposed minimum fix:** Admit cancellation in those phases and honor it at owned analysis/publication safe points. Retain completed results and exact publication identity; never relabel it as generic failure or discard already committed publication facts.

#### L434 E3 retry retention acknowledges the only execution directive without scheduling the next attempt

**Current code:** `aggregate/reducer.rs:263` marks retry Scheduled, but `durability/commit.rs:158,170` acknowledges the prior execution claim without producing a retry directive.

**Proposed minimum fix:** Atomically persist the next attempt's execution directive with RetainRetryableAttempt and old-claim acknowledgement. Include attempt identity in delivery binding and recover that specific pending work instead of leaving Scheduled without a deliverable effect.

#### L437 E3 recovery can request analysis from ReportReady and treats terminal publication as complete without owner checks

**Current code:** `runtime/recovery.rs:57` declares terminal phases complete without owner checks and can fall from ReportReady into BeginAnalysis when the publication directive is absent.

**Proposed minimum fix:** Recover/reconstruct missing publication intent from the already committed ReportReady record rather than restarting analysis. Verify required artifact/evidence owner receipts before terminal completion; distinguish temporary owner unavailability from corruption.

#### L441 E3 can publish analysis/report bindings that F0 later rejects with no in-place campaign repair

**Current code:** `runtime/artifact.rs:125` admits reports by campaign/profile/artifact identity without comparing semantic analysis/dataset/plan digests. `peritus-evolution/.../evaluation/evidence.rs:54` later requires those exact bindings.

**Proposed minimum fix:** Check analysis, dataset and plan semantic digests before report admission/publication using the existing validated report. Retain already published mismatches for an explicit linked correction, preserving downstream evidence requirements rather than weakening them.

### Restore OpenAI continuation authority and response assembly together

Reuse the existing persisted continuation and retained normalized prefix; a cursor alone cannot recreate decoder state.

#### L494 OpenAI exact-resume cursor does not restore stream assembly state

**Current code:** `peritus-provider-openai/src/client.rs:257` constructs OpenAiStream with a new empty ResponseState even for resume; `stream/state.rs:38` has no prior items/parts. restore_continuation validates/rebinds only profile/cursor.

**Proposed minimum fix:** Restore decoder items/parts and reducer prefix with the exact response cursor before accepting suffix events. Feed the already retained prefix into a checked decoder restoration path or reconcile a verified provider snapshot; keep the same response authority and reject unsupported exact restoration honestly.

#### L495 OpenAI background response registration silently stops after 4096 identities

**Current code:** `stream/terminal.rs:28` silently skips response-authority insertion once 4,096 client-local IDs exist. `client.rs:283` already rebinds a persisted continuation into the registry on restore.

**Proposed minimum fix:** Remove the silent insertion cutoff and use the existing persisted continuation binding to reconstruct authority after reopen. Preserve resume/cancellation rights for every accepted response; extend durable custody only where the current run path has not retained that binding, rather than creating a second registry.

### Recover telemetry from verified retained checkpoints and suffixes

Keep trace integrity and queue-loss accounting separate from execution recovery.

#### L516 Telemetry projection and checkpoint recovery replay whole trace histories

**Current code:** `peritus-telemetry/src/projection.rs:148` collects/orders complete trace history and repeatedly projects spans; `recovery.rs:42` recomputes the entire disposed prefix.

**Proposed minimum fix:** Index span observations and retain verified checkpoint prefix state, then replay only its suffix incrementally. Preserve explicit dropped-record counts without conflating telemetry loss with lost execution.

#### L517 The newest telemetry checkpoint can block recovery despite retained older checkpoints

**Current code:** `storage.rs:211` loads only the maximum checkpoint generation and fails if its fixed-size file/checksum/binding is invalid, despite retaining older generations.

**Proposed minimum fix:** Try older retained verified generations in descending order, report the damaged file and replay the remaining suffix. Keep binding/checksum and single-owner cleanup checks; migrate only supported legacy formats.

### Keep proxy cleanup and handoff under exact live ownership

Use existing native process/cgroup ownership and socket handles; supplied recovery metadata alone cannot reattach effects.

#### L536 Proxy cancellation can block on unsupervised host calls and header reads

**Current code:** `resolution.rs:29` uses blocking to_socket_addrs, credential acquisition blocks, headers precede cancellable worker polling; `proxy/mod.rs:153` joins synchronously and owner accept errors can bypass cleanup.

**Proposed minimum fix:** Make resolver/credential/header operations cancellation-aware, actively shut down owned sockets and guarantee cancel/join/revoke cleanup on every exit. Preserve unresolved ownership truth instead of detaching work or adding an abandonment deadline.

#### L538 Managed DNS and protocol compatibility can reject otherwise usable connections

**Current code:** `resolution.rs:35` rejects the whole address collection if one answer fails policy; `proxy/connect.rs:18` chooses only the first admitted answer.

**Proposed minimum fix:** Try every independently admitted address with cancellation, reporting denied answers separately without using them. Preserve explicit deny/special-address policy and cached checked rules; UDP remains unsupported unless a real authorized datagram implementation is needed.

#### L542 Proxy recovery records classify supplied facts rather than reconnecting owned sockets

**Current code:** `recovery.rs:20` checksums supplied owner/port/worker facts and classifies caller booleans; ManagedProxy owns actual thread/socket resources separately.

**Proposed minimum fix:** Obtain exact listener/plan/worker observations from the existing native owner and attach only verified matching resources. Reconcile interrupted release using retained intent and confirmed absence; no provider-context change or invented attachment proof is valid.

#### L543 Inherited listener handoff assumes a complete record from a stream read

**Current code:** `proxy/inherited.rs:200` resets the record buffer for each recvmsg on a Unix stream while receiving an owned descriptor.

**Proposed minimum fix:** Retain partial handoff bytes and the received descriptor until the complete record arrives, or use message-oriented IPC. Preserve exact descriptor count, close-on-exec, cancellation and loopback checks; no timer is needed.

### Preserve credential and ordinary HTTP recovery facts

Do not silently change authenticated request semantics or turn valid redirects into transport EOF.

#### L539 Upstream credentials require expiry and use counts, but exhaustion is silently omitted

**Current code:** `proxy/worker.rs:142` converts any CredentialLease::consume error into None and consumes before provider acquisition.

**Proposed minimum fix:** Propagate required credential expiry/exhaustion as ReacquireCredential, validate owner/scope before consumption and resolve failed pre-acquisition charging. Retain explicit credential validity and the same pending request identity.

#### L541 Proxy redirect and HTTP restrictions turn some normal responses into connection failure

**Current code:** `redirect.rs:15` parses only plain HTTP despite its broader contract; `proxy/worker.rs:91` errors when a valid 3xx cannot be internally replayed.

**Proposed minimum fix:** Forward ordinary 3xx when internal following is denied/unsupported. Reauthorize every followed destination and explicitly report unsupported request framing; preserve valid response bytes instead of closing as failure.

### Keep secret delivery renewable and cleanup recoverable

Retain exact scope/owner and issuer revocation; renew local policy only where authorized and preserve owned files after partial failure.

#### L545 Secret delivery requires finite expiry and use budgets and cannot renew an active lease

**Current code:** `peritus-secrets/src/lease.rs:59` requires finite uses/expiry and consume checks expiry before binding; preparation captures time in advance and deliver consumes a moved lease before staging.

**Proposed minimum fix:** Represent optional/renewable local use/expiry policy, validate exact scope/owner before mutation and observe current delivery time. Return/retain the owned lease on staging failure, preserving issuer-selected revocation and exact delivery identity.

#### L548 Failed file staging and destructor cleanup can lose ownership of secret artifacts

**Current code:** `delivery.rs:170` registers a file only after stage_file succeeds; release retains failed deletions in memory but Drop suppresses its errors.

**Proposed minimum fix:** Register exact exclusive staging-path ownership before writing and retain partial files/cleanup failures in the delivery session. Record pending cleanup through the consuming host's existing operation recovery and reconcile that same identity on retry, keeping restrictive permissions.

#### L549 Secret recovery classifies supplied metadata without reattaching the delivery session

**Current code:** `recovery.rs` classifies supplied delivery metadata/resource-present facts, while live SecretDeliverySession owns files and leases separately.

**Proposed minimum fix:** Verify exact file/lease ownership through the native command owner before reconstructing a delivery session. Retain interrupted creation/release intent; checksum or resource-present booleans alone are insufficient attachment proof.

#### L550 Credential-store availability probing hides causes and blocks synchronously

**Current code:** `store.rs:95` calls a boolean availability probe before native keyring access, hiding locked/denied causes; operations are synchronous.

**Proposed minimum fix:** Return typed native locked/denied/unavailable errors directly and run blocking access outside the control executor. Bind the effective namespace, validate real material admission and treat proven prior removal idempotently.

### Preserve redaction coverage for as long as secret-bearing data remains

Remove synthetic redaction expiry without weakening fragment coverage or exposing raw secret material.

#### L546 Redaction fingerprint construction expands the entire secret before applying its count limit

**Current code:** `fingerprint.rs:51` materializes every sliding fragment hash before checking 65,536 entries.

**Proposed minimum fix:** Use streaming/rolling or direct matching with equivalent exact and 4..64-byte fragment coverage, avoiding the full expansion. Validate actual material representation before expansion and keep bulk work cancellable.

#### L547 Redaction expiry disables matching even while secret material may remain live

**Current code:** `fingerprint.rs:80` returns false after expires_epoch_millis even if delivered secret/output material remains live.

**Proposed minimum fix:** Remove elapsed-time expiration from matching. Retire redaction only after verified secret material and retained secret-bearing output retirement, keeping coverage across long-running work.

### Linux activation and cleanup custody

Retain one owner through activation, recovery and cleanup. Replace elapsed-time abandonment with cancellable, truthful pending states; never infer cleanup from incomplete ownership evidence.

#### L565 Linux cgroup cleanup has a three-second cutoff and can lose its retry owner

**Current code:** `peritus-sandbox-linux/src/cgroup.rs::cleanup` abandons polling after three seconds; Drop ignores cleanup errors and missing/malformed populated evidence can look empty.

**Proposed minimum fix:** Retain the cgroup owner until verified empty and removed, reporting pending cleanup without a lifetime cutoff. Treat unreadable/malformed population data as indeterminate and preserve partial-install cleanup ownership.

#### L566 Linux recovery classifies a cgroup but cannot restore a native session

**Current code:** `peritus-sandbox-linux/src/recovery.rs::{classify,cleanup_exact}` stores metadata, requires a live original root and accepts one descendant as LiveOwned; it does not restore streams or a native session.

**Proposed minimum fix:** Bind recovery to the exact existing command/process identity and verify every member plus empty/dead completion. Remove arbitrary ancestry depth as proof. Do not advertise this metadata classifier as native session reconnection; retain the command's actual owner/streams where persistence is required.

#### L567 Linux child execution remains bound to the launching process

**Current code:** `peritus-sandbox-linux/src/runner.rs` always supplies bubblewrap `--die-with-parent`; session owners and protected streams live in the launching process.

**Proposed minimum fix:** For persistent work, launch under the durable command owner and reconnect clients to that owner, removing client-parent death coupling. Keep explicit cancellation and containment; changing this flag alone would orphan execution.

#### L568 Linux capability probing can block indefinitely before execution is prepared

**Current code:** `peritus-sandbox-linux/src/probe.rs::run_bounded` actually has a probe deadline in this baseline, polls before draining pipes and then uses wait_with_output; output formatting also rejects over 256 bytes.

**Proposed minimum fix:** Run probes through cancellable process supervision with concurrent pipe draining and explicit probe outcomes. Remove the synthetic probe deadline and tiny diagnostic cutoff; use PTY only for probes requiring one.

#### L569 Linux activation consumes its status owner before a blocking observation

**Current code:** `peritus-sandbox-linux/src/session.rs::activated` takes `exec_status` before observe; `exec_status.rs` sets a five-second socket read timeout.

**Proposed minimum fix:** Retain the status owner until authenticated success/failure is observed. Read the finite status frame cancellably without a five-second failure; preserve an indeterminate activation for recovery rather than losing the reader.

#### L573 Linux release can lose proxy ownership and hides actionable recovery causes

**Current code:** `peritus-sandbox-linux/src/network.rs::shutdown_managed_proxy` takes the proxy before fallible shutdown; preparation mapping replaces distinct causes with generic Spawn/CancelAndReap. Session release otherwise restores cgroup ownership on failure.

**Proposed minimum fix:** Keep proxy custody on incomplete shutdown and propagate the original operation/recovery cause. Preserve already-correct cgroup retry ownership; no change is needed for an unreachable observation-cap exhaustion.

### Workspace candidate and rollback completion

Retain the existing action, patch and snapshot identities through each completion stage; allow exact recovery of a dirty result without weakening authorization or inventing a second transaction engine.

#### L582 A dirty Git workspace can lose the only object needed to finalize its candidate

**Current code:** `peritus-workspace/src/mutation.rs` returns an in-memory MutationOutcome after making state Dirty; `candidate.rs` requires it and clean-only rollback admission cannot resolve this state after restart.

**Proposed minimum fix:** Reconstruct the exact mutation outcome from retained patch/action evidence and expose same-action candidate completion. Permit separately authorized rollback of proven dirty preimages; do not synthesize an unrelated mutation.

#### L583 Candidate and rollback completion cross multiple uncoordinated durable effects

**Current code:** `peritus-workspace/src/candidate.rs` creates Git candidate and snapshot before checking next revision, then publishes artifact and installs memory state; rollback has equivalent separated effects.

**Proposed minimum fix:** Preflight counters, then record completion stages against the existing action receipt so retries reconcile snapshot/artifact/state installation idempotently. Keep compensation failures attached to the same operation.

#### L584 Workspace restart inspection performs full synchronous scans and can block unrelated recovery

**Current code:** `peritus-workspace/src/reconcile.rs::inspect_transactions` synchronously sorts/scans the full namespace and folds transaction, Git and publication failures into one workspace condition.

**Proposed minimum fix:** Inspect the requested exact transaction cancellably, separating cleanup pending from dirty/unknown content and publication retry. Index retained transaction identities rather than block one recovery on unrelated namespace scans.

#### L586 Workspace mutation admission inherits finite capability and lease expiry gates

**Current code:** `peritus-workspace/src/gateway.rs` and `verified.rs` validate capability/lease expiry and exact holder, action and generation bindings.

**Proposed minimum fix:** Use the shared optional-authority policy where authorized and return the precise stale/expired receipt. Renew authority for the retained mutation intent; preserve all identity and generation fencing.

#### L751 Initialization effect and final control receipt have an unreconciled publication gap

**Current code:** `peritus-daemon/src/product_run/workbench/folder_mutation/init.rs::apply_init` resolves C0 receipt, then checks old proposal preimage, mutates C1 and only afterward records initialization in C0.

**Proposed minimum fix:** On exact retry, reconcile the retained C1 initialization transaction into its original C0 receipt before checking the stale proposal preimage. Reuse action/manifest identity without reapplying the patch.

### macOS activation and retryable cleanup

Keep exact session, helper status and cleanup custody through fallible transitions; lifecycle evidence follows observed native facts, without introducing a new persistence framework.

#### L593 macOS helper activation has blocking reads without cancellation

**Current code:** `peritus-sandbox-macos/src/exec_status.rs::prepare` actually sets a five-second read timeout in this baseline; observe reads to EOF; manifest/helper readers also block.

**Proposed minimum fix:** Use a retained, framed, cancellable status handshake without synthetic timeout, shared in behavior with L569. Preserve helper identity and distinguish verified exec from interrupted/indeterminate disappearance.

#### L599 macOS recovery stores classifications rather than a reconstructible session owner

**Current code:** `peritus-sandbox-macos/src/recovery.rs::MacosRecoveryRecord` stores identity, activated and cleanup classifications; session owns live handles separately and records PID/group without birth identity.

**Proposed minimum fix:** Extend retained command recovery with exact birth identity and missing phase/cleanup facts. Reconnect through the actual command owner, verifying each prepared resource before clean absence; PID/group equality alone cannot authorize adoption.

#### L600 macOS proxy cleanup failure becomes permanently nonretryable

**Current code:** `peritus-sandbox-macos/src/session/cleanup.rs::record_release` takes proxy ownership then sets permanent proxy_cleanup_failed on shutdown failure.

**Proposed minimum fix:** Retain shutdown/join custody and permit exact release retry. Preserve RetryCleanup and its cause through the adapter and retained recovery record.

#### L601 macOS observation limits describe different collections

**Current code:** `peritus-sandbox-macos/src/session.rs::new` silently clamps observations; rich control mapping and legal lifecycle events use different collections.

**Proposed minimum fix:** Remove silent clamping and distinguish diagnostic retention from authoritative lifecycle evidence. Preflight observation admission before phase mutation; there is no demonstrated five-event lifecycle exhaustion requiring another mechanism.

#### L602 macOS activation removes status ownership before fallible verification

**Current code:** `peritus-sandbox-macos/src/session/lifecycle.rs::record_activation` releases the protected status writer before a fallible read, then mutates phase before recovery/observation writes; the reader itself remains a field in this baseline.

**Proposed minimum fix:** Retain the reader and stage activation/recovery/observation together, treating post-exec errors as recoverable activation. Close the parent writer when required to observe exec, but do not lose the prepared owner or force a new launch.

### Windows native owner, worker and cleanup recovery

Keep Job, ACL, proxy and console custody under the command's actual owner. Persist exact reversal and lifecycle facts through existing recovery, without disabling containment or creating another supervisor.

#### L608 Windows ACL rollback ownership is not durable across owner loss

**Current code:** `peritus-sandbox-windows/src/filesystem/acl/reversal.rs::install` retains ACL reversals only in memory, uses plan-digest/index backup names and ignores rollback failures.

**Proposed minimum fix:** Retain exact ACL preimages/reversal stages before mutation with unique attempt paths. Use owned cancellable ACL tooling and retain restore/cleanup errors until the same operation settles.

#### L610 Windows kill-on-close jobs tie target lifetime to the helper owner

**Current code:** `peritus-sandbox-windows/src/native/job.rs::OwnedJob::create` creates an unnamed kill-on-close Job and always installs CPU time; target lifetime follows helper handle custody.

**Proposed minimum fix:** Keep kill-on-close containment under the durable command owner and give recovery an inspectable exact Job identity. Transfer custody safely; make CPU enforcement optional with L556 while preserving real Windows integer limits.

#### L611 Windows ConPTY control workers have detached and blocking ownership

**Current code:** `peritus-sandbox-windows/src/native/handle.rs::start_io` drops both worker JoinHandles; relay writes hold the mutex also needed for cancellation and EOF.

**Proposed minimum fix:** Retain/join workers and their errors, interrupt blocked I/O independently of the input mutex and validate signed ConPTY dimensions before launch. Retain console custody until workers exit.

#### L613 Windows normal termination produces recovery bytes its decoder rejects

**Current code:** `peritus-sandbox-windows/src/session.rs::terminated` writes only helper_reaped=true; `recovery.rs::decode` rejects any partial cleanup despite advance accepting it.

**Proposed minimum fix:** Use the writer's monotonic per-dimension cleanup invariants in decode, requiring all complete only for Released. Permit normal terminated-but-unreleased records to reopen.

#### L614 Windows recovery cannot reconstruct or identify a live native owner

**Current code:** `peritus-sandbox-windows/src/recovery.rs::classify` trusts caller-supplied digest identity; native Job is unnamed and live handles remain in session memory.

**Proposed minimum fix:** Combine L610's inspectable Job with exact PID/birth, phase and cleanup facts retained by the command owner. Verify native custody before adoption; digest equality or supplied RecoveryProbe alone is insufficient.

#### L615 Windows proxy cleanup has a permanent retry-required dead end

**Current code:** `peritus-sandbox-windows/src/session/teardown.rs::release_proxy` consumes proxy and permanently rejects RetryRequired; release_owned_resources discards individual cleanup errors.

**Proposed minimum fix:** Retain proxy shutdown/join custody for exact retry/reconciliation and preserve each cleanup dimension and typed cause. Reuse existing ACL/filter release progress instead of a new cleanup framework.

#### L616 Windows managed networking adds a five-second native transaction wait and ephemeral filter ownership

**Current code:** `peritus-sandbox-windows/src/native/wfp.rs` opens a dynamic session, then rejects non-IPv4 routes after allocation; its native transaction wait setting does not establish a live run deadline.

**Proposed minimum fix:** Preflight supported route family before allocation or use an implemented matching route. Retain package/filter identity under command custody. Do not describe the unused five-second transaction setting as a run timeout.

#### L617 Windows session observations and release predicates assume unmeasured native facts

**Current code:** `peritus-sandbox-windows/src/session/teardown.rs` hardcodes helper_reaped=true and masks cleanup errors; probe advertises supervisor dimensions while `peritus-process/src/native.rs::poll_resources` defaults to Continue.

**Proposed minimum fix:** Publish only measured native facts. Implement required selected measurements or reject those capabilities before launch, and verify Job/process quiescence plus file absence before release.

#### L618 Windows helper narrows target status and hides post-activation failure causes

**Current code:** `peritus-sandbox-windows/src/native/launch.rs` clamps u32 target exit to i32::MAX; helper_process.rs narrows to u8 ExitCode and maps post-activation errors to TargetCreate.

**Proposed minimum fix:** Send exact u32 target termination and structured post-activation failures through the authenticated status channel/journal. Keep helper process exit separate; phase-aware reserved-code classification cannot restore discarded status.

### Projection continuation, activation and reasoned rebuild

Reuse verified derived checkpoints, catch up a journal suffix and atomically activate only a current candidate. Rebuild corrupt derived state without changing authoritative history.

#### L627 Any journal advance forces an in-memory genesis rebuild rather than incremental continuation

**Current code:** `peritus-projection/src/catalog.rs::plan_repair` returns RebuildFromGenesis on any position advance; `replay.rs::replay_from_genesis` always starts empty.

**Proposed minimum fix:** Decode the existing typed checkpoint and replay its verified suffix, retaining the incremental derived state through interruption. Keep authoritative run/context recovery with its actual owner.

#### L628 Projection installation compares generation ownership but does not check that its candidate still represents the current journal

**Current code:** `peritus-projection/src/sqlite/swap.rs::install_shadow` compares active generation and payload/count but not the candidate against a live journal head.

**Proposed minimum fix:** Check head/position at activation and catch up the suffix before swapping. Keep the prior valid generation until success; retire only unreferenced derived generations.

#### L629 Malformed projection metadata bypasses the advertised reasoned rebuild plan

**Current code:** `peritus-projection/src/sqlite/store.rs::plan_startup` propagates parse_generation failure before plan_repair can produce a reasoned rebuild.

**Proposed minimum fix:** Route malformed derived metadata into the existing reasoned rebuild result with diagnostics. Verify invariant digest/record count alongside payload integrity; preserve the authoritative journal.

#### L630 Global projection replay rejects a changed revision or unsupported historic family before an unrelated fold can proceed

**Current code:** `peritus-projection/src/replay.rs` globally rejects unknown families/schemas and any aggregate revision change before the projection fold.

**Proposed minimum fix:** Align replay with admitted historical schemas and explicit revision successors. Decode relevant history compatibly and isolate unsupported unrelated families to their affected projection; retain corruption detection.

### Quality output evidence and recoverable completion

Use complete retained output as the evidence source, keep ownership through publication and derive terminal classification once for both writer and decoder.

#### L649 Hidden quality rendering truncation and progress caps can turn successful work into incomplete evidence

**Current code:** `peritus-tools-quality/src/render.rs::output` escapes the whole output then calls text(), which silently truncates again at 16 KiB; active/terminal paths track finite progress.

**Proposed minimum fix:** Render only the requested encoded window and report every truncation. Informational progress truncation must not invalidate success when complete output artifacts and predicates establish it.

#### L650 Quality parsing depends on volatile event delivery rather than complete published artifacts

**Current code:** `peritus-tools-quality/src/execution/active.rs::finalize` parses only volatile parser_output with sequence/overflow flags even after wait_and_publish returns output artifacts.

**Proposed minimum fix:** On event gaps/overflow, rebuild parser evidence from the complete retained artifact. Restore the exact quality owner/cursor from operation evidence; still reject genuinely incomplete output.

#### L651 Quality completion consumes ownership before fallible publication and envelope assembly

**Current code:** `peritus-tools-quality/src/dispatcher/run.rs::start` takes plan/backend/artifacts before launch; execution/active.rs::finalize takes owner before fallible publication/terminal assembly.

**Proposed minimum fix:** Keep recoverable ownership through launch/publication and retain terminal facts before assembly. Apply shared referenced-artifact accounting so result metadata cannot invalidate a completed check or require rerun.

#### L652 Quality timeout/cancel/recovery classification can contradict its own decoder and remove recovery

**Current code:** `peritus-tools-quality/src/execution/terminal.rs` classifies candidate and failure separately; `result/contract.rs` only accepts specific outcome/status combinations.

**Proposed minimum fix:** Use one outcome decision for candidate, envelope and decoder. Preserve cancellation/timeout/indeterminate infrastructure facts with parsing detail and the original backend-selection recovery route.

#### L743 Public completion/tool rendering has bounded display loss without a complete-output continuation

**Current code:** `peritus-daemon/src/product_run/interaction/{presentation,tool_activity}.rs` truncates display and uses one pending_tool Option; finished takes it before checking matching name.

**Proposed minimum fix:** Expose explicit truncation/full trace-artifact retrieval and key pending notices by invocation identity. Never consume another call's record on mismatch; elapsed progress counters are observational.

### Product state identity and recoverable generations

Keep real schema/identity contracts and existing generation persistence. Repair corrupt-generation allocation and atomic file publication without introducing runtime session machinery here.

#### L659 Product-state shape gates are strict schema/identity rules rather than session timeouts

**Current code:** `peritus-product-state/src/{state,provider,phase,verified}.rs` validates schema, route and setup shape; this JSON state is configuration, not a live runtime owner.

**Proposed minimum fix:** Keep schema/route identity and explicit failover consent. Remove arbitrary field-width gates where needed and migrate actually supported older schemas; no invented session persistence repair belongs here.

#### L666 Product-state fallback can leave the next generation permanently occupied by corruption

**Current code:** `peritus-launcher/src/persistence.rs::load_latest` falls back to older valid generations; replacement truncates mutable recovery files directly.

**Proposed minimum fix:** Choose the next generation above every observed filename, including invalid generations, or explicitly quarantine conflict. Atomically replace mutable recovery files so fallback can save without colliding with corrupt N+1.

### CLI event and terminal observer continuation

Retain the same session, process and observer cursor across disconnect; keep output backpressure separate from control liveness.

#### L673 CLI event watch exits on disconnect or history gaps and does not durably resume delivery

**Current code:** `peritus-cli/src/events.rs::stream_events` returns on disconnect/history gap; delivery dedup state is local and output is synchronous.

**Proposed minimum fix:** Reconnect with original session and retained cursor, using snapshot recovery on a gap. Move output writes off the control loop and preserve exact operation identities/typed recovery fields.

#### L675 CLI terminal stdin buffers until EOF and terminal following has no reconnect state

**Current code:** `peritus-cli/src/terminal.rs::read_standard_input` reads to EOF before sending; follow creates local state and one sanitizer for all streams.

**Proposed minimum fix:** Stream stdin in negotiated chunks before EOF while control remains responsive. Retain exact observer positions for same-process reconnect and separate incremental UTF-8/sanitizer state per stream.

### Artifact transfer restoration and atomic local publication

Retain exact offset/ordinal/content identity in existing transfer ownership. Resume uncertain chunks by reconciling accepted progress; protocol completion and storage publication remain distinct.

#### L674 CLI artifact transfers lose resumable progress and output replacement has a destructive failure gap

**Current code:** `peritus-cli/src/artifact.rs` creates fresh transfer/local state and removes partial files on failure; upload_chunks begins offset/ordinal zero and destination replacement has a remove/rename gap.

**Proposed minimum fix:** Keep transfer/temp-file progress and verified source identity through reconnect. Atomically replace or no-replace destinations with directory sync; retain cancellation/abandon state instead of deleting partial data indiscriminately.

#### L687 Artifact transfer protocol conserves bytes but has no resumable interrupted phase or restoration API

**Current code:** `peritus-app-protocol/src/artifact/state.rs::ArtifactTransferState` enforces exact Receiving offsets/ordinals but exposes no checked progress restore constructor.

**Proposed minimum fix:** Restore from verified retained offset, ordinal, digest and publication facts. Preserve byte conservation/exact replay, distinguishing protocol Completed from storage finalization.

#### L799 — Attachment upload offsets are acknowledgement-driven but discarded on reconnect

**Current code:** `peritus-tui/src/model/chat/workbench/images/upload.rs` allocates fresh transfer IDs and retains acknowledged offsets in live Upload; import follows the same pattern.

**Proposed minimum fix:** Save transfer ID/source digest/reference/acknowledged offset in client state. Query exact accepted progress on reconnect, resume or explicitly cancel before replacement, retaining chunk/ack checks and independent controls.

### Application negotiation and observer restoration

Expose verified restored owner/session positions explicitly, retaining exact identity and at-least-once replay semantics rather than treating reconnect as new work.

#### L694 Closed application envelopes lack explicit artifact failure and stream-offset restoration messages

**Current code:** `peritus-app-protocol/src/envelope/{event,request,control,response}.rs` exposes artifact chunks/completion and TerminalUnavailable, but lacks transfer failure/status and saved-position reconnect messages.

**Proposed minimum fix:** Negotiate exact transfer/terminal status with verified offset/cursor plus typed failure/cancellation. Reuse existing owner state, integrity and snapshot recovery.

#### L697 Pure terminal attachment state has no saved-position restoration or replay admission

**Current code:** `peritus-app-protocol/src/terminal/state.rs` requires exact sequence/offset but only constructs initial observer state; terminal messages permit broader signed signal values.

**Proposed minimum fix:** Add checked restoration from saved observer position and authoritative disposition, defining exact replay. Preserve terminated attachment identity and reject nonpositive signal values.

#### L698 Negotiation carries requested session identity but does not verify that selection restores it

**Current code:** `peritus-app-protocol/src/version/negotiation/selection.rs::select` returns established_session without comparing ClientHello's requested session.

**Proposed minimum fix:** Compare against the authoritative restored session and explicitly reject mismatch. Keep provider-context recovery in its actual owner; negotiation alone needs no new session subsystem.

### Application error causes and safe recovery advice

Carry typed recovery facts end to end and resolve uncertain effects through the original receipt/owner before replacement or replay.

#### L695 Default protocol recovery advice does not establish safe restoration of the original operation

**Current code:** `peritus-app-protocol/src/error/code.rs::default_retry` maps broad state/order/cancellation errors to NewRequest and other failures to generic reconnect/recovery.

**Proposed minimum fix:** Use the actual typed receipt/owner-reconciliation route. Resolve uncertainty before replay/replacement and direct capacity recovery to the existing durable receipt store.

#### L696 Error display paths preserve different amounts of recovery information

**Current code:** `peritus-app-protocol/src/error/mod.rs::actionable_message` includes code/subsystem/retry but conversions/display paths retain different detail.

**Proposed minimum fix:** Preserve structured retry/subsystem/cause through CLI/local conversion, keeping bounded readable diagnostics and exact nonzero identity checks; a shaped ID is not restored custody.

#### L741 Stored-schema and provider failures can advertise recovery actions that do not address their cause

**Current code:** `peritus-daemon/src/product_run/error.rs::{persistence,invalid_data}` gives generic restart/new-request advice for stored state and conversion failures.

**Proposed minimum fix:** Return actual stored-schema migration or provider-selection recovery, resolving the same retained operation before new work. Preserve full causes through retrievable diagnostics.

### Workbench continuation uses the original operation receipt

Use the existing command identity/revision path for continuation and recover acceptance after disconnect.

#### L716 Continuation lacks the durable operation identity carried by ordinary workbench commands

**Current code:** `peritus-app-protocol/src/wire/workbench.rs::write_command` carries operation/query/revision, but standalone continuation encoding has no equivalent durable operation identity.

**Proposed minimum fix:** Route continuation through existing operation ID/receipt with expected revision and exact run/context binding. Resolve the accepted operation on reconnect instead of issuing fresh work.

### Daemon terminal custody, replay and completion receipts

Restore actual command custody before permitting reattachment; stream replay to each observer and keep owners/receipts until cleanup or durable completion is proved.

#### L725 Terminal restart recovery intrinsically refuses all reattachment

**Current code:** `peritus-daemon/src/terminal/recovery.rs::permits_attachment` always returns false, including LiveControlUnavailable; live registry owns transient controls.

**Proposed minimum fix:** Restore registry from the durable command owner and exact output positions, permitting attachment only after control/replay recovery with actor/session and birth checks. Flipping the boolean would not repair custody.

#### L726 A new terminal attachment must replay its whole history into a smaller pending queue

**Current code:** `peritus-daemon/src/terminal/bridge.rs::attach` rejects an evicted prefix and enqueues the entire replay before checking attachment health.

**Proposed minimum fix:** Stream replay as reader capacity becomes available, starting at an acknowledged available position. Retrieve older bytes from output artifacts; cache eviction cannot permanently deny attachment.

#### L727 Terminal observation can overflow an attachment before giving it a delivery opportunity

**Current code:** `peritus-daemon/src/terminal/bridge/observation.rs::observe` drains multiple process pages before delivery; attachments fault on pending queue overflow.

**Proposed minimum fix:** Bound each drain to downstream capacity or spool through existing output storage. Preserve missed ranges/per-reader recovery and prevent one burst or reader fault from affecting producer/siblings.

#### L728 Terminal completion receipts expire by volatile FIFO capacity and original session binding

**Current code:** `peritus-daemon/src/terminal/registry/receipts.rs::remember` evicts exact detach/cancel facts from a volatile FIFO bound to actor/session.

**Proposed minimum fix:** Retain completion in existing durable operation receipts and reconstruct the scoped cache on reopen. Cache eviction cannot be the sole retry authority; preserve original owner binding.

#### L729 Terminal owners are removed before join succeeds and unrelated reap failures gate new work

**Current code:** `peritus-daemon/src/terminal/registry.rs::retire` and registry/retirement.rs remove owners before join_owner succeeds; reap errors propagate into new admission.

**Proposed minimum fix:** Keep each owner through successful join/cleanup and retain failed owners for retry. Isolate unrelated reaping failures from new registrations and retain all shutdown outcomes.

### Recovery persistence and compensation failures

Record both original failure and incomplete compensation against the existing operation; do not acknowledge recovery that exists only in memory.

#### L730 Recovery and upload compensation paths suppress persistence or cleanup failures

**Current code:** `peritus-daemon/src/session/request/media.rs` ignores cancel_artifact_transfer errors; product_run/execution/{baseline,handoff}.rs suppresses some recovery/projection failures.

**Proposed minimum fix:** Retain original and compensation errors together in operation receipts, including pending transfer cleanup and persistence retry. Acknowledge only durable recovery facts.

### Deliverable operations, exact commit recovery and export identity

Run Git and file work outside global run locks under retained operation custody; reconcile each repository's exact commit and immutable export before retry.

#### L744 Deliverable controls hold the global run lock through unowned synchronous Git and file operations

**Current code:** `peritus-daemon/src/product_run/deliverable.rs::control_deliverable` holds records.write through synchronous Git/hashing; commit.rs calls hooks/signers without owned process continuation.

**Proposed minimum fix:** Use durable command execution for Git/hooks/signers and hash outside the records lock. Retain a fenced operation result before snapshot publication, supporting cancellation and exact reconciliation.

#### L745 Commit recovery retains source proof but not an exact native commit-operation receipt

**Current code:** `peritus-daemon/src/product_run/deliverable/commit/attempt.rs::Attempt` stores workspace/path/source/patch hashes only and rejects sidecars over 65536 bytes.

**Proposed minimum fix:** Extend this sidecar with expected parent/tree and resolved commit IDs per repository. Reconcile them before another commit, preserving saved patch/source proof and removing the arbitrary sidecar quota.

#### L746 Export retries treat an existing path as an immutable result without verifying its bytes

**Current code:** `peritus-daemon/src/product_run/deliverable.rs::export_available` checks only nonempty path/is_file; duplicate Export accepts that result.

**Proposed minimum fix:** Retain and verify export length/digest in the deliverable receipt, publish atomically and resolve saved export independently of mutable workspace admission.

#### L747 Pending discard intentionally reserves the workspace but malformed old state blocks new admission

**Current code:** `peritus-daemon/src/product_run/deliverable/discard/reservation.rs` binds/locks pending discard and rejects malformed or foreign sidecars, blocking other mutable admission.

**Proposed minimum fix:** Keep reservations for genuinely unresolved effects. Expose exact pending facts and reconcile known legacy/malformed sidecars with evidence preserved; publish verified completion before releasing custody.

### Review inspection and retained feedback handoff

Inspect immutable candidates without execution eligibility; accepted feedback retains a recoverable execution handoff under its original command identity.

#### L752 Review pages recompute the complete candidate and require execution eligibility

**Current code:** `peritus-daemon/src/product_run/workbench/review.rs::{current_page,current_targets}` recomputes full live candidate and requires may_start_execution even to inspect.

**Proposed minimum fix:** Read retained immutable candidate/diff independently and page content. Hash live state outside shared locks; require current anchors only for mutating feedback or starting work.

#### L753 Accepted review feedback can lose its execution handoff and exact retry does not restore it

**Current code:** `peritus-daemon/src/product_run/workbench.rs` returns duplicate review receipts before resume_feedback and calls handoff only for fresh acceptance.

**Proposed minimum fix:** Retry accepted feedback handoff on fresh and duplicate receipt paths. Persist or roll back the run transition before spawning, retain cumulative progress and use the original command identity.

### Improvement backfill, migration and later attempts

Advance retained improvement evidence incrementally and preserve exact migration/attempt identity; explicitly authorized later evaluation must not be replaced by the first historical run.

#### L755 Improvement backfill is whole-history work and its evaluation prompt still prescribes a bounded budget

**Current code:** `peritus-daemon/src/product_run/improvements.rs::collect_improvements` scans all runs for each visit; store/evaluation retain whole histories and a bounded-budget prompt.

**Proposed minimum fix:** Use incremental backfill and paged inbox with full evidence references; retry SQLite contention cancellably. Delete the obsolete normal-bounded-run-budget instruction, keeping evaluation explicitly authorized.

#### L756 Improvement schema handling quarantines old state without a resumable migration transaction

**Current code:** `peritus-daemon/src/product_run/improvements/store.rs::quarantine_pre_release` moves SQLite sidecars then database without retained move intent.

**Proposed minimum fix:** Retain a small exact move-intent record and reconcile it before successor open. Recover known legacy suggestions explicitly and preserve unknown formats rather than treat them as empty authority.

#### L757 A suggestion has only one durable evaluation reservation across all future attempts

**Current code:** `peritus-daemon/src/product_run/improvements/store.rs` freezes evidence once evaluation exists and reserves only one evaluation per candidate.

**Proposed minimum fix:** Key separately authorized later attempts to current source/harness revision, retaining prior evaluation receipt/frozen evidence for exact retry. Do not silently return an old run for a new request.

### TUI exact unresolved intent and truthful admission

Persist the existing client intent fields before send and retain them until terminal settlement. Distinguish accepted receipts from confirmed execution.

#### L786 — Selection overlays consume control keys, and model updates clear intent before send success

**Current code:** `peritus-tui/src/model/chat/navigation.rs::OutputMode::handle_key` consumes all Selecting keys; models.rs::save_model_choice clears command/pickers before request success.

**Proposed minimum fix:** Route quit/cancel before selection handling and expose selection exit. Retain model/effort command, picker and unacknowledged selection until send/receipt success in saved client state.

#### L788 — Exact control recovery exists only in the retained UI model, and some messages overstate admission

**Current code:** `peritus-tui/src/model/chat/workbench/receipts/recovery.rs` already distinguishes recovered Resume admission from execution, but unresolved commands/drafts live in the UI model only.

**Proposed minimum fix:** Save exact unresolved command/draft in existing client state before send and restore after restart. Reconcile accepted launch and preserve the existing accepted-versus-started distinction plus actual recovery detail.

#### L797 — Restore recovery receipts release the UI's exact-operation retry record

**Current code:** `peritus-tui/src/model/chat/workbench/checkpoints/receipts.rs::accept_workbench_restore` calls clear_checkpoint_command for RecoveryRequired as well as terminal outcomes.

**Proposed minimum fix:** Retain original restore command plus recovery receipt until settlement, across restart, and expose the exact server reconciliation action. Clear only on terminal outcome; preserve preimage/checkpoint/permission checks.

### Web workspace state and native daemon identity

Preserve durable workspace lineage and original operation binding across schema failures/reconnect; choose the same exact store/session rather than a filename-selected replacement.

#### L809 — Web state schema failure becomes a fresh workspace, and receipt replacement lacks directory durability

**Current code:** `peritus-web/src/state.rs::App::open` quarantines parse failures then creates Workspace::default; save syncs temporary file but not parent, update serializes under mutex and nest lacks visited-set validation.

**Proposed minimum fix:** Preserve original and return recoverable migration error instead of fresh state. Add explicit supported migration, parent sync and cycle validation; serialize captured state outside mutex and publish only matching revision.

#### L810 — Web daemon discovery and request sessions do not preserve a native owner binding

**Current code:** `peritus-web/src/daemon.rs::latest` selects lexicographically latest config independently; raw_request and receipts::retransmit connect with None session and a 30-second duration.

**Proposed minimum fix:** Bind configuration/product state and prepared operations to the same explicit store/workspace/session. Reuse session and actual owner run/provider context on reconnect; remove default request deadline via shared client repair. Metadata reopening alone does not restore native context.

## 7. Responsiveness, cancellation, and history scaling

### Keep unrelated runs responsive during control and persistence work

Keep a single serialized SQLite writer and serialized effects for each run. Limit the global records lock to finding/publishing a run owner, perform blocking work off the async executor, and preserve per-run ordering so a late save cannot overwrite a newer state. Combine this with the verified-replay cache rather than introducing a new orchestration framework.

#### L023 — Synchronous global control ownership and run-state publication barriers

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench.rs` — `with_controls`, `workbench_query`, synchronous ordinary `workbench_command`; `product_run/execution.rs` — `observe`, `finish`, `deliver_public_reply`.

**Proposed minimum fix:** Move synchronous store calls off async request tasks and use the cache for reads. Retain per-run serialization while releasing the global registry before checkpoint sealing, goal settlement, and filesystem writes. Preserve failed publication details and retry that publication instead of silently returning forever once phase becomes `RecoveryRequired`.

#### L742 Public interaction observation writes whole run records and hides pending-input read failures

**Current code:** `crates/app/peritus-daemon/src/product_run/interaction/inputs.rs` — `append_control_inputs`, `synchronize_public_inputs`; `interaction/models.rs` — `update_models`; `interaction.rs` — `pending_interactive_input`, `query_interaction`, `terminal_activity`.

**Proposed minimum fix:** The input projection already skips `public_input_count`; retain that cursor and remove the redundant full replay through L021. Save under the same per-run owner outside the global registry lock, then publish the matching revision. Return pending-input inspection errors instead of `false`, and surface terminal activity append failures. Keep model-update rollback on failed persistence.

### Keep cancellation responsive while command input is blocked

Bound per-tick queue work and move blocking input I/O out of the supervisor's control/output loop.

#### L092 — Bounded control admission and synchronous stdin can obstruct cancellation and observation

**Current code:** `crates/runtime/peritus-process/src/control.rs::try_send` refuses every full-queue control, including cancel. `supervisor/io.rs::{drain_controls,write_input,drain_output}` drains until empty and synchronously writes/flushes stdin; cumulative overflow fails the owner.

**Proposed minimum fix:** Give cancellation an independent flag/reserved path, use an owned input writer, and limit each drain's work before yielding to other control/output. Remove default cumulative stdin policy with L087; reject an explicitly over-limit write at admission rather than crash the owner.

### Managed Git capture and restore need owned, cancellable operations

Give the existing Git helper operations observable ownership and cancellation. Keep restore preimages, Git locks and exact conflict checks until settlement; do not replace missing cancellation with another elapsed deadline.

#### L144 — Managed capture requires a complete representable Git snapshot and waits synchronously for helper processes

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed.rs::{repository_fingerprint,changed_paths,patch,git}` recaptures and synchronously waits; `managed/capture.rs::capture_with_baseline` converts raw Git paths to UTF-8, buffers outputs and recursively captures nested repositories.

**Proposed minimum fix:** Route helpers through an owned cancellable command adapter and reuse verified unchanged snapshots. Carry native Unix path bytes through baseline keys, Git operands and persisted representation, including compatible decoding of existing strings. Keep complete-snapshot, mode/object and nested-repository integrity checks; 128-path chunks are batching, not a quota.

#### L145 — Managed restore refuses unowned structural conflicts instead of overwriting them

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/paths.rs::{validate_restore_paths,validate_directory}` refuses unowned structural replacements; `git_path.rs::tree_name` requires UTF-8.

**Proposed minimum fix:** Keep the ownership, merge-stage and foreign-file guards. Return the exact conflicting paths so the user can resolve them and retry the same restore; support native Unix path bytes as in L144. Do not overwrite unrelated files.

#### L146 — Discard holds Git locks across restoration and can wait indefinitely without a control channel

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/head/prepared.rs::Transaction::{exchange,publish,drop}` synchronously reads/waits while Git locks are held; error stderr is clipped to 8,192 bytes.

**Proposed minimum fix:** Make this existing transaction an explicitly owned cancellable helper; abort and reap through its owner rather than blocking Drop. Retain locks until confirmed settlement, preserve recovery assets and full diagnostics with marked previews. Keep exact HEAD checks. Cross-filesystem restore needs a verified staged-copy/publication path before rename can be replaced; do not silently weaken atomicity.

#### L147 — Retained repository recovery depends on exact local metadata and has no migration or cancellation here

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/retention.rs::{capture_nested,validate_object}`, `retention/linked.rs::initialize` and `retention/restore.rs::{prepare_nested,stage}` require exact retained objects and linked administrative metadata.

**Proposed minimum fix:** Keep Git object-format and exact retained-metadata checks. Add cancellation and progress to the existing synchronous recovery path, and migrate only an actually supported historical manifest. A deleted linked database cannot be reconstructed without a backup; report that requirement rather than inventing a substitute identity.

#### L150 — Discard restart accepts only recorded before/after states and rechecks the entire workspace

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/transaction/validation.rs::{structure,progress}` and `validation/{paths,directories}.rs` validate recorded identities, before/after states and archive contents.

**Proposed minimum fix:** Keep before/after identity and foreign-edit validation. Make existing workspace validation cancellable and reuse unchanged captured facts where safe; expose the recorded conflicting paths for explicit resolution instead of weakening restore integrity.

### Separate IPC control progress from outbound backpressure

Own reads, heartbeat replies and outbound frame progress independently of slow application work. Socket loss must leave durable sessions, runs and receipts recoverable.

#### L236 — Application heartbeat has a fixed three-miss cutoff and rejects late superseded replies

**Current code:** `crates/app/peritus-daemon/src/session/heartbeat.rs` has a three-miss cutoff and replaces the one pending nonce on each send; `session/connection.rs` sends every ten seconds and awaits handlers/writes serially.

**Proposed minimum fix:** Remove the missed-heartbeat disconnect count. Keep one outstanding heartbeat until its valid reply arrives, so a delayed reply is not invalidated by a replacement nonce. Service reads/control independently of slow requests and writes; retain durable session/run identity through transport replacement.

#### L237 — Connection capacity can be occupied by uncancellable hello reads or stalled serial writes

**Current code:** `crates/app/peritus-daemon/src/ipc/server.rs` holds connection permits until connection exit. `session/connection.rs` awaits hello/setup/serial writes outside stop-aware waits. `ipc/frame.rs` retains partial read state, but not partial write state.

**Proposed minimum fix:** Make setup and frame delivery explicitly cancellable. Retain a write owner with frame/offset state or discard the interrupted socket, never restart a partial frame on it. Split inbound control from blocked delivery so stop can release connection permits without adding a synthetic hello/write deadline.

#### L238 — Shutdown acknowledgement precedes queue admission, and reporting delivery can block

**Current code:** `crates/app/peritus-daemon/src/session/request.rs` writes `ShutdownAccepted` before `shutdown.try_send`. `session.rs` uses an eight-event reporting channel whose send is awaited; `session/connection.rs` relays reports through serial socket writes.

**Proposed minimum fix:** Admit the command before acknowledging acceptance. Retain the shutdown owner's latest/completed result independently of a reporting subscriber and make notifications nonblocking with respect to teardown. Expose retained completion after reconnect; do not claim an existing durable shutdown receipt where this channel currently supplies none.

### Keep terminal control and scroll independent of output availability

Output projection loss must not imply process completion or remove exact authorized cancellation.

#### L252 — Terminal output loss disables input and cancellation even when the process may be running

**Current code:** `crates/app/peritus-tui/src/terminal.rs::can_capture` requires output availability; model/interaction/terminal.rs gates cancellation on it. accept_output resets scroll to zero.

**Proposed minimum fix:** Add a separate exact-process control predicate that permits authorized cancellation/recovery even when output is unavailable. Follow new output only when already at the bottom; preserve user scroll and expose retained command-output retrieval where available, with an explicit unavailable state otherwise. Keep display scrollback independent of process lifetime.

#### L263 — Preview rendering silently adds another scrollback cut; dashboard conversation shows only 12 activities

**Current code:** `crates/app/peritus-tui/src/render/product/preview.rs::append_output_lines` calls terminal::preview_lines but reports omission only from the host flag. render/product/detail.rs::conversation_text selects twelve activities.

**Proposed minimum fix:** Report emulator-side eviction as well as host truncation and expose retained full output where available, without claiming the host kept bytes it discarded. Add dashboard conversation scrolling or navigation to the dedicated conversation view. Keep display window sizes as presentation choices, not run limits.

### Keep authority controls responsive while retaining one mutation owner

Separate read/control service from blocking storage/replay; preserve serialized mutations and exact receipts.

#### L274 — Status, cancellation, and shutdown wait behind synchronous work in one authority FIFO

**Current code:** `crates/app/peritus-daemon/src/authority/owner.rs` puts all messages in one bounded Tokio queue. owner/handle.rs::send awaits space/reply; owner/runtime.rs executes synchronous journal and artifact operations before receiving the next message.

**Proposed minimum fix:** Give status, stop and cancellation independent admission/observation; execute blocking storage/replay on an owned blocking worker and make long operations cooperatively interruptible at safe boundaries. Keep one serialized mutation owner. Preserve accepted command identity when a receiver disappears and expose durable outstanding prompt enumeration from L259.

#### L275 — Installed native child handlers reject supported start and cancellation work for missing orchestration inputs

**Current code:** `crates/app/peritus-daemon/src/authority/owner/orchestrator/children/collaboration.rs` rejects StartWriter/StartReview/StartFixer for missing work/reservation inputs; scheduler.rs rejects CancelChildren for absent work root.

**Proposed minimum fix:** Admit these kinds only when the owning producer supplies or durably references the existing native work/reservation/root inputs. Route to the existing native helpers under exact child heads; otherwise report unsupported capability before durable publication. Scope already queued failures through L265; do not introduce another scheduler.

### Make scheduler delivery and dependency work progress without starvation

Keep exact dispatch identities and truthful resource accounting. Use continuation and affected-node work rather than elapsed cutoffs or complete-history rescans.

#### L285 — Scheduler delivery repeatedly selects the same bounded prefix and has no separate cancellation lane

**Current code:** `crates/orchestration/peritus-scheduler/src/runtime.rs::pending_directives` always scans reservations from index zero and returns the same bounded dispatch/cancel prefix.

**Proposed minimum fix:** Add a dispatch-ID continuation/rotation cursor to polling and a separately serviced cancellation batch. Keep the per-poll batch bound and redeliver the same outstanding identities until acknowledged; no acknowledgment deadline is needed.

#### L292 — Bypass aging is a priority class, not a guarantee of dispatch at the configured bypass count

**Current code:** `crates/orchestration/peritus-scheduler/src/selection/scan/work.rs::candidate_precedes` and selection/model.rs rank priority before enqueue order even when both candidates are aged; selection scans all work and first feasible worker.

**Proposed minimum fix:** Order aged feasible work by age/enqueue order before priority to provide eventual service, and rotate equivalent available workers with explicit deterministic cursor state. Reuse resource sums within each selection. Update versioned replay/spec ordering together rather than changing the meaning of old committed events.

#### L298 — Dependency refresh finishes synchronously by repeatedly rescanning all retained work

**Current code:** `crates/orchestration/peritus-scheduler/src/state/mutation.rs::propagate_dependencies` runs full work scans until its structural measure reaches a fixed point, inside each reducer transition.

**Proposed minimum fix:** Maintain an index of dependency edges derived from retained work and propagate only nodes affected by a changed dependency. Preserve the structural termination measure and complete fixed-point truth. Run expensive pure computation away from the shared authority control owner; do not delete its termination witness as though it were a work quota.

### Reuse verified E0 state without repeated complete-history work

Optimize the existing representation and owner boundary, retaining canonical evidence.

#### L311 — E0 validates, serializes, and replays growing complete history synchronously

**Current code:** `crates/orchestration/peritus-orchestrator/src/reducer.rs::decide` validates predecessor/successor and clones state; state/validation.rs uses pairwise history checks. canonical/digests.rs allocates canonical_state_bytes; durability.rs loads the full chain.

**Proposed minimum fix:** Reuse verified predecessor facts and keyed uniqueness indexes, stream the same canonical hash input, and extend verified replay/checkpoints by suffix. Remove redundant complete-state work while keeping final invariant admission. Move blocking persistence off the control executor with L274; no new history format is required.

### Keep B1 routine accounting proportional to the affected frontier

Reuse existing account/reservation identities and receipts; full-history validation belongs at reconstruction boundaries.

#### L348 B1 validates all retained accounting history on every transition and query

**Current code:** `transition.rs:89,149` fully validates input/output and refinement; `state.rs:216` copies all accounts and reservations. Prefix/action/lineage checks repeat scans over permanent history.

**Proposed minimum fix:** Index account relationships, reservation/action identities and exact receipts. Validate changed balances and affected ancestors on normal transitions, retaining explicit full verification for recovery. Replace executable recursive witness traversals only with equivalent checked invariants; do not delete accounting history.

### Replay E1 once from a verified frontier

Use the existing checkpoint and immutable revision objects; avoid duplicate reduction and repeated history reconstruction.

#### L364 E1 recovery repeatedly rebuilds complete history and re-encodes state

**Current code:** `durability/recovery.rs:83` validates decoded events through reduction then reduces again; HarnessReplay::rebuild replays them again. State hashing serializes complete retained history.

**Proposed minimum fix:** Decode/reduce each event once and expose that verified state. Restore a verified checkpoint plus contiguous suffix, reuse immutable graphs and stream hashing while preserving independent consistency checks and historical identity.

### Build acceptance topology with indexed adjacency

Keep deterministic gate ordering and the independent certificate, avoiding repeated scans and input-sized executable recursion.

#### L394 Acceptance graph construction and its executable certificate use cubic scans and input-sized recursion

**Current code:** `peritus-spec/src/gate.rs:308` repeatedly scans for the first eligible definition and linearly resolves dependencies; it then executes a full graph-order certificate.

**Proposed minimum fix:** Build adjacency and indegrees once and choose ready gates deterministically in the same order. Validate the resulting order iteratively against the same certificate relation, keeping every gate and evidence obligation.

### Keep B0 routine control independent of full retained history cost

Reuse checked immutable contracts and indexed identities while preserving history and exact transition integrity.

#### L403 B0 revalidates and clones the complete growing session history on every command

**Current code:** `peritus-kernel/src/reducer.rs:120,175,305` validates/clones the aggregate and checks full successor/refinement; aggregate lookups linearly scan growing vectors.

**Proposed minimum fix:** Index commands/events/parent relations and validate affected relations on checked state. Reuse immutable contract validation and avoid full-state copying per control command, preserving independent recovery validation and all history.

### Make E3 analysis and recovery proportional to owned work

Reuse verified campaign checkpoints and deterministic analysis inputs; preserve exact seeds and requested statistical work.

#### L438 E3 repeatedly copies and replays the complete campaign instead of resuming bounded work

**Current code:** `durability/replay.rs:76` checks and reapplies each event; state identity encoding includes complete rollout collections and normal transitions clone them.

**Proposed minimum fix:** Reduce each loaded event once and reuse verified checkpoint/suffix recovery. Iterate rollout state without complete temporary vectors, stream hashing and scope evidence export to its producers, preserving exact campaign state and control responsiveness.

#### L439 E3 bootstrap analysis performs every resample synchronously with no durable cursor or cancellation

**Current code:** `statistics/paired.rs:248` executes every replicate/draw synchronously, allocating each hash preimage, and only returns after sorting the complete result.

**Proposed minimum fix:** Run the deterministic loop in cancellable batches, retaining replicate cursor and partial results in the existing analysis work state (which currently lacks that cursor). Reuse preimage buffers and preserve profile/replicate/draw identity and requested replicate count.

### Avoid repeated F0 whole-state and whole-store work

Use existing immutable evidence and scoped journal queries while preserving exact attribution and ownership.

#### L455 F0 repeats whole-state and whole-store work synchronously during attribution, commit and recovery

**Current code:** `attribution/engine.rs:123` scans task/k observations repeatedly; `durability/replay.rs:79` checks and reapplies events; publication exports the whole store.

**Proposed minimum fix:** Index task/k observations once, reduce events once from a verified checkpoint/suffix and avoid duplicate full encoding. Scope publication provenance queries to required records and reuse immutable evidence, retaining exact attribution checks.

### Publish known HTTP rejection before optional body capture

Reuse typed status/acceptance facts and cancellable body evidence capture; scheduling failures must not erase the original cause.

#### L475 Compatible rejection recovery waits for the entire error body first

**Current code:** `peritus-provider-compatible/src/client.rs:196` drains the full rejected body before classifying headers or planning retry; `client/response.rs:10` collects it all under a fixed limit.

**Proposed minimum fix:** Classify rejection immediately from headers and stream/hash optional error evidence independently with cancellation. Preserve status if capture fails, and do not hold safe recovery for body EOF.

#### L488 Google HTTP recovery drains complete rejected bodies and can lose typed failures behind retry-policy errors

**Current code:** `peritus-provider-google/src/client.rs:280` drains full error bodies before deriving retry/acceptance facts and uses body-dependent classification with fixed retry policy.

**Proposed minimum fix:** Classify header-known rejection immediately and capture optional body facts incrementally. Preserve typed failure if scheduling/body capture fails, honor Retry-After through the shared policy and avoid complete request copies on each retry.

#### L498 OpenAI optional metadata and whole error bodies can block response classification

**Current code:** `peritus-provider-openai/src/client.rs:212` waits for full error body and optional metadata parsing; `stream/metadata.rs:173` drops unsupported/large Retry-After values.

**Proposed minimum fix:** Apply header-first typed rejection and streamed error evidence, isolate optional telemetry failures and honor valid backoff without arbitrary delay refusal. Retain accepted output when telemetry is unusable and preserve original acceptance/status if capture fails.

### Keep MCP cancellation and connection recovery live under backpressure

Reuse daemon authority and per-request ownership; selected concurrency/page sizes are operational bounds, not task lifetime limits.

#### L502 MCP finite admission and response backpressure can block cancellation notifications

**Current code:** `peritus-mcp/src/server.rs:189` awaits response queue sends in the reader and rejects saturated admission; only call/read/get methods in `bridge.rs:225` receive cancellation.

**Proposed minimum fix:** Keep notifications readable while response delivery is backpressured, pass cancellation through every bridge method and classify saturation as recoverable backpressure. Validate semaphore-native capacity; retain explicitly selected frame/page/concurrency policy.

#### L503 MCP task completion and disconnect handling lack active-session reconciliation

**Current code:** `server.rs:130` joins requests/writer only after read_requests ends; active IDs are removed only on normal request completion, and clean EOF does not cancel outstanding work.

**Proposed minimum fix:** Select task completion, writer failure and EOF during serving. Use owned cleanup guards for active IDs and cancel/reconcile admitted bridge work on disconnect through daemon authority; no new MCP persistence store is needed.

#### L504 MCP strict protocol and post-result serialization bounds can terminate a usable connection

**Current code:** `framing.rs:45` turns oversized serialized responses into writer failure; `server.rs:317` supports one exact protocol version, and list results are collected before paging.

**Proposed minimum fix:** Return per-request capacity errors or existing resource references without killing the connection. Page at the bridge source with stable snapshot binding and negotiate only implemented versions, preserving strict correctness-critical fields.

### Append trace observations from the current verified projection

Keep immutable history and exact replay without reconstructing it for every new event.

#### L521 Every new durable trace observation replays the complete aggregate history

**Current code:** `peritus-trace/src/storage.rs:102` reads/replays the full aggregate before every append; projection hashing encodes whole state.

**Proposed minimum fix:** Reuse a verified projection and apply only its journal suffix, staging changed spans for atomicity and streaming invariant hashing. Keep exact committed-command resolution and full recovery checks.

### Pure heartbeat and shutdown ordering

Keep protocol ordering separate from host policy; do not invent a clock limiter in DTOs that contain none.

#### L688 Heartbeat and shutdown contracts contain no clock-based liveness or six-step execution deadline

**Current code:** `peritus-app-protocol/src/daemon/heartbeat.rs::HeartbeatState` tracks sequence/nonces only; shutdown describes ordered stages rather than an elapsed six-step deadline.

**Proposed minimum fix:** No clock limiter to remove here. Keep correlation/framing and apply host cutoff/remaining-work corrections at L236/L199.

### Nonblocking diagnostics before mutation readiness

Authenticated diagnostic/status access remains available during startup/recovery. Optional logging and large human detail must not gate progress.

#### L731 Best-effort diagnostics can block and unbounded diagnostics can fail status projection

**Current code:** `peritus-daemon/src/diagnostic.rs::report` synchronously locks/writes stderr; lifecycle/error projection can embed unbounded Display detail.

**Proposed minimum fix:** Make best-effort writes nonblocking, retain bounded status previews plus full structured cause access and preserve recovery/source fields.

#### L759 Formal readiness predicates prohibit diagnostics while startup remains incomplete

**Current code:** `peritus-daemon/src/verified.rs` readiness predicates gate diagnostics together with incomplete startup.

**Proposed minimum fix:** Permit authenticated startup/recovery status before mutation readiness. Keep fail-closed mutation and phase ordering; no additional recovery subsystem is needed.

### Incremental conversation library and exact search pages

Use an existing derived conversation/reservation projection over immutable events, preserving stable pages and exact fork/receipt authority.

#### L735 Conversation creation rescans all immutable control history and every restore reservation

**Current code:** `peritus-daemon/src/product_control/storage/library.rs::conversation_ids` rescans journal from zero; reservations.rs loads every conversation/restore for child admission.

**Proposed minimum fix:** Advance the library/reservation projection from its verified cursor instead of global rescans. Isolate unrelated load failures and preserve atomic child lineage/receipt publication.

#### L739 Library pagination still performs full-history work and can construct an invalid exact snippet

**Current code:** `peritus-daemon/src/product_run/library.rs` loads/sorts whole governed history for each offset page; excerpt can exceed the protocol's 512-byte snippet maximum.

**Proposed minimum fix:** Use L735's incremental stable-fenced view and load relevant records only. Fit context around the full literal within the admitted snippet or expose source retrieval for longer literals; isolate unrelated damage.

### TUI independent reads and preserved destinations

Track observations per destination/panel and allow cancellation/supersession without suppressing unrelated progress. Keep accepted mutations serialized under their original identity.

#### L785 — Chat progress is gated by volatile pending lookups and a missing binding can detach the run

**Current code:** `peritus-tui/src/model/chat.rs::poll_chat` returns for pending binding/query; send_chat_message clears run_id when no destination is found.

**Proposed minimum fix:** Poll chat/goal independently, retain selected run and destination on lookup failure and retry binding without another Enter. Serialize only mutations for the same destination.

#### L789 — Removing request expiry left global pending-read gates without independent progress recovery

**Current code:** `peritus-tui/src/model/chat/workbench/navigation.rs` gates every refresh on workbench_request_pending, coupling unrelated panels and goal progress.

**Proposed minimum fix:** Use per-panel/query pending state with explicit cancel/supersession and late-response rejection. Retain accepted mutations under original receipts; elapsed request expiry is not the progress-recovery mechanism.

#### L794 — A stale compaction preview is refreshed with the same obsolete revision

**Current code:** `peritus-tui/src/model/chat/workbench/compaction.rs::refresh_compaction` resends the saved request including obsolete revision.

**Proposed minimum fix:** Refresh the conversation snapshot, rebuild preview from its current revision and require confirmation of the new preview while preserving draft. Do not resend the obsolete request unchanged.

#### L796 — Goal drafting is local and goal controls inherit the global read fence

**Current code:** `peritus-tui/src/model/chat/workbench/goal/{draft,refresh}.rs` retains draft/brief handoff locally; goal refresh inherits global pending-read gating.

**Proposed minimum fix:** Admit pause/resume independently of unrelated reads, refresh then send unchanged control for the same goal. Save draft/brief-to-goal intent until settlement; use shared larger-text admission and task-appropriate confirmed criteria.

### TUI local work cancellation and command custody

Keep UI event/control processing live during owned work, with explicit cancellation and exact cleanup before releasing resources.

#### L790 — Canceling an attachment panel does not cancel its capacity-one blocking file reader

**Current code:** `peritus-tui/src/runtime/{files,images}.rs` allows one spawn_blocking read; abandoning panel intent does not interrupt that reader.

**Proposed minimum fix:** Read in cancellable chunks and ignore late results by request ID. Release abandoned panel admission while separately accounting for any uninterruptible filesystem call instead of occupying the next request's only slot.

#### L791 — Foreground candidate execution suspends the whole UI reducer without a durable process owner

**Current code:** `peritus-tui/src/runtime/candidate.rs::execute` waits only for foreground task/interrupts while the screen is suspended and directly launches std::process::Command.

**Proposed minimum fix:** Pump daemon/control events while foreground command runs and use the owned command runtime for durable identity/cancel/reattachment. Preserve foreground group handling and candidate digest checks; add no deadline.

#### L792 — Clipboard writing has no default deadline but a hung helper holds its sole slot

**Current code:** Contrary to the historical heading, `peritus-tui/src/runtime/clipboard.rs::copy` supplies a two-second timeout to native_command; OSC52 writes synchronously.

**Proposed minimum fix:** Remove the synthetic helper timeout and add explicit cancellation that kills/closes/reaps before slot release. Move OSC52 writes off the UI executor and report actual helper/write outcome.

### TUI visible rendering and complete recovery access

Cache unchanged wrapping and slice logical rows before viewport conversion, keeping full source and recovery detail accessible.

#### L801 — TUI rendering processes whole retained content before taking visible rows

**Current code:** `peritus-tui/src/render/chat/transcript.rs::transcript_rows` formats every retained activity; render/editor.rs lays out full drafts then slices visible rows.

**Proposed minimum fix:** Cache unchanged wrapping and render visible rows plus small margin. Retrieve older transcript pages on demand while retaining full-source access and omission markers.

#### L802 — Some display bounds can hide recovery details or omit wide characters

**Current code:** `peritus-tui/src/render/prompts.rs::maximum_scroll` saturates to u16; transcript wrapping drops graphemes wider than width and status summaries constrain recovery detail.

**Proposed minimum fix:** Use usize logical scroll and slice before u16 viewport conversion. Preserve wide graphemes for copy/resize, make full recovery details inspectable and show only dispatchable actions; fix input exclusions through shared admission.

### Browser independent loading, polling and controls

Release core UI loading independently of optional observations and make each observation cancellable/supersedable. Keep original effect identity for explicit recovery.

#### L814 — Browser startup and polling can wait forever while recovery controls are hidden

**Current code:** `webui/src/lib/workspace.svelte.ts::start` awaits refresh/loadProject/poll/reconcile before loading=false; poll chains status/consoles/facts/recovery; Recovery.svelte hides controls while pending.

**Proposed minimum fix:** Release loading after core workspace state and update each observation independently. Keep recovery controls reachable during pending work and cancel/supersede reads without adding execution deadlines.

#### L817 — Activity ordering can discard authoritative state, and control replies can move between sessions

**Current code:** `webui/src/lib/conversations.ts::latestConversation` discards an entire incoming snapshot when activity tail is older; workspace dispatch reads ui.sessionId again after awaiting control.

**Proposed minimum fix:** Merge phase/legal controls using authoritative revision and order only activity by activity sequence. Capture destination before awaiting and apply reply solely to that session.

### Browser visible content and persistent editor positions

Bound actual render/fetch work to visible ranges while retaining complete source and draft/cursor history.

#### L820 — Frontend paging does not bound whole-content work or preserve all editor state

**Current code:** `webui/src/lib/components/{FileViewer,TextEditor,Markdown,PdfViewer}.svelte` processes full fetched text; files/text.ts indexes whole content with UTF-16 page boundaries; obsolete fetches are only ignored.

**Proposed minimum fix:** Virtualize activity/collections, cache unchanged Markdown and keep draft/cursor/undo outside mounted components. Fetch/index ranges incrementally, preserve Unicode boundaries and cancel obsolete fetches; keep mixed-newline protection and explicit large-edit confirmation.

## 8. Admission, capabilities, and correctness

### Checkpoint coverage for partial file selections and empty directories

Keep checkpoint read coverage separate from mutation authority. A partial selected workspace file can retain its whole backing-file preimage without granting extra write permission. Empty-directory restoration needs actual directory support in the existing checkpoint/patch transaction, rather than removing an exclusion and pretending restoration works.

#### L004 — Checkpoint coverage omits partial selections and empty directories

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/capture.rs` — `capture_selected_coverage`, `validate_automatic_checkpoint`, `observe_empty_directory`; `capture/automatic.rs` — empty-directory branch; `crates/app/peritus-product-runner/src/control/checkpoint.rs` — `CheckpointFileVersion`; `crates/runtime/peritus-patch/src/operation.rs` — `PatchOperationKind` currently contains regular-file operations only.

**Proposed minimum fix:** Stop excluding partial ranges when their backing workspace file is authorized for capture. For empty directories, add a checked directory version and create/remove-directory transaction operation, including manifest/recovery representation and exact empty-directory checks; then remove the exclusion. Keep external imports and external effects excluded. Existing file-only records must continue to decode.

### Preserve governance and storage errors through admission and UI mapping

A transient read/storage failure must remain an explicit recoverable error for the original run, rather than being converted to fabricated context or generic admission pressure. Keep mutation admission closed while authority is unknown, with diagnosis and retry still available.

#### L026 — Governance failures become persistent execution/admission barriers

**Current code:** `crates/app/peritus-daemon/src/product_run/interaction/live.rs` — `stable_request_context`, `reference_authority_context`, `revision`, `render`, `protected_paths`, `effective_permissions`, `provider`, `input`.

**Proposed minimum fix:** Carry fallible governing-context reads into the existing request/tool admission path and retry before resuming the same operation. Retain the last validated binding as historical state, explicitly unavailable for new effects until refreshed; do not use fake revision `u64::MAX`, an empty authority context, or stop prose as successful data. Keep unknown permissions fail-closed.

#### L754 Ordinary workbench error mapping discards causes and labels storage failures as backpressure

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench.rs` — `error_value`; `workbench/images.rs` — `daemon_error`; `workbench/files.rs` — `prepare_file`; `workbench/folder_mutation/init.rs` — `discover_init`, `prepare_init_patch`.

**Proposed minimum fix:** Map Busy/backpressure, missing identity, permission failure, unsupported provider, source drift, and corruption according to their actual typed causes, and attach the available diagnostic/recovery detail. Stop dropping causes in `map_err(|_| ...)`. Use the shared lock/replay repair for blocking work; do not replace every error with a new generic retry state.

### Use explicit authority without accidental restrictions or forced new runs

Keep actual permissions and enforceable protected paths. Correct inferred restrictions and allow existing sessions to accept newly explicit authority.

#### L039 — Tool authority and mode gates can suppress process work

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor.rs::{required_permissions,execute}` checks live permissions and read-only mode before effects; owned command poll/recover/cancel already require no new permission.

**Proposed minimum fix:** No blanket permission removal is warranted. Correct mode selection at callers if wrong, and preserve existing observation/cleanup access after revocation.

#### L044 — Any leave-alone path disables all unconfined commands

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/access_policy.rs::from_transcript` infers opaque/hidden restrictions lexically. `authorize_command` blocks unconfined commands whenever any hard-constraint path exists.

**Proposed minimum fix:** Replace lexical inference with explicit task constraints. Route commands through enforceable confinement when available. The present raw backend cannot protect selected paths, so retain refusal for effects it cannot confine; removing that guard alone would violate authority rather than repair the capability gap.

#### L048 — File ownership can require a new run for deletion

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/ownership.rs::ensure_removable` accepts baseline/direct/command ownership but otherwise tells the user to start a new run. Baseline enumeration skips errors and non-UTF-8 untracked paths.

**Proposed minimum fix:** Allow explicit same-session deletion authorization to enroll the exact current target with identity evidence. Replace the new-run instruction and report incomplete baseline enumeration. Do not infer ownership of unrelated late files.

#### L164 — Hunk leave-alone feedback protects the whole path until explicit dismissal

**Current code:** `crates/app/peritus-product-runner/src/control/review.rs::ReviewFeedback` and `control/review/ledger.rs::{prompt,dismiss}` intentionally protect the complete path for LeaveAlone because the command backend cannot confine writes to a hunk.

**Proposed minimum fix:** Keep this explicit user authority constraint. Make release/rebind state clear and use a genuinely confined backend for any finer-grained write permission; do not delete the guard or reinterpret KeepBehavior/Explain as write grants.

### Keep conversation revision width in request identities

Avoid narrowing an already durable identity counter.

#### L055 — Conversation request identity narrows the durable revision range

**Current code:** `crates/app/peritus-product-runner/src/execution/conversation.rs` converts revision u64 to u32 for `request_name`. `turn/request_name.rs::format` takes the cycle as u32 and formats it as decimal text.

**Proposed minimum fix:** Give the conversation identity path a u64 revision argument, removing the narrowing conversion. Preserve the exact decimal format for previously representable values and leave unrelated role cycle semantics intact.

### Preserve literal structured argv in persisted run instructions

Do not interpret literal arguments through a whitespace-only string parser.

#### L109 — Persisted direct-command parsing excludes quoted paths and ordinary shell syntax

**Current code:** `crates/tools/peritus-tools-shell/src/input.rs::ExecInput::from_command_line` rejects quotes/operators and splits ASCII whitespace; `from_arguments` already accepts structured executable/argv.

**Proposed minimum fix:** Persist/use the existing structured form for paths and arguments containing spaces or quotes. Preserve the separately authorized script route; adding shell interpretation to the literal entry point would change authority.

### Preview Network capability must match the actual backend

Separate backend capability from provider configuration. Network denial needs a backend that can enforce it; removing permission checks from inherited-host execution would widen authority.

#### L190 — Preview terminal input requires Network permission even for local interaction

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/launch/terminal.rs::authorize_preview_terminal_input` requires Read/Write/Process/Network; `launch/process.rs` launches via CommandRuntime.

**Proposed minimum fix:** Provide an enforceably confined local preview backend and route denied-Network interaction to it. The current inherited-host launch path must retain Network permission until confinement exists; do not assume the ordinary command runtime already guarantees network isolation.

#### L194 — Host Network availability is inferred from provider configuration, and previews require it unconditionally

**Current code:** `crates/app/peritus-daemon/src/product_run/permissions.rs::HostPermissionCatalog::new` derives network from nonempty providers; preview request permission policy also requires Network.

**Proposed minimum fix:** Derive Network availability from the actual execution backend rather than provider-map membership. Apply L190's confined-backend repair for denied-network local previews; do not drop the permission check from unrestricted inherited-host execution.

### Admit execution for a valid writable fork

Use existing branch authority without requiring a goal when ordinary chat execution is sufficient.

#### L201 — An isolated writable fork without a goal can be created but cannot start execution

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/execution.rs::branch_execution_allowed` has no isolated-writable/zero-goal/StartExecution arm. `workbench/fork.rs::branch` also supplies empty criteria when the source has no goal.

**Proposed minimum fix:** Align branch construction and execution admission for a no-goal writable fork, including the `ConversationBranch` criteria validation from L154; add the missing StartExecution/Chat case. Retain separate nonoverlapping writable workspace, exact lineage and governed contract. Fixing the match alone cannot admit a branch rejected during construction.

### Continue executable discovery past unusable candidates

Retain canonical executable pinning and report actual launch/discovery errors.

#### L220 — Native executable discovery stops at a nonexecutable file instead of trying later PATH entries

**Current code:** `crates/model/peritus-provider-{openai,anthropic}/src/runtime/executable.rs::discover` immediately returns pin(candidate) for the first regular file, even if it is not executable.

**Proposed minimum fix:** Skip nonexecutable PATH candidates and continue searching before pinning the first usable executable. Keep canonical executable pinning and report all relevant discovery failures; handle platform launchers through the existing platform launch path.

### Keep UI recovery independent of presentation configuration and recent lists

Recover the exact selected session/run and each outstanding query independently. Retain drafts and prompt authority through their existing owners.

#### L245 — Presentation retention depends on exact config equality; latest-conversation recovery is one-shot

**Current code:** `crates/app/peritus-tui/src/runtime/state.rs::take_model` requires full TuiConfig equality; retain clears prompts. `model/product/resume.rs` clears LatestConversation on failure and selects ties by encounter order.

**Proposed minimum fix:** Separate durable session/conversation selection from cosmetic config equality, retaining workspace/endpoint ownership checks. Keep failed resume discovery retryable in place and sort tied activity revisions by stable conversation identity. Reload outstanding prompts through the durable prompt repair below; do not re-use stale transport authority.

#### L246 — One outstanding observation can suppress later polling; recent-list replacement can lose an older selection

**Current code:** `crates/app/peritus-tui/src/model/product/observation.rs::poll_product_runs` returns early for any pending product query, suppressing interaction refresh. `accept_product_runs` replaces the recent vector and settlements; exact responses insert a run into that same vector.

**Proposed minimum fix:** Track recent, exact-run and interaction observations separately, with explicit cancellation/reissue and connection-generation correlation. Preserve selected exact runs outside recent-list replacement and ignore obsolete responses. Share the wider/paged inspection-scroll repair with L260.

### Validate the exact persisted scheduler successor with one contract

Remove heuristic byte gates and make actual aggregate constraints agree across typed/wire admission, commit and restart.

#### L289 — Scheduler replay repeatedly clones and hashes retained state, and estimated size is an additional admission gate

**Current code:** `crates/orchestration/peritus-scheduler/src/reducer.rs` clones state, applies/refreshes, checks estimated bytes, then adds the command and hashes. state.rs estimates fixed per-record sizes.

**Proposed minimum fix:** Delete estimated_encoded_bytes as an admission/reopen gate; use actual encoding and the shared expandable state representation. Reuse checkpoints and avoid redundant full-state clones during replay while preserving canonical hash semantics and resource accounting.

#### L296 — Scheduler ingress can durably accept state that its own restart decoder rejects

**Current code:** `crates/orchestration/peritus-scheduler/src/wire/command.rs` reads descriptors/specs with production_limits; reducer/apply/worker_control.rs omits aggregate concurrency recheck and admission.rs omits aggregate dependency/attempt recheck. durability/binding.rs checks cross-record facts but not validate_inert.

**Proposed minimum fix:** Revalidate admitted descriptors against the actual aggregate's remaining selected constraints, after removing the synthetic ones. Validate the completed successor with the same invariant contract used on decode before append, including typed command paths. Keep the predecessor usable on rejection and preserve exact historical schema replay.

#### L297 — State-size admission runs before adding the accepted command to history

**Current code:** `crates/orchestration/peritus-scheduler/src/reducer.rs::decide` checks the estimate before prepare_cursor adds another command; wire/state.rs subsequently invokes validate_inert on the final state.

**Proposed minimum fix:** Remove the estimate gate with L289 and validate all remaining invariants/actual encoded admission after cursor advancement, on exactly the successor persisted. Use the same contract during reopening so successful control commands cannot create rejected checkpoints.

### Encode immutable challenge identity separately from mutable observation time

Repair the existing approval format rather than changing the signed semantic digest.

#### L384 B1 encoding a request after authority-time advancement disagrees with its semantic digest

**Current code:** `codec/request.rs:29` encodes mutable `authority_time` in field 18, while `digest.rs:313` hashes original challenge_epoch/challenge_tick; reconstruction requires those values to match.

**Proposed minimum fix:** Encode field 18 from immutable original challenge time. Persist the advanced authority floor separately in aggregate state and version only that affected format, preserving historical request digests and decoding.

### Build the acceptance view from relevant current evidence

Retain full review/authority history separately. Stable identity, chronological order and the independent quorum are different contracts.

#### L396 B2 canonical snapshots couple identity order to cycle order and disallow repeated finding/approval history

**Current code:** `peritus-quality-policy/src/evidence/reviews.rs:49` requires cycle-ID and cycle-ordinal order simultaneously and disallows repeated finding IDs across reviews; `evidence/authority.rs:41` disallows repeated approval subjects.

**Proposed minimum fix:** Canonicalize stable IDs independently of chronological ordinals and admit distinct reviewers in one cycle. Construct an explicit current evidence view from retained history, combining corroborating provenance without inventing new defects or erasing contradictory observations; coordinate canonical/formal relations.

#### L397 B2 rejects any stale observation and checks all current reviews beyond an already valid quorum

**Current code:** `evaluator/freshness.rs:137` requires every supplied observation to be current. `evaluator/reviews.rs:228` checks independence across all supplied current reviews beyond a sufficient quorum.

**Proposed minimum fix:** Select an explicit exact-revision set and sufficient independent quorum for acceptance, retaining stale/surplus history separately. Surface contradictory current facts and unresolved defects; do not let irrelevant older or redundant records invalidate otherwise valid coverage.

#### L398 B2 treats superseded or unnecessary authority facts as current acceptance failures

**Current code:** `evaluator/authority/supplied.rs:15` validates every current waiver, including nonblocking/resolved findings; authority evaluation also demands approvals corresponding to supplied facts.

**Proposed minimum fix:** Exclude explicitly superseded waivers and historical approvals from the active acceptance input after resolution, retaining immutable records. Validate active authority decisions and report unresolved contradictions rather than treating obsolete facts as current blockers.

#### L399 B2 acceptance construction and evaluation repeatedly scan collections without indexed lookup

**Current code:** `evaluator/authority/lookup.rs:33` scans waiver/approval vectors; construction, gate, requirement, coverage and independence helpers repeat similar searches.

**Proposed minimum fix:** Build keyed definitions/findings/approval/waiver matches once and use sets for independence checks. Preserve canonical output and exact current-evidence semantics; no new acceptance store is needed.

### Validate probability values at every public prediction boundary

A mathematical probability range is an integrity requirement, not an artificial workload limit.

#### L453 An accepted out-of-range probability threshold cannot survive F0 wire decoding

**Current code:** `change/prediction.rs:85` has a checked probability helper, but public MetricValue can bypass it and Prediction::new at :151 checks only metric compatibility; wire decoding rejects out-of-range values.

**Proposed minimum fix:** Validate 0..1,000,000 in Prediction::new or require the checked wrapper, keeping writer/reader admission identical. Preserve the mathematical range and prevent direct enum construction from producing unreadable durable state.

### Reject unsatisfiable tool/resume requests before submission

Validate only features the request actually needs, against the selected provider's implemented contract.

#### L459 C5's complete request constructor admits unsatisfiable tool selection and unsupported semantic continuation

**Current code:** `peritus-model-protocol/src/request/validation.rs:11` does not receive ToolChoice; continuation validation at :119 requires resumable capability only for exact cursors.

**Proposed minimum fix:** Validate Required/Specific tool choice against the declared catalog and semantic/exact continuation against the exact profile's supported strength. Return a precise pre-submission capability error while preserving local history.

#### L471 Compatible profiles exclude all provider-side continuation and several unused capabilities

**Current code:** `peritus-provider-compatible/src/profile.rs:224` rejects every Unknown/unmapped capability, and `request/validation.rs:15` permits only stateless replay and no continuation.

**Proposed minimum fix:** Check required request capabilities rather than every unused Unknown field. Preserve durable local replay; provider continuation requires an implemented verified mapping, so simply deleting lifecycle checks is not sufficient to create exact resume.

### Qualify providers through the ordinary response and effort contract

A canary must exercise the selected provider behavior and normalized terminal rules.

#### L468 Live provider qualification bypasses normalized response legality

**Current code:** `peritus-provider-core/src/qualification.rs:313` qualifies on text plus ResponseCompleted without passing events through ResponseReducer.

**Proposed minimum fix:** Run the canary through the ordinary reducer and successful terminal rules, retaining incomplete/refusal/capacity causes. A completed event alone is insufficient qualification.

#### L469 Connection qualification conflicts with explicitly selected reasoning effort

**Current code:** `qualification/canary.rs:20` requests Low reasoning while `effort.rs:19` wraps an explicitly selected effort, creating conflicting request policy.

**Proposed minimum fix:** Build canary and tool-stage requests with the selected effort. Keep canary output scope small and report effort incompatibility separately from authentication.

### Derive valid normalized IDs and preserve provider-frame semantics

Keep original provider IDs as evidence. Distinct normalized children need distinct identity and collision-free coordinates.

#### L477 Compatible normalization expands IDs and packs indices beyond the original value contract

**Current code:** `stream/chat.rs:304` appends suffixes to maximum-width response IDs; tool indices add 65,536 and `stream/responses.rs:351` packs coordinates by multiplication without bounding content to that radix.

**Proposed minimum fix:** Derive normalized IDs from original identity/type/coordinates with a collision-resistant digest fitting by construction. Use a checked tuple or an injective mapping with explicit coordinate bounds and retain original IDs as evidence; do not permit packed-coordinate collisions.

#### L492 Anthropic and Google reuse one provider frame identity for multiple normalized events, causing C5 to discard required semantics as duplicates

**Current code:** Anthropic `stream/state.rs:108,191` and Google `stream/state.rs:81` attach the same provider frame ID to every expanded event; C5 `reducer/stream.rs:51` treats later children as duplicates before applying semantics.

**Proposed minimum fix:** Deduplicate at provider-frame admission, then bind provider ID only to the first normalized child or derive distinct child IDs. Reuse the compatible adapter pattern, preserving every semantic child, sequence legality and conflicting-frame rejection.

#### L499 OpenAI stream indexing and whole-fragment assembly retain hidden bounds

**Current code:** `peritus-provider-openai/src/stream/state.rs:71` tracks increasing sequences, while `output.rs` derives/assembles item/content identities and buffers structured output under hidden fragment bounds.

**Proposed minimum fix:** Use checked tuple coordinates and fitting derived IDs, and restore decoder assembly through the shared exact-continuation repair. Detect gaps only according to the actual provider sequence contract, expose buffering progress and retain terminal-content comparison.

### Validate parsed batches before publishing one terminal outcome

The public terminal boundary must not hide errors or cancellation already known by the adapter.

#### L479 Compatible streams can hide a known malformed suffix behind an already queued completion

**Current code:** `peritus-provider-compatible/src/stream.rs:164` enqueues events while iterating a parsed batch; enqueue marks terminal immediately, and later fail appends an error behind it.

**Proposed minimum fix:** Validate all already parsed frames/semantic events before exposing terminal completion and settle exactly one public terminal result. Observe cancellation before queued delivery; do not wait indefinitely for EOF once the actual logical boundary is established.

#### L483 Anthropic terminal publication suppresses errors detected later in the same parsed batch

**Current code:** `peritus-provider-anthropic/src/stream.rs:109` suppresses cancellation after internal terminal even before queued public delivery; state marks terminal during parsed-batch expansion.

**Proposed minimum fix:** Validate the entire parsed batch before terminal publication and arbitrate cancellation at that public boundary. Emit exactly one authoritative terminal outcome, including already detected failures rather than hiding them behind completion.

#### L496 OpenAI terminal queue can hide malformed suffixes and defer local cancellation

**Current code:** `peritus-provider-openai/src/stream.rs:104` enqueues completion while processing a batch; next():230 delivers pending events before observing cancellation/errors.

**Proposed minimum fix:** Apply shared batch-before-terminal validation and check cancellation before queued public delivery. Replace provisional completion when the same parsed batch is already known malformed, publishing one terminal result.

### Keep real provider contracts distinct from session lifetime

Validate implemented features before submission and retain durable local history; provider cache TTL does not expire logical work.

#### L481 Anthropic direct request controls are restricted by adapter mappings, including cache TTLs that do not expire sessions

**Current code:** `peritus-provider-anthropic/src/request.rs` maps specific beta/cache/media features, including requiring Files API opt-in for provider-file references.

**Proposed minimum fix:** Keep actual API feature restrictions and cache validity. Validate them before work begins and retain local conversation replay; no blanket TTL removal or fabricated provider capability is warranted.

#### L486 Google gateway routing accepts an Interactions configuration that loses its gateway prefix, and discovery uses beta despite stable-only documentation

**Current code:** `peritus-provider-google/src/config.rs:57` admits reviewed gateway prefixes, but `request.rs:67` composes Interactions with an absolute path that replaces the prefix.

**Proposed minimum fix:** Preserve gateway prefix in path composition or reject an unsupported gateway/dialect combination during construction. Make discovery version/routing explicit and apply shared catalog continuation, retaining exact credential-origin checks.

#### L487 Google request projection omits parallel policy and repeatedly scans the whole transcript for tool results

**Current code:** `peritus-provider-google/src/request.rs:41` does not map the requested parallel policy; `request/content.rs:45` searches the entire transcript backwards for tool names, allowing later calls to satisfy earlier results.

**Proposed minimum fix:** Validate or enforce actual tool-choice/parallel mapping before submission. Build preceding-call indexes in one forward traversal so results bind only to already declared calls, retaining precise unsupported-feature errors.

#### L497 OpenAI request admission and whole-value encoding expose secondary blockers

**Current code:** `peritus-provider-openai/src/request/input.rs:153` materializes Base64 media and request options map provider-specific constraints; client config also applies finite retry/transport defaults.

**Proposed minimum fix:** Apply shared retry/transport admission corrections, validate artifact resolution and reasoning representation before credential use and count actual JSON/Base64 size. Preserve provider-required options; prompt-cache TTL needs no session-lifetime repair.

### Keep Git compatibility and ignored content separate from task validity

Retain exact root/registration ownership and explicit authorization for filters or destructive cleanup.

#### L526 Git repository admission excludes filter-configured repositories and non-UTF-8 roots

**Current code:** `peritus-git/src/repository.rs:251` blanket-rejects configured external filters; native repository/path helpers require UTF-8.

**Proposed minimum fix:** Use lossless native Unix path bytes. Detect required filter capability before admitting work and invoke a selected filter only through a supervised authorized path; keep option/environment protections and exact root identity.

#### L530 Git handle recovery has strict path, topology and schema requirements but no TTL

**Current code:** `worktree/recovery.rs` verifies existing destination, registration and snapshot topology; no TTL controls exist in these handle contracts.

**Proposed minimum fix:** Use the existing trusted current-worktree recovery route when HEAD legitimately advances. Keep exact registration/snapshot binding; no TTL removal or automatic path rebinding is needed.

#### L531 Snapshot admission and clean reconciliation include ignored path state

**Current code:** `snapshot/operations.rs:90` compares complete status digests; `reconcile.rs:110` includes ignored dirtiness unless expectation.allow_ignored is selected.

**Proposed minimum fix:** Separate candidate-content correctness from ignored observational dirtiness and use the existing allow_ignored policy where appropriate. Preserve unrelated ignored files and require explicit authorization for destructive removal.

### Reference sandbox fixtures and lifecycle evidence

Keep fixture repairs small. Strengthen lifecycle evidence at the actual backend boundary without inventing a production persistence service for the reference backend.

#### L561 Reference sandbox sessions are transient and their terminal operations do not reconcile repeats

**Current code:** `peritus-sandbox/src/reference/session.rs::{cancel,release}` maintains transient fixture state and rejects some repeated terminal operations.

**Proposed minimum fix:** Make terminal cleanup idempotent in the reference fixture where outcomes are already known. No production persistence framework is warranted for this reference backend.

#### L562 Sandbox observation caps drop later detail and teardown classification checks only event positions

**Current code:** `peritus-sandbox/src/reference/{mod,session}.rs` caps observation detail; `refinement.rs` checks scalar event positions rather than complete bound teardown evidence.

**Proposed minimum fix:** Keep lossy optional diagnostics separate from authoritative lifecycle evidence. Require matching plan/backend/session sequence and verified process termination before accepting release.

### macOS selected capabilities and preparation

Prepare only facilities requested by the plan. Keep integrity checks cancellable and errors specific, avoiding consumption or resource allocation before deterministic preflight.

#### L592 Configured macOS proxy or secret support rejects plans that do not use it

**Current code:** `peritus-sandbox-macos/src/preparation.rs::{validate_protected_bindings,prepare_authorized}` requires configuration presence to equal plan use and allocates configured proxy/secrets before later compilation.

**Proposed minimum fix:** Treat configured facilities as availability, selecting them only for required capabilities. Move inert compilation/resource/manifest checks ahead of allocation and retain cleanup custody on any later failure.

#### L594 macOS preparation repeats bounded synchronous integrity work and collapses resource failures

**Current code:** `peritus-sandbox-macos/src/preparation.rs` repeatedly verifies helper/Seatbelt and `preparation/resources.rs` reads whole bounded helper/secret files; preparation maps errors to generic causes.

**Proposed minimum fix:** Stream verification cancellably without independent byte policy ceilings; retain exact protected-path/digest checks. Preserve typed preparation and cleanup errors rather than issue unnecessary reauthorization.

#### L595 macOS support probing uses mandatory synthetic deadlines and weak native evidence

**Current code:** `peritus-sandbox-macos/src/probe/native.rs` uses mandatory connection durations, an allow-default profile probe and broad resource support; probe request supplies synthetic timing.

**Proposed minimum fix:** Remove mandatory probe deadlines, make cancellation explicit and supervise/drain/reap every probe. Observe the selected deny-default/resource controls rather than advertise support from weak booleans.

#### L596 macOS resource supervision has fixed traversal ceilings and fragile live sampling

**Current code:** `peritus-sandbox-macos/src/resource_monitor.rs` synchronously scans disk and fixed PID/descriptor buffers; `resource_monitor/pids.rs` rejects full buffers and sampling merges peaks.

**Proposed minimum fix:** Remove the million-entry cutoff, grow enumeration buffers and distinguish vanished entries from unknown measurements. Sample off the control executor, enforce only selected dimensions and correct CPU accounting without counting the same cumulative usage twice.

#### L597 macOS helper setup silently tightens inherited limits and can lose secret cleanup evidence

**Current code:** `peritus-sandbox-macos/src/runner.rs` registers secret files after materialization; `runner/native.rs` ignores failed partial-file removal and installs native limits.

**Proposed minimum fix:** Register secret custody before creation/write and retain failed cleanup in session recovery. Negotiate selected limits against inherited hard ceilings, raising a lower soft limit where allowed instead of silently tightening the requested contract.

### Complete workspace registrations independent of UI selection

Use the existing durable registration collection for identity and authority; keep recency and active selection as presentation only.

#### L658 Product workspace recent-list limits still remove discoverable choices; retained registrations are a different surface

**Current code:** `peritus-product-state/src/workspace/selection.rs` retains registrations but active/find_repository resolve recent only; registered excludes direct folders.

**Proposed minimum fix:** Extend existing retained lookup to all remembered managed/direct identities and lineages; make the 32-recent view presentation-only. No second registry is needed.

#### L665 Direct-folder navigation and global tool allowlisting depend on the active presentation selection

**Current code:** `peritus-launcher/src/app/navigation.rs::configured_context` searches registered plus active; bootstrap/configuration.rs gates the global tool list on active trust.

**Proposed minimum fix:** Resolve each conversation target from complete registrations and its own authority, including remembered direct folders. Preserve explicit trust/exact target identity independently of current presentation selection.

#### L668 Workspace selection can silently fall back from current-directory errors or regenerate forgotten lineage

**Current code:** `peritus-launcher/src/workspace_setup.rs::ensure_configured` discards current_dir/discovery errors then reuses active; activate_repository looks up only recent identities.

**Proposed minimum fix:** Surface current-directory discovery errors and use L658's complete lineage lookup before creating profiles. Atomically publish registration/profile refresh through existing generations.

### Product candidate control and owned execution

Fence mutations at the authoritative owner to the candidate and operation actually observed, and keep process execution independent of client lifetime.

#### L676 Product-run CLI executes candidate commands without a persistent process owner and accumulates all list pages

**Current code:** `peritus-cli/src/product_run.rs::list` accumulates all pages; control prechecks observed state then submits only run/action and candidate commands execute client-side.

**Proposed minimum fix:** Stream list pages and route candidate execution through daemon-owned process runtime. Carry expected candidate digest and operation generation in authoritative control admission, not a separate client precheck.

#### L691 Product controls are not fenced to the observed candidate or operation generation

**Current code:** `peritus-app-protocol/src/product/control.rs::ProductRunControl` contains only run_id/action; CLI prechecks candidate separately.

**Proposed minimum fix:** Add expected candidate digest and operation generation, validating them at the authoritative owner. Derive legal controls from reconciled effect state and keep waiting/recovery resumable.

#### L703 Wire product observations preserve settlement integrity but do not add context or candidate fences to controls

**Current code:** `peritus-app-protocol/src/wire/product.rs` encodes whole snapshots/controls; settlement decoding validates provenance but controls lack candidate/generation fences.

**Proposed minimum fix:** Encode/decode L691's fences and page large snapshot fields. Reuse L694 transfer reconciliation while preserving settlement integrity checks.

## 9. Workflow policy and retained safeguards

### Existing compaction and transport boundaries that do not need removal

These findings do not establish an independent execution blocker in the mapped code. Keep the functioning contract, and repair any specific misuse at its owning caller rather than removing framing or authority protections globally.

#### L027 — Governed live execution disables semantic compaction

**Current code:** `crates/app/peritus-daemon/src/product_run/interaction/live.rs` — `allows_semantic_compaction`; `crates/orchestration/peritus-agent/src/developer/execution.rs:94-127` — local-context versus legacy-compactor branch; `crates/app/peritus-product-runner/src/local_context/port.rs:178-204` — `observe`, `assemble`.

**Proposed minimum fix:** No change to the `false` legacy-compactor flag. Local context already calls `compact_locally`, `prepare_view`, and `publish`; capacity or continuation defects in that path belong to the local-context bundles.

#### L029 — Application-protocol default and negotiated capacity ceilings

**Current code:** `crates/app/peritus-app-protocol/src/limits.rs` — `AppProtocolLimits::new`, `negotiated`, `validate` accepts values wider than production when they fit the selected codec.

**Proposed minimum fix:** No constructor clamp removal is needed. Retain negotiated chunk/page/in-flight windows. Fix lifetime idempotency retention or a queue being treated as session capacity in the corresponding runtime bundle, using acknowledgement, paging, and durable identities.

### Keep real workspace ownership and paged run queries

These admission checks protect identity and workspace capabilities. Repair stale ownership through settlement rather than allowing conflicting owners.

#### L015 — Workspace run admission, folder mode, and result pagination

**Current code:** `crates/app/peritus-daemon/src/product_run.rs::start_interaction` checks duplicate/staged identity, active workspace ownership and discard availability. `validate_workspace_mode` rejects Build for ordinary folders; `query` takes an offset and a bounded page. Fallbacks are explicitly configured.

**Proposed minimum fix:** Keep these authority/capability checks and pagination. Release a barrier only when the existing run/discard is conclusively settled; preserve staged exact recovery. Ordinary-folder work should use its supported in-place mode rather than bypassing the Git requirement for Build.

### Retain the 500-line source-file lint and remove synthetic workflow obligations

Keep the 500-line source-file lint to prevent god files. Remove synthetic retry/monitoring obligations and name-based source exclusions; retain actual repository/task requirements and authority restrictions.

#### L140 — Embedded engineering prompts impose additional layout and loop policies

**Current code:** `crates/app/peritus-product-runner/src/engineering_workflow/reviewer.rs` prescribes 500-line findings and finite retry; `engineering_workflow.rs` requires finite retries and at least three periodic observations.

**Proposed minimum fix:** Keep the 500-line source-file lint unchanged. Remove mandatory finite retry counts and the three-observation quota. Use task-defined acceptance and the actual monitoring need, retaining meaningful forward-progress detection.

#### L143 — Candidate exclusion names classify entire directory components as generated

**Current code:** `crates/app/peritus-product-runner/src/workspace_filter.rs` treats any matching directory component such as `build` or `target` as generated.

**Proposed minimum fix:** Use actual ignore rules and explicit task/file ownership, with an explicit include route for requested source. Do not discard new source solely because an ancestor has a familiar build-directory name; preserve previously owned paths.

### Delivery gates must represent current explicit requirements

Keep exact path and removal authority. Make ambiguous lexical inference advisory and recognize an already correct result.

#### L176 — Deterministic output gates turn limited language heuristics into hard delivery requirements

**Current code:** `crates/app/peritus-product-runner/src/gates/explicit_paths.rs::{parse_path,transcript_requests_single_file}` and `explicit_paths/{extraction,language,alternatives}.rs` derive mandatory output policy from token windows.

**Proposed minimum fix:** Bind hard delivery requirements to explicit current instructions, preserve quoted paths with spaces/punctuation, and reduce later relaxation chronologically. Keep uncertain guesses advisory. Retain exact-root confinement and requested file-kind validation.

#### L177 — Closed single-file inventory requires an actual change and rejects every extra changed path

**Current code:** `crates/app/peritus-product-runner/src/gates/deliverable_inventory.rs::run` requires its single target in changed_paths and rejects every other changed path.

**Proposed minimum fix:** Accept an already correct unchanged target. Apply exclusive single-file delivery only to a current explicit requirement, and exclude known verification byproducts from deliverable changes; keep explicit removal and foreign-file safety checks.

### Explicit offline guarantees remain authority constraints

No synthetic workload repair should silently give an offline task unrestricted network execution.

#### L204 — Explicit fully-offline mode admits only literal-loopback compatible HTTP routes

**Current code:** `crates/app/peritus-daemon/src/config/context.rs::validate` forbids failover/nonlocal routes in fully_offline; `config/provider/locality.rs::is_local_task_route` admits literal-loopback compatible HTTP only.

**Proposed minimum fix:** No removal is needed for an explicitly selected fully-offline guarantee. Broaden route admission only when the existing backend can prove equivalent network denial; do not treat DNS locality or an unconstrained subprocess as that proof.

### Plan checks from the actual project and requested outcome

Reuse TargetGatePlan and TargetGateReport; add explicit project commands where discovery has no contract and distinguish missing environment from candidate defects.

#### L338 — Product target-gate discovery is restricted to six project families and fixed marker conventions

**Current code:** `product/plan.rs:13,128` recognizes six project families and classifies uncovered paths; `product/commands.rs:290` requires exactly two artifact-manifest fields.

**Proposed minimum fix:** Accept explicit host-approved command specs for uncovered project types in the existing plan. Extend the artifact manifest only for that command contract. Report unsupported/missing configuration as actionable planning state rather than repeatedly asking code repair to invent a supported project.

#### L339 — Fixed product gate commands can demand an invalid feature/toolchain/dependency configuration

**Current code:** `product/commands.rs:60` always plans Rust all-features/all-targets locked checks with warnings denied. Python dependency checks force pip --no-index and discover tests/lint through filesystem/text heuristics.

**Proposed minimum fix:** Use explicit repository gate configuration and applicable feature/toolchain matrices; retain requested strict/offline policies only where selected. Classify unavailable dependencies/tooling as environment recovery, and execute the configured commands instead of universal combinations that may be invalid.

#### L340 — Product target-gate acceptance universally requires a nonempty changed-path inventory

**Current code:** `product/plan.rs:194` requires changed paths, projects and commands for coverage; `product/report.rs:50` compares record count with command count rather than exact identities.

**Proposed minimum fix:** Require mutation only when the task contract requires it. Let read-only/artifact outcomes satisfy their declared evidence checks, and correlate every execution record to the exact planned command identity so duplicate or unrelated successful records cannot establish coverage.

### Retain the 500-line source-file lint and task-specific acceptance

Keep the 500-line source-file lint to prevent god files. Remove blanket synthetic work obligations from embedded workflow text; retain validation and acceptance actually required by the user's task and repository.

#### L678 Embedded workflow creates synthetic acceptance obligations even without elapsed deadlines

**Current code:** `peritus-product-runner/skills/{independent-reviewer,maintainable-developer}.md` treats >500-line files and unbounded retries as findings; production workflow/design prompts impose broad acceptance obligations.

**Proposed minimum fix:** Keep the 500-line source-file lint unchanged. Remove blanket finite-attempt and mandatory-design requirements that create unrelated blockers. Bind acceptance to actual task/repository obligations and use repeat-cursor detection plus continuation without expanding scope.

#### L692 Goal projections remove time/resource budget fields while preserving mandatory runner acceptance

**Current code:** `peritus-app-protocol/src/workbench/goal.rs::WorkbenchGoalDefinition::new` requires a mandatory RunnerAcceptance criterion; projection retains cumulative observational usage without an eight-hour clock gate.

**Proposed minimum fix:** Require only goal-appropriate acceptance supported by actual evidence capabilities. Keep observational accounting without quotas and expose unsupported evidence as a resumable blocker; no eight-hour limit exists here.
