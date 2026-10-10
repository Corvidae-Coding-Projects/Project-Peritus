# File, attachment, artifact, and workspace size limits

## Read large sources without imposing source or absolute-range ceilings

Keep exact selected bytes, source digests, metadata drift checks, and no-follow file handles. Remove absolute source/range ceilings, and let the caller's requested response size remain a page bound. Full-file checkpoint capture and bounded interactive previews must select their appropriate existing read mode.

### L007 — Independent source-scan and included-content ceilings

**Current code:** `crates/runtime/peritus-workspace/src/scoped_inspection.rs` — `MAX_INSPECTION_SOURCE_BYTES`; `scoped_inspection/read.rs` — `FolderInspection::read_file`, `Scan::new`, `Scan::accept`; `scoped_inspection/selection.rs` — `FileReadSelection::bytes`, `lines`; `inspection.rs` — `MAX_INSPECTION_FILE_BYTES`.

**Proposed minimum fix:** Delete the 64 MiB source/byte-offset and 67,108,864-line cutoffs and the universal 8 MiB caller-bound clamp. The reader already hashes in 64 KiB chunks; retain that loop rather than writing another hashing reader. Use checked/wider line accounting when removing its old bound, and offer continuation for an oversized requested selection instead of silent truncation.

## Expose complete workspace evidence through bounded continuations

Keep bounded replies but return exact continuation positions and omissions. Remove whole-file exclusion policies; do not treat a truncated prefix as a complete observation.

### L041 — Workspace inspection truncates or omits evidence

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/inspection.rs::{list,search,read,read_line_range}` has depth/entry/match caps, silently skips large search files and child errors, and sends reads through the separate output prefix truncator. List/search currently have no continuation cursor.

**Proposed minimum fix:** Add continuation to list/search, stream large search files, and report skipped entries. Return actual last displayed line plus a byte continuation for oversized individual lines; the existing line range alone cannot recover a line that exceeds the byte bound. Keep bounded response pages.

### L045 — Tool output and filesystem-coverage exclusions

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/effect.rs::limit` cuts output at 512 KiB. `path.rs::ignored` excludes build/dependency directories from traversal, while `checked` preserves workspace/traversal/symlink authority. Deletion is redirected to the exact-target removal tool.

**Proposed minimum fix:** Make prefix output resumable under the inspection repair and permit explicitly selected ignored-directory inspection. Keep path authority and the grounded deletion route; same-session deletion authorization is addressed separately.

### L124 — Artifact navigation samples 2,000 entries after collecting entire directories

**Current code:** `crates/app/peritus-product-runner/src/design/artifact.rs::inventory_with_limit` collects/sorts a full directory before applying the 2,000-entry sample boundary and aborts on child errors.

**Proposed minimum fix:** Enumerate incrementally with cancellation and paged continuation; report inaccessible entries individually. Keep the sample labeled partial and reuse the shared workspace inspection continuation.

## Filesystem tool scale, continuation and exact mutation handoff

Use ranged/paged access sized by encoded bytes, keep every retained result reachable and restore mutation outcomes through existing operation evidence.

### L634 Filesystem schemas impose whole-file and inline-mutation ceilings without ranged alternatives

**Current code:** `peritus-tools-fs/src/input.rs` caps traversal depth/entries/search bytes and fs.read at 48 KiB; schemas/decoder expose whole-file and bounded inline mutations.

**Proposed minimum fix:** Add fs.read ranges and traversal/search continuation, remove artificial whole-job quotas and use existing artifact input for larger mutations. Keep transfer pages representable and mutation preimages exact.

### L635 Filesystem traversal/search loses progress at bounds and silently excludes some search scope

**Current code:** `peritus-tools-fs/src/read.rs::{discover,search,collect_matches}` builds full traversal, skips large/binary files, fails at aggregate/match caps and prefixes previews to 512 bytes.

**Proposed minimum fix:** Return continuation plus explicit skipped/depth scope, stream actual reads and center previews on matches. Isolate unrelated unsafe entries while preserving no-follow protection.

### L636 Filesystem result windows expose only the first 500 items and can still fail the canonical JSON ceiling

**Current code:** `peritus-tools-fs/src/render.rs` takes the first MAX_RENDER_ITEMS=500 before canonical JSON construction; protocol JSON/envelope limits can reject that window.

**Proposed minimum fix:** Size each page by actual encoded bytes and expose continuation/full artifacts. Remove independent JSON policy clamps consistently; preserve exact numeric values and file content.

### L637 Filesystem dispatch is synchronous, loses error distinctions, and retains candidate handoff only in its live object

**Current code:** `peritus-tools-fs/src/dispatcher.rs::start` executes synchronously, uses start time as completion time and retains MutationOutcome only in an Option.

**Proposed minimum fix:** Route work through owned cancellable tool execution, preserve typed filesystem/workspace recovery causes and observe actual completion time. Reconstruct exact mutation handoff from L582's retained operation evidence.

## Hash remembered files without a total-byte cutoff

Use the existing streaming hasher; its buffer size is not a file-size limit.

### L076 — Remembered-file hashing has a 64-MiB synthetic stop

**Current code:** `crates/app/peritus-product-runner/src/local_context/memory/environment.rs::digest_file` already streams in 8-KiB buffers but returns an error after 64 MiB.

**Proposed minimum fix:** Delete the total-byte refusal, retaining checked byte arithmetic and digest validation. Add cancellation/yielding through the existing refresh owner where needed; no alternate hash store is required.

## Continue exact memory retrieval without source-size or packing blockers

Use existing scope-bound cursors and offsets, stream source ranges/search, and charge actual encoded response bytes.

### L078 — Memory retrieval has small pages, scan caps, and conservative byte packing

**Current code:** `crates/app/peritus-product-runner/src/local_context/tools/read.rs::{execute,source_page,append_source,state_page,cursor}` imposes request/query/handle/cursor limits, scans 32 sources and 64 MiB per page, loads full artifacts, and packs source text at one sixth of remaining bytes. A large state entry cannot be partially returned.

**Proposed minimum fix:** Remove independent query/handle/request policy maxima; keep bounded pages with explicit continuation. Stream search/ranges, continue later matches within a source, and return partial state entries when needed. Pack actual escaped bytes rather than a worst-case sixfold estimate.

## Checkpoint capture and patch restore capacity

Remove the fixed file, aggregate-payload, operation-count, and atomic-install admission ceilings together. Reuse the existing checkpoint body rows and C1 patch transaction; do not add a second checkpoint store. Patch identity encoding, manifest decoding, journal state admission, and SQLite length admission must accept the same sizes. Keep nonempty operations, exact preimages, protected paths, checked arithmetic, and one atomic publication.

### L003 — Checkpoint capture inherits patch byte ceilings

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/capture.rs` — `capture_selected_coverage`, `capture_checkpoint_paths`, `observe_path`; `capture/automatic.rs` — `capture_automatic_checkpoint_for_operation`, `seal_checkpoint` still use the 8 MiB patch policies.

**Proposed minimum fix:** Remove the aggregate comparisons and stop passing `MAX_FILE_BYTES` as a checkpoint storage policy. Return the original `WorkspaceError` for failed inspection; reserve `StaleRevision` for observed drift. Checkpoint bodies already use `StateInstall` in `product_control/storage/checkpoints.rs`, so use that path.

### L006 — Patch creation, replacement, deletion, and encoding admission limits

**Current code:** `crates/runtime/peritus-patch/src/set.rs` — `MAX_FILE_BYTES`, `MAX_PATCH_BYTES`, `MAX_PATCH_OPERATIONS`, `PatchSet::new`, `canonical_identity`; `content.rs` — `FinalFile::new`; `verified.rs` — `patch_bounds_valid`.

**Proposed minimum fix:** Remove the 8 MiB/1,024-operation rejections, including the preimage-size check that rejects deletes. Change the executable predicate and its Verus contract together. Stop selecting `CodecLimits::PRODUCTION` for local patch identity; preserve canonical bytes for existing patches and actual length-prefix overflow checks.

### L013 — Checkpoint coverage and restore capabilities disagree

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/rewind/plan.rs` — `restore_plan_with_store` builds `FinalFile` and `PatchSet`; `checkpoints/projection.rs` — `patch_input` erases every `PatchError`.

**Proposed minimum fix:** Use the repaired patch admission for every captured path in the same transaction, and preserve typed patch errors in `patch_input`. Keep preview equality and exact owned-postchange comparisons; do not split a restore into independently visible partial patches.

### L016 — Transaction manifest ancestor and decoder ceilings

**Current code:** `crates/runtime/peritus-patch/src/transaction/manifest.rs` — `validate_patch_capacity`, `Manifest::encode`, `Manifest::decode`, `read_identity`.

**Proposed minimum fix:** Ancestor paths are already deduplicated with `BTreeSet`; do not implement deduplication again. Remove the 1,024-entry and 8 MiB identity checks and use matching local-storage codec capacities on encode/decode. Keep checksum, path ordering, transaction binding, and the existing schema for unchanged encodings.

### L020 — Journal atomic-batch ceilings restrict checkpoint admission

**Current code:** `crates/state/peritus-journal/src/append_plan.rs` — `MAX_*` batch constants; `append_plan/validation.rs` — `validate_bounds`; `crates/app/peritus-daemon/src/product_control/storage/checkpoints.rs` — `checkpoint_installs`, `checkpoint_key`.

**Proposed minimum fix:** Remove the fixed batch collection comparisons while keeping nonempty event/head requirements and canonical uniqueness checks. Replace the `u16` checkpoint index ceiling with an extended key encoding; keep the existing two-byte keys for old indexes so retained bodies remain readable. Publish bodies, root, and receipt in the same batch.

### L035 — Tool writes and completed-command receipts impose a tighter file ceiling

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor.rs::MAX_FILE_BYTES`, `executor/effects.rs::{write,patch}`, and `executor/checkpoint_observer.rs::{prepare_write_checkpoint,prepare_patch_checkpoint,exact_file_receipt}` enforce two MiB, including after a successful command. The receipt actually needs a digest, size and mode, not an inline file body.

**Proposed minimum fix:** Remove these file-size checks with the checkpoint/storage ceilings. Stream the post-command file digest instead of reading the entire file merely to produce a receipt; retain before/after identity checks. No new receipt store is needed.

### L036 — Command preflight checkpoints every enrolled file

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/executor/checkpoint_observer.rs::prepare_command_checkpoints` checkpoints every enrolled scope path and checks that the scope stayed unchanged.

**Proposed minimum fix:** Reuse already captured, unchanged preimages and remove the capture ceilings in this group. Retain preflight coverage of the actual writable scope and exact scope identity; do not bypass rollback protection.

## Checkpoint manifest representation and generated labels

Remove artificial manifest count restrictions, and prevent derived display text from invalidating a legitimate checkpoint or branch. Keep names as bounded display labels where that suffices; do not widen every text type indiscriminately.

### L010 — Checkpoint manifest count and text-field ceilings

**Current code:** `crates/app/peritus-product-runner/src/control/checkpoint.rs` — `UserCheckpoint::new`, `validate`, `CheckpointPath`, `exclusions`, `external_effects`; `control/checkpoint/restore.rs` — `RestoreOperation::settle`.

**Proposed minimum fix:** Remove the `u16::try_from` count admission checks from constructors and validation. Store exclusion path and reason separately, or add a path-bearing exclusion variant while reading legacy strings; a valid 4,096-byte path must not be squeezed into `ControlText<512>`. Match public checkpoint DTO admission when that consumer is mapped.

### L012 — Derived text can exceed otherwise valid field capacity

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/checkpoints/logical.rs` — `logical_rewind_branch` constructs `ConversationTitle::new(format!("Rewind of {}", record.title()))`; `checkpoints/capture.rs` — `empty_directory_exclusion`.

**Proposed minimum fix:** Fit the derived rewind label to the existing title byte limit at a UTF-8 boundary, or fall back to a stable short label. Use L010's structured exclusion instead of concatenating a full path into a 512-byte reason. User checkpoint/branch admission must not depend on decorative prefixes.

## Remove imposed archive quotas at configuration and enforcement together

Make logical quota/per-artifact policy optional in the existing store configuration, reservation and catalog contracts, and select no quota for persistent local memory. Preserve checked durable accounting, physical failures, finalized digests and referenced history.

### L057 — Local context archive, record, and artifact storage capacities

**Current code:** `crates/app/peritus-product-runner/src/local_context/storage.rs` selects 64-MiB artifacts and a one-GiB quota. `local_context/record.rs::{encode,decode}` independently imposes 32 MiB; storage records also decode with the production codec.

**Proposed minimum fix:** Remove those caller/record maxima and use matching writer/reader capacity, including the shared codec/state-install repair. Keep the same lineage and existing artifact references. Remove incidental archived call ID/name text caps without weakening their identity checks.

### L062 — Generic artifact quota admission and storage-location fences

**Current code:** `crates/state/peritus-artifact-store/src/config.rs::StoreConfig::new` requires positive numeric artifact/quota limits and quota within SQLite i64 range.

**Proposed minimum fix:** Add an explicit absent-quota policy and thread it through existing store admission. Keep real SQLite representation checks for stored sizes/accounting and protected storage location; a numeric sentinel is not unlimited.

### L065 — Logical artifact quota blocks writes independently of physical free space

**Current code:** `crates/state/peritus-artifact-store/src/quota.rs::{QuotaSnapshot::new,QuotaPlan::reserve}` and `catalog.rs::record_finalized` both compare totals against mandatory numeric quotas.

**Proposed minimum fix:** Apply quota comparisons only when explicitly selected, retaining checked totals. Do not rely on changing the local-context constant alone: both reservation and final catalog admission must support absence.

### L067 — Referenced and quarantined artifacts keep consuming the logical quota

**Current code:** `crates/state/peritus-artifact-store/src/catalog.rs::record_finalized` sums all recorded artifact bytes, including quarantined records.

**Proposed minimum fix:** Use the absent imposed-quota policy rather than delete referenced history to admit writes. Preserve reference-safe collection of genuinely unreferenced artifacts; reference/GC integrity is not the blocker being removed.

## Discard state size and interrupted preparation

Repair the existing discard transaction representation and preparation lifecycle. Preserve its checksums and foreign-file protection; no replacement journal is needed.

### L148 — Managed discard recovery adds a fixed 64 MiB whole-state ceiling

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/transaction/state.rs::{read,save}` imposes 64 MiB on a complete JSON state, inside a header with a checked u32 payload length.

**Proposed minimum fix:** Remove the 64 MiB policy check from both read and save. Stream serialization/checksum where practical and retain atomic save. The existing u32 header remains a real representation limit; widening beyond it requires a versioned header with a compatible old decoder, not an unchecked cast.

### L149 — Incomplete or changed discard preparation blocks restart cleanup with no repair operation in this module

**Current code:** `crates/app/peritus-product-runner/src/candidate/managed/transaction/owned.rs::Asset::check` rejects payload leaves with no saved seal; `own_directory` records ownership before payload preparation.

**Proposed minimum fix:** Record and resume preparation explicitly so an owned unsealed payload is recognized as interrupted work. Preserve it while completing/verifying preparation, or expose explicit abandonment of that owned preparation. Queue/retry owner contention with cancellation. Keep changed markers, foreign leaves and replaced parents untouched.

## Effect receipt capacity, compatibility and outcome reconciliation

Repair the existing receipt ledger and bind its command records to the shared command-owner repair. Never infer a failed command or safely repeat an effect merely because its completion receipt is missing.

### L165 — Durable effect receipts have fixed per-record and lifetime storage ceilings

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/receipt.rs` defines 2 MiB records/128 MiB ledger; `receipt/storage.rs::{append,read_bytes,decode_records}` enforces them; Applied and Completed duplicate output.

**Proposed minimum fix:** Remove record/lifetime quotas on both append and replay. Stream frames and reference an existing immutable full result for subsequent state transitions rather than duplicating it. Keep checked lengths, sync and action identity; retain control access and recovery evidence if actual persistence fails after an effect.

### L166 — An interrupted command receipt becomes ambiguous, with an epoch-wide barrier that survives acknowledgement

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/receipt.rs::{begin,prior_command_barrier}` makes Started command outcomes ambiguous and blocks cross-scope effects; `receipt/inspection.rs::acknowledge_uncertain_effect` records Reviewed without proving outcome.

**Proposed minimum fix:** Bind receipts to a stable command-owner/result identity and reconcile Started against that owner before choosing replay or resumption. This depends on the persistent native owner in L086/L094; no durable reattachment endpoint currently exists. Keep genuinely unknown effects blocked and require confirmed inactive ownership for acknowledgement; new user input alone must not manufacture a known result.

### L167 — Unknown receipt format versions are silently skipped during replay

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/receipt/storage.rs::decode_records` advances past unsupported versions without retaining them; `receipt/codec.rs::decode` accepts a version field.

**Proposed minimum fix:** Stop silently skipping unsupported receipt versions. Preserve their effect identities behind an explicit incompatibility error or decode a known historical version; repair only provably incomplete tails under the existing owner.

### L183 — Unknown command projection blocks retry and delivery controls without native outcome reconciliation

**Current code:** `crates/app/peritus-daemon/src/product_run/operation.rs::{project,acknowledge_command_outcome}` treats nonterminal Started as Running and terminal unresolved receipts as OutcomeUnknown, offering acknowledgement without native reconciliation.

**Proposed minimum fix:** Reconcile durable receipt identity with the persistent native owner/result from L086/L094/L166. Require proven inactive ownership for acknowledgement rather than inferring it from a terminal run phase; retain independent read/export controls for unaffected candidates while keeping unknown mutations fenced.

## Make patch preparation and cleanup idempotent under existing receipts

Retain owned preparation/completion facts before fallible filesystem work; keep preimages and exact transaction identity.

### L552 Patch preparation can leave a manifestless blocker and quarantine has only 100 names

**Current code:** `peritus-patch/src/transaction/apply.rs:62` creates the transaction directory before a durable manifest; cleanup errors are discarded and `recover.rs:298` tries only 100 quarantine names.

**Proposed minimum fix:** Persist preparation ownership in the existing manifest before staging payloads and recover provably owned interrupted directories. Retain cleanup errors; use collision-resistant quarantine names with cancellable retry rather than a hundred-name cutoff.

### L553 Patch cleanup removes the local completion evidence and repeated recovery fails on absence

**Current code:** `apply.rs:139` removes staging completion evidence and returns cleanup_pending; repeated recover requires the old directory before consulting any caller outcome.

**Proposed minimum fix:** Retain completion in the existing durable caller operation receipt before staging cleanup. Resolve repeated removal/parent-sync from that receipt, treating proven absence idempotently without a second ledger.

### L554 Patch filesystem work is synchronous and has platform and exact-observation restrictions

**Current code:** `transaction/filesystem.rs:34` fully reads files under MAX_FILE_BYTES and synchronous exact-path checks; apply rejects executable modes only on non-Unix platforms.

**Proposed minimum fix:** Apply shared file-size policy removal and streamed hashing, offload blocking work and observe cancellation at transaction safe points. Validate volume/mode support before effects, preserving preimages, disjoint roots, no-follow and unsafe-target checks.

## Artifact validation should follow the artifact contract

Remove file-size policy cutoffs and run large validation as cancellable owned work. Hard acceptance must come from the requested format/behavior rather than a blanket dialect or task guess.

### L172 — CSV validation has a fixed whole-file ceiling and a single mandatory dialect

**Current code:** `crates/app/peritus-product-runner/src/gates/artifact_csv.rs::{validate_file,validate,CsvParser::parse}` reads a whole 64 MiB-bounded UTF-8 comma-separated rectangular file.

**Proposed minimum fix:** Delete the 64 MiB CSV size rejection and validate incrementally with cancellation. Select delimiter, encoding and row-shape rules from the actual artifact contract rather than imposing one dialect on every CSV.

### L173 — JSON and YAML structural validation reject files above 16 MiB

**Current code:** `crates/app/peritus-product-runner/src/gates/{json_structure,yaml_structure}.rs::validate_file` applies 16 MiB checks and whole-file parsing; YAML also rejects empty documents.

**Proposed minimum fix:** Remove the 16 MiB JSON/YAML file checks and perform parsing off the async executor with cancellation, using streaming validation where available. Keep syntax requirements; require nonempty YAML only when the task requires it.

### L174 — SQLite qualification fixes file size, workspace shape, and repeatability policy

**Current code:** `crates/app/peritus-product-runner/src/gates/sqlite_migration.rs::{verify,discover_migrations,execute_file}` assumes schema.sql/migration.sql, a fresh database, two executions, optional rollback and 16 MiB SQL.

**Proposed minimum fix:** Remove the SQL byte ceiling and mandatory second migration execution. Use the task's actual migration paths and initial database, requiring repeatability or rollback only when requested; interpret explicit postcheck assertions and retain foreign-key checks.

### L175 — Source-readability gates still require whole-file UTF-8 and restrict the eligible ownership set

**Current code:** `crates/app/peritus-product-runner/src/gates/source_layout.rs::run` has no line-count ceiling; it filters metadata failures away, restricts owned source and reads complete UTF-8.

**Proposed minimum fix:** No line-count limiter remains here. Validate the language's actual encoding and report selected unreadable or missing paths instead of silently omitting them; stream or offload reads. Keep qualification scoped to the intended source set.

## Remove Git output/status/diff/history policy exhaustion

Use native path identity and complete retained observations, with paging or artifacts only for materialized views.

### L525 Git command byte ceilings fail whole operations and do not bound their duration

**Current code:** `peritus-git/src/command.rs:108` drains bounded stdout/stderr and reports a protocol failure after process completion if either overflowed.

**Proposed minimum fix:** Remove policy overflow as whole-operation failure; stream or spool complete output through existing command ownership. Preserve actual exit/mutation identity and cancellable pipe/reap handling; a large diagnostic must not invalidate successful work.

### L527 Git status rejects complete observations at fixed counts, paths and parser boundaries

**Current code:** `status/porcelain.rs:15` enforces status byte/entry quotas and rejects unborn HEAD; `status.rs:262` turns every write-tree failure into index_tree=None.

**Proposed minimum fix:** Remove status count/byte/path policy caps, parse incrementally with native path identity and represent unborn HEAD explicitly. Preserve actual write-tree failures and distinguish index conflict from corruption.

### L528 Structured Git diff and history have additional fixed, nonpaged observation ceilings

**Current code:** `diff.rs:141` builds complete name/patch output then rejects by requested capacities; `history.rs:55` imposes a hard maximum with no continuation result.

**Proposed minimum fix:** Remove independent path/patch quotas, stream or reference large patches and add exact commit continuation with a more-results indication. Preserve immutable base/target identity and precise incompatible-entry diagnostics.

### L529 Candidate creation and restoration scan every directory under a fixed entry limit

**Current code:** `snapshot/support.rs:169` scans every directory and rejects beyond MAX_SCAN_ENTRIES, including unrelated ignored trees.

**Proposed minimum fix:** Remove the 200,000-entry cutoff and restrict candidate inventory to relevant owned content. Keep cancellable inspection of nested repositories/control metadata and explicitly support registered nested repositories; restoration/cleanup must still protect unrelated ownership.

## Git tool results and supported catalog

Share byte-aware continuation with Git observation and filesystem rendering. Retain completed mutation facts before any fallible result formatting.

### L643 Git observation and rendering ceilings have no continuation protocol

**Current code:** `peritus-tools-git/src/{input,schemas,render}.rs` exposes bounded observations with first-500 items, 32 parents and 48 KiB patch windows but no continuation.

**Proposed minimum fix:** Add exact history/path/parent continuation and patch range or full artifact access, sharing L528/L636's encoded-byte paging. Keep preview completeness explicit and all retained data reachable.

### L644 Git mutation can succeed before its terminal envelope is rejected

**Current code:** `peritus-tools-git/src/dispatcher.rs` performs candidate/rollback then renders/assembles the result; mutation_outcome is held in an Option and protocol validation remains fallible.

**Proposed minimum fix:** Preflight representable result metadata and retain the exact outcome against the existing operation receipt before rendering. Retry publication/assembly without repeating mutation effects, using L582 recovery.

### L645 Registered Git merge is permanently unsupported; errors do not provide progress or retry detail

**Current code:** `peritus-tools-git/src/catalog.rs` registers git.merge while `dispatcher.rs::merge_unsupported` permanently rejects it; dispatch errors collapse causes.

**Proposed minimum fix:** Remove unsupported git.merge from advertised capabilities until implemented. Preserve typed Git/workspace recovery details and use owned cancellable execution for long work; avoid inventing merge machinery for this repair.

### L734 Production tool composition has a hard configured-count gate and closed route catalog

**Current code:** `peritus-daemon/src/component/tools/registry.rs::ToolComponents::build` rejects allowed.len()>MAX_CONFIGURED_TOOLS before checked_names and selects a closed catalog.

**Proposed minimum fix:** Remove the count cap only where it binds implemented catalog entries; keep exact implementation digests and explicit allowlisting. Remove nonexistent merge advertisement and preserve typed causes/lower-layer receipts.

## Page structured review and represent valid Git data without whole-review rejection

Share candidate/diff identities and continuation between server parsing, protocol pages and TUI navigation; keep comment anchors exact.

### L249 — Structured-review navigation stays on the loaded page and repeat refresh has no in-flight guard

**Current code:** `crates/app/peritus-tui/src/model/product/review.rs::refresh_review` always requests offset zero and has no pending-query guard; keys only navigate loaded collections. `review/state.rs` retains page/selection/drafts separately.

**Proposed minimum fix:** Request next/previous comment and diff pages and coalesce repeated refreshes under one pending query. Retain drafts and explicit stale-anchor rebind. Keep the existing mutation-owner/terminal-boundary check while allowing complete read access.

### L254 — Structured diff is a whole bounded object, while only comments are paginated

**Current code:** `crates/app/peritus-app-protocol/src/workbench/review.rs` sets 512 files/4096 hunks/32768 lines; review/page.rs::WorkbenchReviewPage::new requires the complete diff on every comment page and rejects larger collections.

**Proposed minimum fix:** Extend the query/page with file/hunk/line continuation bound to the same candidate and raw diff digest; parse only the requested bounded view instead of rejecting the whole diff at collection ceilings. Update daemon page selection and TUI navigation together. Retain per-page memory/frame limits and exact anchor binding.

### L255 — Valid Git quoted paths and long source lines can disable the entire structured diff

**Current code:** `crates/app/peritus-app-protocol/src/workbench/review/parser.rs::path_from_header` searches literal ` b/`; diff_path accepts only unquoted prefixes. `review/diff.rs` rejects line/header text over MAX_PRODUCT_DETAIL_BYTES or containing controls.

**Proposed minimum fix:** Decode Git C-quoted path bytes exactly before validating workspace-relative paths. Represent oversized/control-containing lines as safe ranged text tied to retained raw bytes and exact anchors, preserving full content for inspection. Reuse the diff paging repair rather than making one line fail the entire review.

## Durable request archives must match admitted requests

Align request encoding, manifest verification and artifact storage. Keep exact immutable request identity and incorporation; a separate archive quota must not strand an otherwise admissible turn.

### L180 — Durable request admission caps the entire serialized model request at 16 MiB

**Current code:** `crates/app/peritus-daemon/src/product_control/inputs/archive.rs::{new,verify_manifest}` imposes 16 MiB request/state and repeats source/media bounds; `inputs/files.rs::file_context` checks a fixed rendered context; `inputs/manifest.rs::messages` uses production codec limits.

**Proposed minimum fix:** Remove independent request/archive/manifest ceilings together with the shared codec, control-text and media repairs. Store large immutable bodies through existing artifact references and budget only the actual next provider view. Preserve message/media order, byte/digest identity and exact incorporated selections; change readers with writers.

## Project initialization source selection

Use exact source observations and reviewed preimages without imposing a small-file prerequisite.

### L188 — Project initialization rejects any selected source above 256 KiB and only discovers a fixed root-local set

**Current code:** `crates/app/peritus-daemon/src/product_control/init.rs::{discover_init,read_selected}` uses a fixed root-local SELECTED_SOURCES list and rejects above MAX_INIT_SOURCE_BYTES before reading.

**Proposed minimum fix:** Remove the 256 KiB selected-source rejection and read larger sources through existing ranges. Report discovery and parsing failures per source and allow explicit command selection; retain unverified proposals and exact preimage approval.

## Media admission and discovery

Remove duplicated host selection quotas together, then assemble explicit media against the selected provider's real capabilities and capacity. Preserve exact bytes, pixel validation and owned-path access. Decoder memory policy must be explicit; removing checks without controlling allocation is not a repair.

### L126 — Image attachments have independent host ceilings

**Current code:** `crates/app/peritus-product-runner/src/attachment.rs::ValidatedImage::decode` and `attachment/decode.rs::{limits,check_dimensions,validate_frames}` enforce encoded, count, side, pixel, frame and decoded-byte limits independently of provider capacity.

**Proposed minimum fix:** Remove fixed selection/count/encoded-size ceilings from host admission and their control-ledger callers. Replace fixed decoder ceilings with an explicit resource policy and incremental frame validation; keep actual decoder allocation failures and image integrity errors. Offer explicit frame selection or downsampling when needed instead of changing input silently.

### L142 — Managed workspace image discovery silently drops candidates and can stop the whole traversal on depth

**Current code:** `crates/app/peritus-product-runner/src/workspace_media.rs::{discover,discover_paths,attach}` truncates selection to 16 and breaks the traversal at depth/count; `workspace_media/folder.rs::discover_explicit` applies `.take(MAX_IMAGES)` before deduplication.

**Proposed minimum fix:** Skip only an over-depth branch, or remove the synthetic depth policy; remove silent discovery/selection cutoffs and deduplicate explicit paths first. Add continuation for large discovery results—the current traversal has none—and report skipped/unreadable media. Admit the chosen set using actual provider capacity and the shared media policy.

### L162 — Attachment count limits can block releasing held inputs, while pin overrides use a different selection path

**Current code:** `crates/app/peritus-product-runner/src/control/images.rs::ImageLedger::validate` and `control/files/ledger.rs::validate` cap selected eligible ledgers; `control/record/projection.rs::{eligible_images,eligible_files}` applies Pinned/Excluded preferences differently.

**Proposed minimum fix:** Remove host selection quotas from whole-record lifecycle validation. Compute one effective selection for provider admission after pin/exclusion resolution, using the shared media/text policy. Releasing a held caption must not fail because archived selection counts reached an arbitrary ceiling; retain image integrity and exact artifacts.

### L261 — External file ranges still require reading a complete source capped at 64 MiB

**Current code:** `crates/app/peritus-tui/src/file_import.rs::read` rejects a source over 64 MiB, buffers/hash-scans all bytes, then applies a 256 KiB selected range; resolve_lines uses u32. image_import uses the media path grouped above.

**Proposed minimum fix:** Stream the complete source digest while collecting only the requested range; remove the 64 MiB whole-source policy ceiling and share selected-text/media admission with host/protocol limits. For a selection too large for one transfer, use chunked artifact/range access rather than whole-file buffering. Keep exact source-change checks and regular-file/path authority.

## Reviewer evidence and text attachment capacity

Select the next provider view using its actual remaining context. Keep the complete underlying material available; a bounded prompt projection must not become a lifetime admission limit.

### L127 — Initial reviewer evidence is capped independently of available provider context

**Current code:** `crates/app/peritus-product-runner/src/turn/evidence.rs` estimates bytes per token and applies 50/75-percent targets plus a 384 KiB ceiling to weighted evidence.

**Proposed minimum fix:** Remove the independent ceiling and fixed percentages; budget evidence from actual remaining input capacity, including system/tools/output reservation. Preserve source identities and explicit omitted ranges, with archived conversation evidence retrievable rather than assuming fresh filesystem reads recover old conversation text.

### L128 — Explicit text attachments have separate selection admission ceilings

**Current code:** `crates/app/peritus-product-runner/src/attachment/text.rs` independently caps file bytes, aggregate bytes and reference count; it also validates UTF-8, controls and exact digest.

**Proposed minimum fix:** Remove those admission quotas and update the file-ledger callers consistently. Use range reads or artifact-backed retrieval for a provider view that cannot contain the complete selection. Retain exact source/digest and text validity checks.

## Attachment originals, ranges and paged history

Retain immutable originals and full selection history. Apply actual decoder/provider capacity to selected views and transfer pages, not to the lifetime/source metadata.

### L707 Brief projection limits exact proposals and rejects empty file observations

**Current code:** `peritus-app-protocol/src/workbench/brief.rs::WorkbenchBriefObservation::new` rejects bytes==0 and >64 MiB; proposal/page constructors add text/source limits.

**Proposed minimum fix:** Allow empty-file observations consistently and remove proposal/source-history quotas. Page exact proposals/observations with explicit user confirmation and disclosed omissions.

### L708 File and image history pages impose a 256-retained-reference ceiling

**Current code:** `peritus-app-protocol/src/workbench/{files,images}/page.rs` already checks exact 32-row coverage but also rejects total>256 and restricts offsets.

**Proposed minimum fix:** Remove total-reference and matching offset policy ceilings together, keeping exact revision fences, bounded pages and retained selection history.

### L709 File selection cannot address any source beyond 64 MiB and has a narrower import-label gate

**Current code:** `peritus-app-protocol/src/workbench/files.rs::WorkbenchFileRange::validate` caps byte/line coordinates at 64 MiB; preview/import metadata add source/label limits.

**Proposed minimum fix:** Remove original-source and range-coordinate policy caps, align import/workspace labels and keep selected provider chunks separate. Preserve source digest, exact range and confirmation binding.

### L710 Raster admission has fixed encoded, geometry, frame and format gates

**Current code:** `peritus-app-protocol/src/workbench/images/metadata.rs::new` caps original encoded size, 64 frames, 8192 dimensions and 16 Mi pixels.

**Proposed minimum fix:** Retain originals in existing artifacts; enforce actual decoder/provider limits only on selected views. Make frame selection/conversion explicit with provenance instead of rejecting original media categorically.

### L736 File and image consent archives add fixed 16 KiB proof ceilings and full-byte reconstruction

**Current code:** `peritus-daemon/src/product_control/storage/{files,images}.rs` reconstructs archived bytes; image proof verification imposes an independent 16 KiB limit.

**Proposed minimum fix:** Remove independent consent-proof/original-size gates and stream archived-content verification. Preserve exact consent digest, producing position and atomic proof publication.

### L748 File preview shares image decoder admission and repeats complete verification on confirmation

**Current code:** `peritus-daemon/src/product_run/workbench/files.rs::preview_workbench_file` consumes image_decodes capacity and runs uncancelable blocking prepare_file; confirmation repeats source verification.

**Proposed minimum fix:** Separate text I/O from raster decoder admission with fair cancellable work. Stream selected ranges and retain exact consent comparison plus committed-result replay.

### L749 Refresh-on-request reobserves every selected source and can repeatedly invalidate preparation

**Current code:** `peritus-daemon/src/product_run/workbench/files/refresh.rs::refresh_request_files` re-prepares each selected source and commits successive revisions.

**Proposed minimum fix:** Capture one coherent source snapshot and apply refresh changes together before request construction. Retry affected sources only and preserve their exact errors instead of restarting all preparation.

### L750 Brief projection skips larger replies and can reject a legitimate empty file

**Current code:** `peritus-daemon/src/product_run/workbench/brief.rs` skips replies once proposal count is full or WorkbenchInputText rejects their size; file observations inherit zero-byte rejection.

**Proposed minimum fix:** Allow empty source observations and page large exact proposals instead of excluding them at eight replies/8 KiB. Keep explicit acceptance and immutable author/source digests.

### L798 — Attachment previews select launcher role defaults instead of the active conversation's provider

**Current code:** `peritus-tui/src/model/chat/workbench/{images,files}.rs` chooses writer from product.launch defaults rather than chat_providers/selected run.

**Proposed minimum fix:** Resolve preview/confirmation through the active chat provider/model binding and invalidate on actual binding change. Preserve source digest/workspace/confirmation checks.

### L800 — Caption edits unnecessarily invalidate file previews, and multiline captions are rejected

**Current code:** `peritus-tui/src/model/chat/workbench/files/keys.rs` discards preview after every edit/cursor key and rejects all control characters in captions.

**Proposed minimum fix:** Invalidate only source path/range/mode or authoritative provider changes, allowing newline/tab captions. Accept a returning preview only when its complete request still matches.

### L807 — Web attachments are accepted at sizes that the durable message cannot send

**Current code:** `peritus-web/src/files/attachments.rs::stage` accepts 48 KiB snapshots/256 MiB cache; daemon/chat.rs::message embeds contents then rejects combined text above 8 KiB.

**Proposed minimum fix:** Send durable attachment handles through one staging/send admission contract. Add explicit snapshot removal of bytes plus metadata and remove retained-metadata quota as action admission.

## Complete checkpoint and initialization confirmation

Hash exact retained manifests incrementally and page their presentation; transfer capacity must not change what the user confirmed.

### L712 Checkpoint and rewind coverage use whole 65,535-item lists and production-sized preview hashing

**Current code:** `peritus-app-protocol/src/workbench/checkpoints.rs::validate_lists` limits whole coverage lists to u16; rewind constructor hashes a production-sized encoded preview.

**Proposed minimum fix:** Remove whole-list policy limits and stream the existing full confirmation fingerprint. Page display coverage while binding consent to all retained paths, exclusions/effects and preimage/conflict facts.

### L715 Initialization uses fixed whole-file source, patch and diff ceilings

**Current code:** `peritus-app-protocol/src/workbench/init/{proposal,render}.rs` embeds complete original/proposed content/diff and caps source/command lists and bytes.

**Proposed minimum fix:** Remove whole-source/diff policy ceilings, using ranged reads and existing exact patch/artifact references. Preserve managed markers, original bytes and user consent; reuse the initialization path.

## Preview capture and behavior evidence

Publish exact capture/output artifacts once, retain their bindings and retry publication/control independently of operation admission. Use bounded output tails only for display.

### L192 — Preview capture has a 16 MiB PNG ceiling and only an X11/ImageMagick backend

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/launch/capture.rs::capture_window` sets a ten-second helper timeout, enforces MAX_CAPTURE_BYTES, removes the file only after reading, then decodes without the shared guarded decoder; capability discovery is X11/ImageMagick.

**Proposed minimum fix:** Remove the synthetic ten-second helper deadline through the optional shared launch contract and remove the 16 MiB capture ceiling. Use the shared guarded decoder with explicit allocation policy, cleanup guards on every outcome and retry publication of the same capture. Report backend availability accurately; adding a new platform capture backend is separate from this minimum repair. Retain capture consent.

### L193 — Preview behavior evidence must be found in the retained stdout tail and only after terminal state

**Current code:** `crates/app/peritus-daemon/src/product_run/workbench/launch/output.rs::retain_output` retains 128 KiB tails; `launch/evidence.rs::{check_preview_behavior,qualify_graphical_goal}` requires terminal state, tail substring and live preview-map ownership.

**Proposed minimum fix:** Bind behavior checks to full retained output artifacts from the shared output-retention repair (L090), using exact offsets/digests. Permit observations while running where the criterion allows and qualify from durable process/build/capture facts after restart. Keep ordering and consent; the current tail is not a full artifact.

### L206 — Preview protocol exposes 8 KiB output tails and accepts collections beyond downstream process bounds

**Current code:** `crates/app/peritus-app-protocol/src/workbench/launch/output.rs` exposes 8 KiB tails; `launch/profile.rs::new` admits u16 collections but requires readiness_millis>0 and <=wall_millis, with InheritedHost network.

**Proposed minimum fix:** Keep bounded tails for display and expose full output artifact/range retrieval through L090/L193. Align profile and downstream launch admission, making readiness/wall expiry optional rather than forcing a synthetic execution deadline. Retain exact consent, native dimensions and Network authority for inherited-host execution.

## Retain complete command output and a live tail

Stream all output into the existing spool by default, while maintaining bounded live display/event pages.

### L090 — Output retention is a finite prefix, and its displayed tail freezes after quota exhaustion

**Current code:** `crates/runtime/peritus-process/src/supervisor/io.rs::accept_output` sends only accepted prefix bytes to spool/window/events. `output.rs::OutputAccounting::observe` drops bytes past stream/aggregate limits; `output/window.rs::RetainedWindow` therefore freezes at that prefix.

**Proposed minimum fix:** Use absent default spool/stream quotas from L087 and update the rolling display from every observed chunk. Keep exact dropped-byte reporting if an explicitly selected quota or physical failure occurs, and preserve publication causes for retry.

## Remove incidental process recovery and argument size caps

Widen matching writer/readers and preserve canonical checksums, native argv/environment semantics and exact identities.

### L095 — Process recovery records impose a 16-KiB canonical manifest bound

**Current code:** `crates/runtime/peritus-process/src/recovery/manifest/codec.rs::{encode,decode}` imposes 16 KiB and classifies new encoding overflow as corruption. Signal text is bounded at 128 bytes and encoded with u16 length.

**Proposed minimum fix:** Remove the aggregate and signal policy maxima, widening the signal length format compatibly where necessary. Preserve checksum/terminal binding and distinguish new-record admission errors from corrupt persisted data.

### L096 — Structured command/environment admission has fixed size and portability restrictions

**Current code:** `crates/runtime/peritus-process/src/command.rs::CommandSpec::new` caps executable/argument/count/total sizes. `environment.rs::{allowlisted,finish}` caps variables and folds names on every platform. `working_directory.rs::open` rejects non-Unicode paths.

**Proposed minimum fix:** Remove application argv/environment quotas and defer to real native admission with precise errors. Use platform-correct case handling and native Unix bytes through compatible canonical representation. Keep NUL, allowlist, authority and backend identity checks.

### L169 — Preview admission permits untimed operation but retains payload bounds and a narrower downstream process contract

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/preview.rs::PreviewCommand::new` currently requires a positive `Duration`, u16 collection admission, 4,096-byte program, 64 KiB argument/value and 512-byte key. This baseline does not yet support `None`.

**Proposed minimum fix:** Change preview timeout to explicit optional duration and carry it through the shared corrected launch/resource contract. Remove preview-only payload/count quotas and align downstream admission with L095/L096. Keep actual OS limits, NUL/name validity, nonzero terminal dimensions and specific launch errors; do not claim deleting a local check makes a mandatory downstream timeout optional.

## Full command evidence with bounded prompt previews

Keep durable full output and retrieve it by identity; prompt previews can remain bounded when omissions are explicit.

### L170 — Independent review receives a rolling, truncated command-observation window

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/evidence.rs::{record_named,render,merge_rendered}` retains 32/128 KiB preview records and only command/purpose for the unbounded successful list.

**Proposed minimum fix:** Keep full command-result references in the existing evidence records and let review retrieve them. Retain a rolling preview for prompt fit, with framing counted and omissions marked; do not make the preview the only retained evidence.

## Explicit external-reference navigation

Keep explicitly granted read authority while removing traversal dead ends and path-recognition mistakes.

### L168 — Explicit external-reference reads have fixed discovery and path-recognition bounds

**Current code:** `crates/app/peritus-product-runner/src/developer_tools/reference.rs::{list,explicit_absolute_path,names_match,task_tokens}` cuts off at 512, caps depth at six, strips terminal periods and applies case folding everywhere.

**Proposed minimum fix:** Add continuation to listing, enumerate incrementally and report child failures. Preserve quoted path punctuation and apply filesystem-appropriate case rules. Remove the independent path text ceiling coherently with native representation. Keep explicit external roots and symlink confinement; listing currently has no continuation cursor to reuse.

## Evidence bundle scale, history and completion

Export immutable provenance with its original identity, stream exact content and publish completion atomically. Separate historical evidence from claims of current authority.

### L621 Evidence and bundle limits reject legitimate scale, sometimes after output has already been written

**Current code:** `peritus-evidence/src/{record,manifest}.rs` and `bundle/{plan,assemble,verify}.rs` impose independent counts/bytes; assemble writes directly to caller output before later failure.

**Proposed minimum fix:** Remove fixed policy ceilings consistently, preflight exact encoded sizes and stream reads/hashes. Use owned temporary publication or explicitly retain partial-output status; preserve artifact digest verification.

### L622 Admitted causal history across revisions cannot be represented by the current portable bundle policy

**Current code:** `peritus-evidence/src/bundle/plan.rs::{build,plan_bundle}` requires one identical revision and Current freshness for every record, although causality admits historical parents.

**Proposed minimum fix:** Include historical parents at their original revisions as provenance. Require freshness only for records asserting current authority, preserving complete ancestry instead of rewriting history to one revision.

### L623 Evidence work repeats synchronous whole-history and artifact verification without a resumable work owner

**Current code:** `peritus-evidence/src/admission.rs` scans exported records/references; store and bundle paths repeat whole artifact verification and synchronous reads.

**Proposed minimum fix:** Recognize exact committed retries first, index immutable frame/reference lookups and reuse verification for the same snapshot. Stream cancellable hashing with explicit framed completion instead of unowned EOF waiting.

### L624 Evidence startup holds a write transaction for a full catalog scan and has no quarantine reconciliation operation

**Current code:** `peritus-evidence/src/sqlite/quarantine.rs::contain_corrupt_records` holds an Immediate write transaction across every active identity; public quarantine APIs expose reads/counts only.

**Proposed minimum fix:** Scan incrementally outside one catalog-wide writer lock and contain each verified corrupt record narrowly. Add explicit digest-checked reconciliation of repaired dependencies, retaining originals and identity fences.

## TUI drafts share durable input admission

Use the same referenced/chunked input contract for editing and sending; retain original intent through queue/order continuation.

### L784 — TUI composition duplicates whole drafts before its fixed task-size rejection

**Current code:** `peritus-tui/src/input/composer.rs::layout` lays out the complete draft; model/chat/keys.rs gates editing at MAX_PRODUCT_TASK_BYTES after copies.

**Proposed minimum fix:** Use shared referenced/chunked input and visible-range layout, avoiding full-paste copies before admission. Report rejected insertion; preserve UTF-8/sanitization and explicit slash-command intent.

### L787 — The composer accepts a larger draft than durable chat admission can represent

**Current code:** `peritus-tui/src/model/chat/workbench/conversation.rs::send_workbench_chat` converts the larger composer draft to an 8192-byte WorkbenchInputText and retains submission only in memory.

**Proposed minimum fix:** Use the same referenced-text admission for composer and durable queue. Give queue-to-execution continuation the existing operation identity and reconcile its launch on retry; preserve workspace/unarchive authority.

### L795 — Queue reorder requires every pending ID but the composer cannot hold the largest valid order

**Current code:** `peritus-tui/src/model/chat/workbench/queue.rs` parses every pending 32-hex ID from /queue order text, exceeding composer capacity for a valid large queue.

**Proposed minimum fix:** Accept an exact referenced order or shared incremental ordering operation against the inspected queue revision. Retain normal reorder receipts and identity-based paged selection.

## Web console custody and resumed output

Reuse owned process execution for console PTYs and retained bindings/output positions; gateway restart/disconnect detaches observers without destroying work.

### L808 — Web consoles are process-local and counted even after the CLI exits

**Current code:** `peritus-web/src/terminal.rs::start` caps consoles at 24, spawns transient PTY/reader owners and retains only 1 MiB output; consoles.rs lists ended children without automatic retirement.

**Proposed minimum fix:** Remove console-count gate and reap ended children. Use owned process service with saved console bindings/spooled offsets; move input/reaping outside map lock and support explicit close/cancel.

## Web observations and Git work with real continuation

Bound work as well as rendering with stable inventories/ranged reads, and retain Git mutation custody independently of request lifetime.

### L811 — Web paging and Git actions still wait for whole-content work

**Current code:** `peritus-web/src/files.rs::{list,text}` scans/sorts directories or reads whole files before paging; daemon.rs::runs collects every page; git.rs::execute kills at two minutes.

**Proposed minimum fix:** Return one run page per request, cache stable directory inventories and read requested text ranges. Remove Git's synthetic timer and use owned cancellable command execution with reconciliation; release project observation locks while retaining mutation custody.

### L812 — Web presentation and HTTP bounds are separate from execution lifetime

**Current code:** `peritus-web/src/config.rs::Preferences::parse` caps aliases/shortcuts=100, font 12–22 and explorer 180–480; title validation differs across API/native preparation.

**Proposed minimum fix:** Use one title type/byte-unit errors and remove arbitrary presentation/count restrictions where widgets support values. Keep operation grammar, chunked transport, origin/token and path confinement; these are not execution lifetime limits.

## Make large inspection and incomplete control parsing recoverable

Page or use wide logical positions for display; finish sanitizer state only at explicit stream/record boundaries, preserving inert output.

### L260 — Render scroll saturates at 65535 rows; an unterminated control string can hide later output

**Current code:** `crates/app/peritus-tui/src/render/product.rs::content_scroll_limit` saturates to u16; review/state.rs and terminal.rs use u16 scroll. `sanitize.rs::TerminalSanitizer` has no end-of-stream finish and can stay in OSC/String state indefinitely.

**Proposed minimum fix:** Use usize/u64 logical scroll or explicit pages, rendering a viewport without ratatui's u16 offset becoming a total-content ceiling. Add an explicit finish/reset operation at genuine stream/record end that emits an inert incomplete-control indication; retain parser state across ordinary chunks and never let source controls execute.

## Native path admission and per-entry directory failures

Keep relative-path confinement, no traversal, protected metadata, and no-follow access. Apply Windows naming restrictions where Windows actually requires them, and report an unsupported directory child without invalidating unrelated children.

### L008 — Path representation and all-or-nothing directory inspection

**Current code:** `crates/runtime/peritus-patch/src/path.rs` — `WorkspacePath::new`, `forbidden_byte`, `valid_component`, `windows_device_name`; `verified.rs` — `path_bounds_valid`; `crates/runtime/peritus-workspace/src/inspection.rs` — `list_directory`, `metadata_from`.

**Proposed minimum fix:** Remove portable depth/name restrictions that reject native-valid targets, updating the predicate contract with the implementation. Gate Windows alias/device checks by the target platform. Return supported directory entries plus explicit per-child unsupported-name/type diagnostics instead of propagating the first such child as failure of the entire listing; do not follow links or grant mutation authority.

## Windows selected capabilities and channel preparation

Admit only capabilities actually needed and proved, preflight before taking consumable preparations, and preserve exact native path/channel causes.

### L605 Windows capability gates rely on synchronous and incomplete probe evidence

**Current code:** `peritus-sandbox-windows/src/probe.rs::supported_features` uses broad baseline/resource gates; `native/probe.rs` reads the entire helper, infers facilities and reports credential_manager=true.

**Proposed minimum fix:** Check selected controls with real native evidence, omitting unused resource requirements. Stream cancellable helper verification and report precise unsupported capabilities before authority consumption.

### L606 Windows preparation consumes channel owners before later fallible compilation

**Current code:** `peritus-sandbox-windows/src/channels.rs::prepare_network` rejects a configured proxy for deny-all and takes proxy preparation; `preparation.rs` compiles terminal/manifest after channels.

**Proposed minimum fix:** Treat configuration as availability, selecting optional channels only when needed. Preflight terminal/manifest requirements before taking preparations and preserve owners/typed causes through later failure.

### L607 Windows path and ACL projection imposes workspace-shape and finite-root gates

**Current code:** `peritus-sandbox-windows/src/filesystem.rs::PathPolicy` counts before deduplication and constrains workspace/input shape; `filesystem/path.rs` uses ASCII folding and to_string_lossy for native paths.

**Proposed minimum fix:** Remove arbitrary root count gates after deduplication. Project deny-dominant permissions around protected roots and validate creation through existing parents using lossless native identity; retain reparse and authority checks.

## Optional updates and exact resumable staging

Remove optional update work from critical launch, retaining exact package identity and stage ownership through download, install and verification.

### L670 Optional startup update discovery remains on the critical launch path without cancellation or durable download continuation

**Current code:** `peritus-launcher/src/app.rs` awaits offer_on_startup; update/download.rs deletes staging and sets 10s connection, 30m total, 1 GiB and 5m extraction caps; update/install.rs sets 15m install/30s verify.

**Proposed minimum fix:** Run discovery independently and cancellably; remove synthetic execution/download deadlines and arbitrary archive ceilings. Retain exact resumable staging where HTTP range support is verified, use existing install receipts and verify the installed CLI/daemon pair.
