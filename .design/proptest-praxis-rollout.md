# Feature: Whole-codebase proptest-praxis investigation

## Summary

Apply the `proptest-praxis` skill to Peritus as a continuing correctness investigation. Build independent reference models and generated operation histories on a local discovery branch, run them extensively, shrink failures, and extract a deterministic regression and the smallest complete fix for each confirmed bug. Make execution cheap enough that most additional search consumes CPU rather than model turns.

This document records the original investigation design. The stage descriptions and exit criteria below are planning targets, not proof that every mechanism in the repository has been tested. Target selection used the manifests, navigation, selected implementations, and callers available at the planning snapshot; it was not a complete source review.

Planning snapshot: 2026-10-30, `develop` at `d106c9b13318fbcdd3d816ed63ae48532806fa7e`, with pre-existing uncommitted changes. The root manifest has **81 active Rust packages**, plus executable Python, JavaScript/TypeScript, shell, and PowerShell surfaces. Scope is this repository, including its module host and bundled resources. The separate Module Market repository and its Red Team implementation need their own campaign inventory.

## Delivery status as of 2026-11-01

Stages 1–8 were pursued as bounded property-testing campaigns. Commit `08296cb74` on `develop` integrates the runnable [property campaign](../discovery/proptest-praxis/README.md), owner-level checks, focused regressions, and the confirmed production repairs from earlier stages. Stage 8 adds analysis, telemetry, context-flow, and [policy/settlement properties](../discovery/proptest-praxis/stage8-policy/README.md); it found no new production defect in the checked cases. The integrated source passed all 42 fixed public campaign targets, Stage 8 package tests, and strict Clippy on the affected Rust packages. Generated run receipts and observation archives remain local and are not part of this design.

These results qualify the exercised mechanisms and revisions only. The acceptance criteria below still describe broader coverage and native/platform checks that require their own evidence before any whole-codebase correctness claim.

## User-visible behavior

The investigation changes no product defaults or available functionality. Confirmed fixes must preserve the existing supported behavior and improve the demonstrated incorrect case. Interactive steering, exact model/effort selection, continuation retrieval, cancellation, browser permissions, and recovery are explicit campaign targets.

Progress reports identify targets actually exercised, generator coverage, confirmed failures, fix status, and remaining platform gaps. A green build or a large case count is not a claim of whole-codebase correctness.

## Requirements

| ID | Requirement | Required evidence |
| --- | --- | --- |
| R1 | Account for every active package and executable language surface. | Manifest-derived inventory reconciled with the allocation below; every row has an owner and disposition. |
| R2 | Check intended behavior independently. | Written contract with source/caller references and a separately expressed model, interpreter, truth table, or exact expected result. |
| R3 | Explore histories and interactions, including failed operations followed by continued use. | Replayable operation AST, state-aware operand generation, adversarial patterns, and observed transition counts. |
| R4 | Make failures reproducible across generator changes. | Exact structured case or bytes, configuration, schedule, revision, fixture/tool identities, seed, and minimized witness. |
| R5 | Qualify the harness itself. | Deliberate behavioral defects detected in disposable worktrees, with restored baseline and separately classified build failures/timeouts. |
| R6 | Preserve current ownership, authority, compatibility, and resource contracts. | Relevant existing consumers and real-interface replay; no bypasses or new public APIs solely for testing. |
| R7 | Deliver each established defect independently of the broad search suite. | Deterministic regression failing before the fix, minimal repair, after-fix replay and generalized-family search. |
| R8 | Measure coverage instead of inferring it from silence. | Successful and rejected operations, depths/sizes, history combinations, fault hits, native surfaces, and explicit gaps. |
| R9 | Keep campaigns efficient and resumable. | Bounded search profiles, bounded worker/process ownership, reusable builds, machine-readable results, and resume commands. |
| R10 | Distinguish a contract violation from ambiguity, harness error, infrastructure failure, and unsupported environment. | Triage record naming the first divergence and the evidence behind its classification. |

## Acceptance criteria

Each stage has specific exits below. Additionally, a target is qualified only when its contract and caller/error paths have been read, its operations reach meaningful states, its independent oracle has detected representative deliberate defects, and its search/replay commands work from the recorded revision.

The initial whole-codebase sweep is complete when all 81 packages and additional executable surfaces have a justified disposition: calibrated search completed for a named profile, forwarding behavior covered through a named owner/consumer, or an explicitly unresolved native/environment/contract gap. A gap remains incomplete coverage; assigning it a label does not turn it into a pass. Repository membership drift must be reconciled before reporting completion.

No minimum number of bugs is required. Every confirmed finding must be fixed and verified or explicitly remain open in the report. A seed without a readable deterministic counterexample is not a completed bug deliverable.

## Current architecture

The human coding path is CLI/launcher or UI -> daemon `ProductRunService` -> `ProductRunner` -> `DeveloperLoop` -> provider and host-owned tools. The journal-backed `AgentDriver` and `OrchestratorDriver` have separate state and recovery ownership. Their models must remain separate, even when they share underlying types.

Current repository facts that affect this plan:

- [Root manifest](../Cargo.toml) pins Rust 1.97.1 and declares the 81 packages. [Cargo configuration](../.cargo/config.toml) disables incremental builds; [xtask dispatch](../xtask/src/cli.rs) currently handles packaging/install/release commands.
- [README real-testing policy](../README.md) retires repository unit suites, external qualification gates, and formal proof tooling. This requested investigation does not restore those retired systems. Broad property machinery stays on the discovery branch; extracted regressions can be standalone real consumers or focused owner tests where needed for a demonstrated defect.
- At the planning snapshot, no `proptest`, `quickcheck`, `proptest-state-machine`, or `fast-check` entry was found in the root lock/manifest and WebUI package manifests. The dependencies and commands below were proposals; use the delivered manifests and campaign README for current commands.
- [WebUI package scripts](../webui/package.json) currently provide `dev`, `check`, and `build`, with Playwright available, but no property runner.
- [Earlier bug-discovery design](proactive-bug-discovery.md) is useful background but references retired packages and absent owners such as `peritus-test-support/src` and `xtask/src/ci_shard.rs`. Revalidate each reusable fixture; do not implement this plan against those historical paths.
- At the planning snapshot, the checkout had web/browser work, lockfile churn, and navigation edits. Stage 1 needed a reproducible, complete source snapshot without discarding or silently including that work. The locked check then failed because the new `httpdate` dependency was absent from the `peritus-tools-web` lock entry; Chrome bridge sources/assets also existed under local ignore rules. These were baseline issues, not findings from a property campaign.
- At the planning snapshot, [context configuration](../crates/app/peritus-product-runner/src/context_config.rs) defaulted to `History` decision views and disabled read-only plans. Test explicit modes and composition overrides; that old snapshot does not establish current defaults.

## Proposed design

### 1. Discovery layout and delivery model

Use a local branch such as `discovery/proptest-praxis`, based on the chosen complete development snapshot. Give each implementation worker its own worktree and bounded target ownership. The coordinating agent combines planning and orchestration; normally allow two search/implementation workers and one independent reproducer/reviewer, within the available four-agent capacity. Avoid concurrent Cargo jobs multiplying memory use.

Proposed discovery-only files:

| Location | Purpose |
| --- | --- |
| `discovery/proptest-praxis/README.md` | Setup, search profiles, exact replay commands, resource limits, and branch/base identity. |
| `discovery/proptest-praxis/targets.json` | Authoritative package/surface -> contracts -> harness -> current evidence ledger. |
| `discovery/proptest-praxis/support/` | Small shared replay schema, deterministic schedules, metrics and temporary-resource ownership. No production semantic oracle here. |
| `discovery/proptest-praxis/cases/` | Sanitized minimized structured witnesses and deliberate-defect patch descriptions. |
| Owner-crate investigative modules | Tests of private production interfaces, gated by `cfg(test)` on the discovery branch. Add only the required dev dependencies there. |
| Standalone consumers under the discovery tree | Public library, CLI, daemon/IPC, PTY, HTTP, browser, Python/JS and installer campaigns. |

These paths were proposed before implementation. The delivered layout and commands are in the [campaign README](../discovery/proptest-praxis/README.md). Begin with ordinary compositional Rust `proptest` strategies; add `proptest-state-machine` only if state-dependent transitions justify another dependency. Qualify and lock dependency versions against the pinned toolchain during Stage 1. Do not add runtime dependencies to ship broad search machinery.

For WebUI and extension JavaScript, use a discovery-only locked JS project with `fast-check` and the existing browser tooling. For Python, prefer generated scenario JSON driven through real drivers; use a locked discovery-only Hypothesis environment where Python-native state machines materially improve shrinking. Shell and PowerShell installers consume generated fixture/scenario data in disposable environments rather than duplicating their logic in Rust.

Do not expose private structs publicly to make an external harness compile. Use an owner-crate investigative module, an existing narrow port, or the actual public interface. Preserve existing Cargo architecture boundaries. Thin forwarding packages can be covered through named consumers and owners instead of receiving artificial wrapper tests.

### 2. Target and evidence ledger

Each target record contains package/path, source content digest, contract/callers/error paths read, operation variants, independent oracle description, access strategy, dependencies, features/platform, owner/worktree, exact command, fixture version, last tested revision, coverage counters, deliberate-defect outcomes, witnesses, fixes, and unresolved gaps.

Use explicit statuses: `inventoried`, `contract-read`, `harness-runnable`, `calibrated`, `searched`, `failure-triaged`, and `fix-verified`, with separate `blocked`/`native-pending` fields. Record interface/source reading separately from generated behavioral coverage. A search result cannot automatically advance a native integration status.

For every run preserve schema version, production SHA and dirty-content identity, discovery SHA, generator/oracle versions, platform/tool versions, initial fixture, configuration, exact operations/bytes, fault schedule and reached points, seed, first divergence, metrics, command and resource outcomes. Keep original and shrunk witnesses. Reject incompatible replay schemas explicitly. On source changes, rerun affected properties and canaries; do not carry an old pass onto a new revision.

### 3. Generator and oracle rules

Use explicit operation enums with logical object IDs mapped separately to production handles. Choose operands using reference state, retaining fresh, missing, removed, duplicate and stale identities. A defective production result must not select the future test space. Make shrinking preserve valid references, or report missing references explicitly rather than silently skipping most commands.

Generate valid structures constructively, then introduce one targeted violation. Partition empty/singleton/boundary values, same-size unequal replacements, cycles/diamonds/sharing, exact limits and limit ±1, Unicode bytes, integer extrema and terminal states. Combine random histories with warm -> mutate -> read, remove -> reinsert, snapshot -> mutate either branch, fail -> continue, and restart -> reconcile. Vary growth-heavy, reuse-heavy, read-heavy and mutation-heavy mixes.

Independent models use ordinary lists/maps/sets, explicit truth tables, BFS/full recomputation, small exhaustive enumeration, or checked wider arithmetic. Production `model.rs`, `verified.rs`, invariant validators, canonicalizers and replay functions are specification references or supplementary assertions, never the only oracle. Round trips alone cannot detect a serializer and decoder sharing the same wrong field layout.

Read operations are explicit history steps. Some observers warm caches; choose observation schedules deliberately so assertions do not erase lazy-state bugs. Check completeness as well as soundness: all required reachable nodes must appear, all source bytes must remain retrievable, and no accepted input or finding may silently disappear.

Sequential schedules qualify pure reducers. For concurrent behavior, introduce explicit barriers at the owning admission/commit/dispatch boundary, enumerate small competing schedules, and use real child termination for process-crash cases. Distinguish cancellation request, observation and acknowledgment. A returned I/O error is not a process crash, and SIGKILL is not a power-loss durability test.

### 4. Stages and concrete completion gates

#### Stage 1 — Reproducible discovery foundation and first searches

1. Freeze a complete baseline and preserve existing work; record tracked, untracked and ignored build inputs. Reconcile the lockfile only in the selected campaign snapshot. Verify a fresh checkout can build without local-only assets.
2. Expand the package allocation below into contracts, current callers, error paths, source access and runtime requirements. Inventory non-Rust executable surfaces and archived versus active fixtures.
3. Implement replay, failure persistence, profile selection, behavioral counters and deterministic temporary-resource cleanup with the smallest shared support needed.
4. Build three early pilots: provider SSE/NDJSON framing, working-context dependency closure, and telemetry buffer accounting. Run them as soon as each is usable, while the remaining infrastructure is developed.
5. Calibrate each pilot with a plausible defect in an isolated worktree: dropped frame/finish event, omitted dependency edge, or wrong drop/export counter. Verify a named behavioral assertion catches it, then remove the deliberate defect and replay the witness.

Exit: reproducible source/dependency snapshot; all active owners assigned; three runnable calibrated independent pilots; exact commands and explicit cases replayed from fresh fixtures; build/search timing and memory/disk baselines recorded. No product defect needs to be invented to finish this stage.

#### Stage 2 — Context, memory and efficient inspection

Extend pilots to working memory, dependency invalidation, run knowledge, retrieval, role filtering, read reuse, conservative decision compilation and sequential read-only plans.

Primary entrypoints: [working selection](../crates/orchestration/peritus-context/src/working/selection.rs), [working invalidation](../crates/orchestration/peritus-context/src/working/invalidation.rs), [knowledge plan](../crates/orchestration/peritus-run-knowledge/src/plan.rs), [knowledge deltas](../crates/orchestration/peritus-run-knowledge/src/delta.rs), [memory retrieval](../crates/orchestration/peritus-memory/src/retrieval/plan.rs), [read reuse](../crates/app/peritus-product-runner/src/local_context/read_reuse.rs), [decision compiler](../crates/app/peritus-product-runner/src/local_context/assembly/decision/mod.rs), and [plan execution](../crates/app/peritus-product-runner/src/local_context/operation_plan/execution.rs).

Use independent graph closure and staleness recomputation, a simple interval union for model-visible read bytes, a current-source map, and a tiny read-plan interpreter. Generate candidate/role/input/permission/source changes, same-size content edits, missing archives, stale cursors and restart. Cross History/Conservative views and enabled/disabled plans explicitly. Compare read-plan calls with ordinary individual reads under the same admission schedule.

Exit: required closure, role isolation, exact retrieval, no stale reuse, complete response accounting, per-child current authority and stop-on-error/control-change are checked; every tested locator resolves to its exact bytes. Capacity accounting follows current provider semantics; no double subtraction of output allowance. Optional selection need not be globally optimal or have monotonic membership unless the API promises it. Model answers themselves are not compared for equality.

#### Stage 3 — Interactive steering, run lifecycle and settlement

Build a list-based input ledger and separate phase models for interactive runs and durable domain agents. Exercise input enqueue/edit/hold/withdraw/reorder/correction, model/effort updates, request preparation, tool admission, cancellation, finalization, retry, crash/restart, candidates, findings, obligations, gates and settlement.

Primary entrypoints: [input transitions](../crates/app/peritus-product-runner/src/control/inputs/transition.rs), [live interaction](../crates/app/peritus-daemon/src/product_run/interaction/live.rs), [provider selection](../crates/app/peritus-daemon/src/product_run/catalog.rs), [model updates](../crates/app/peritus-daemon/src/product_run/interaction/models.rs), [daemon lifecycle](../crates/app/peritus-daemon/src/product_run/lifecycle.rs), [product finding ledger](../crates/orchestration/peritus-review/src/product/ledger.rs), [settlement reducer](../crates/orchestration/peritus-run-settlement/src/reducer.rs), and [AgentDriver](../crates/orchestration/peritus-agent/src/runtime/driver.rs).

Run real disposable daemon/IPC sessions with a recording provider, explicit release barriers, receipt inspection and an independent external effect counter. Trace accepted, admitted and incorporated input revisions separately. Check that a request retains its selected model/effort while later updates apply at the documented boundary. Treat no-selection/unsupported-choice behavior according to the current contract; ambiguous desired changes are not silently introduced as tests.

Exit: generated steering-versus-dispatch/finalization schedules account for every accepted input; stale tool admissions yield; completed effects are not repeated; uncertain effects remain truthful; candidate and terminal settlement are exact. Reviewer omission/fixer proposal cannot by themselves resolve untouched findings. ProductRunService and AgentDriver each have independent recovery evidence.

#### Stage 4 — Durable storage and domain coordination

Cover journal, projections, artifacts, evidence, leases, approvals and migrations together with scheduler, collaboration, harness and module state. Use pure models first, followed by actual disposable SQLite/filesystem replay and controlled failure windows.

Primary entrypoints: [journal append](../crates/state/peritus-journal/src/sqlite/append.rs), [projection replay](../crates/state/peritus-projection/src/replay.rs), [artifact GC plan](../crates/state/peritus-artifact-store/src/gc_plan.rs), [migration plan](../crates/state/peritus-migrations/src/plan.rs), [scheduler selection](../crates/orchestration/peritus-scheduler/src/selection.rs), [collaboration reducer](../crates/orchestration/peritus-collaboration/src/reducer.rs), [harness materialization](../crates/orchestration/peritus-harness/src/materialization/planner.rs), and [module jobs](../crates/orchestration/peritus-modules/src/jobs.rs).

Reference models: append-only event list plus exact command map; fold/rebuild against independently computed semantic state; owned byte/reference set; versioned lease table; signature-backed approval table; explicit migration interval; exhaustive feasible work/worker pairs and resource totals; parent-map join semantics; path-to-content materialization; ordered module stages.

Exit: exact duplicate versus conflicting replay, every-attempt observation invalidation, external SQLite commits, atomic boundaries actually promised, projection generation publication, conservative GC, lease fencing/expiry, approval binding, migration retry, reservation conservation, descendant cancellation, and stage checkpoint publication are exercised. Reopen is real. Archived migration files do not imply supported upgrade paths. Distinguish source-package installation from hosted module execution.

#### Stage 5 — Protocols, providers and extension boundaries

Expand Stage 1 framing to canonical frames, every application request/response/event/control family, model response reduction, tool protocol/schema, plugin SDK/host and MCP. Exercise all five provider adapters through their actual decoders and a scripted byte/loopback transport.

Primary entrypoints: [frame codec](../crates/foundation/peritus-codec/src/frame.rs), [canonical reader](../crates/foundation/peritus-codec/src/reader.rs), [application wire](../crates/app/peritus-app-protocol/src/wire/mod.rs), [response reducer](../crates/model/peritus-model-protocol/src/reducer.rs), [provider framing](../crates/model/peritus-provider-core/src/framing/sse.rs), [effort selection](../crates/model/peritus-provider-core/src/effort.rs), [plugin quota](../crates/runtime/peritus-plugin-host/src/quota.rs), and [MCP framing](../crates/extensions/peritus-mcp/src/framing.rs).

Generate valid message ASTs with known semantics and grammar-aware malformed variants. Vary chunk splits, duplicate/conflicting IDs, exact bounds, truncation, partial tool JSON, usage updates, cancellation, extra terminal events, quota exhaustion and subprocess loss. Providers are compared only on the semantic subset each supports; compatible Chat/Responses and Google protocol modes each get rows. Include account-runtime adapters found during the source inventory.

Exit: all enumerated wire variants and provider modes are reached or explicitly unsupported; partial/invalid output cannot dispatch a tool; terminal state and identities are exact; request model/effort remains selected; normalized usage follows the contract. Chunk invariance is asserted only for admissible schedules: the framing buffer may legitimately reject a giant chunk before draining. Encoding/decoding agreement is supplemented by semantic expectations and retained compatibility corpus cases.

#### Stage 6 — Authority, filesystem and host execution

Test foundation policy/budget/kernel contracts, router replay, patch/workspace/Git transactions, FS/shell/quality adapters, process output/ownership, network, secrets, local sockets and sandbox plans/backends.

Primary entrypoints: [budget transition](../crates/foundation/peritus-budget/src/transition.rs), [policy evaluation](../crates/foundation/peritus-policy/src/decision.rs), [kernel reducer](../crates/foundation/peritus-kernel/src/reducer.rs), [tool router](../crates/tools/peritus-tool-router/src/router.rs), [patch transaction](../crates/runtime/peritus-patch/src/transaction/apply.rs), [patch recovery](../crates/runtime/peritus-patch/src/transaction/recover.rs), [workspace mutation](../crates/runtime/peritus-workspace/src/mutation.rs), [process output](../crates/runtime/peritus-process/src/output.rs), and [network matching](../crates/runtime/peritus-network/src/matcher.rs).

Models: independently summed reservation tree with checked wider arithmetic; declarative authority matrix/time windows; explicit kernel phase table; exact action-to-call replay map and dispatcher effect count; virtual file tree with actual bytes/modes; raw Git state; exact argv fixture; retained-byte process model; IP/name/port rules; synthetic secret/lease model.

Exit: rejection atomicity where promised, replay still checks current authority, changed call identity is rejected, partial writes account for the written prefix, owned processes are reaped, unrelated canaries survive, and native allowed/denied operations match independently expected capabilities. Patch cancellation after the first mutation is deferred according to its contract. Installation or multi-file operations are not asserted globally atomic without that promise. Linux, macOS and Windows enforcement require distinct native evidence; cross-compilation qualifies compilation only.

#### Stage 7 — Clients, browser state and installation

Cover CLI argument grammars, TUI editor/event loops, WebUI asynchronous operations/drafts, gateway/client reconnect and reconciliation, Python browser/desktop driver, Chrome extension/native bridge, launcher/onboarding and package/install/upgrade/remove scripts.

Primary entrypoints: [CLI web arguments](../crates/app/peritus-cli/src/web.rs), [TUI paste](../crates/app/peritus-tui/src/model/editor/input.rs), [client request validation](../crates/app/peritus-app-client/src/request.rs), [gateway operations](../crates/app/peritus-web/src/operations.rs), [WebUI workspace](../webui/src/lib/workspace.svelte.ts), [conversation order](../webui/src/lib/conversations.ts), [browser driver](../crates/tools/peritus-tools-computer/src/browser.py), [web sources](../crates/tools/peritus-tools-web/src/source.rs), [install bootstrap](../install.sh), and [package owner](../xtask/src/product_package.rs).

Use an independent text/cursor model, per-project/session draft and operation ledger, deterministic pending-response schedule, full-text source/cache/time model, and installation tree with user-data and sibling canaries. Replay minimized traces through a PTY, disposable browser and real daemon. Generate both language-native and cross-language frame/session cases. Inventory ignored bridge assets explicitly so replay never depends on untracked local files.

Required seeds include the short pasted `Shared https://doc.qt.io for reading.` message; middle-click, keyboard clipboard and bracketed paste remain distinct native event paths. Include Unicode byte boundaries, oversize rejection retaining the draft, decimal sequence IDs above JS `2^53`, stale responses after a project switch, draft edits during send, tab revocation/navigation, challenge HTML at HTTP 200, redirect-to-cooldown, and missing CLI operands.

Exit: exact input preservation and bounded responsiveness, correct async operation reconciliation, stale browser observations rejected, selected HTTPS consent enforced, unknown HTTP status retained as unknown, failed pages never stored as successful evidence, retry/backoff honored, installation errors truthful and retries preserve user data. Native clipboard/desktop/supervisor/locked-executable checks are recorded per OS and cannot be replaced by a model-only result.

#### Stage 8 — Analysis, reporting, cross-system closure and continued search

Complete the remaining analysis/reporting packages and test the combinations most likely to hide faults: read archive -> decision view -> plan -> receipt -> restart; accepted input -> model selection -> request -> child dispatch; journal -> projection -> trace -> telemetry; review -> obligations/gates -> candidate settlement; package/module installation -> stage execution -> recovery.

Primary entrypoints: [debugger clustering](../crates/analysis/peritus-debugger/src/clustering/engine.rs), [pass-at-k](../crates/analysis/peritus-eval/src/statistics/pass_at_k.rs), [distribution statistics](../crates/analysis/peritus-eval/src/statistics/distribution.rs), [evolution selection](../crates/analysis/peritus-evolution/src/selection/engine.rs), [telemetry buffer](../crates/observe/peritus-telemetry/src/buffer/state.rs), [telemetry pump](../crates/observe/peritus-telemetry/src/export/pump.rs), [quality acceptance](../crates/foundation/peritus-quality-policy/src/evaluator.rs), and [release evaluation](../crates/foundation/peritus-release-policy/src/evaluator.rs).

Use exact small combinatorial probabilities with specified fixed-point rounding/error bounds, direct sorted statistics, independently enumerated eligible variants and objective ordering, explicit cluster equivalence classes, causal span maps, and a simple queue/disposition table. Generate exporter failure/wrong acknowledgment/retry/checkpoint/restart histories; preserve `submitted = queued + exported + dropped` under the API's actual accounting definitions, and the distinction between historically accepted records and rejected arrivals.

Exit: statistical boundaries, evidence freshness, denied/unavailable criteria, stable selection, finding/citation conservation, telemetry final-disposition prefixes and report/manifest identity all have independent evidence. The package/surface ledger reconciles to the current tree, every confirmed failure has its extraction status, and the final report states remaining gaps. Established harnesses remain runnable; findings drive related-mechanism searches after the first sweep.

Stage order is dependency-aware, not a requirement to idle independent searches. After Stage 1, pure lanes from Stages 2, 4, 5, 6 and 8 can run concurrently; Stage 3/7 real-interface work reuses those fixtures and requires the owning runtime baseline. Deliver each stage independently with its exit evidence.

### 5. Complete package allocation

This table accounts for all 81 currently active packages. Entries are assignments, not claims that their complete implementations were read or tested. Stage 1 expands them into per-mechanism rows; each substantive parser, cache, reducer and persistence boundary needs a disposition. The earliest relevant stage owns the first campaign; integration stages revisit consumers.

| Packages | Main mechanisms / oracle | Stages |
| --- | --- | --- |
| `peritus-types` | Checked identities, numbers, capability names, equality/order and overflow; explicit primitive expectations. | 5, 6 |
| `peritus-codec`, `peritus-protocol` | Canonical primitive/frame and domain-family grammar; independent bytes and semantic records. | 5 |
| `peritus-budget`, `peritus-policy`, `peritus-kernel` | Reservation conservation, exact authority/time/use limits, causal lifecycle table. | 6 |
| `peritus-spec` | Gate graphs, exact acceptance bindings, canonical input rejection; small graph enumeration. | 3, 6 |
| `peritus-quality-policy`, `peritus-security-policy`, `peritus-release-policy` | Independently enumerated required evidence, exact revision/freshness and review/waiver truth tables. | 6, 8 |
| `peritus-tcb` | Static exported path vocabulary; document-only/historical paths classified explicitly; no artificial random algorithm suite. | 1, 8 |
| `peritus-plugin-sdk` | Canonical JSON, bounded framing, manifest/module/host-service envelopes and request bindings. | 5 |
| `peritus-journal`, `peritus-projection` | Append/CAS/replay/outbox/observation invalidation and rebuild/shadow publication; log model. | 4 |
| `peritus-artifact-store`, `peritus-evidence` | Chunk/finalize/quota/GC and causal/freshness/bundle completeness; bytes/reference/DAG models. | 4 |
| `peritus-leases`, `peritus-approval` | Ownership/fencing/version/time and authenticated exact one-use transitions. | 4, 6 |
| `peritus-migrations` | Supported descriptor intervals, capacity/backup, interrupted apply/restart. | 4 |
| `peritus-context`, `peritus-memory`, `peritus-run-knowledge`, `peritus-role` | Closure/retrieval/invalidation/exact reuse and role matrix. | 2 |
| `peritus-agent`, `peritus-orchestrator` | Separate pure/durable driver phase tables, usage and outbox/effect ownership. | 3, 4 |
| `peritus-gates`, `peritus-review`, `peritus-obligations`, `peritus-run-settlement` | Exact check multiset, finding/provenance conservation, public-clause evidence, single settlement. | 3, 8 |
| `peritus-scheduler`, `peritus-collaboration` | Feasible assignment/resource enumeration and independent join/descendant model. | 4 |
| `peritus-harness`, `peritus-modules` | Component graph/revision materialization and package/job/stage/reopen identities. | 4, 8 |
| `peritus-model-protocol`, `peritus-provider-core` | Request/response state, usage high water, framing, cancellation/retry and effort bindings. | 1, 5 |
| `peritus-provider-openai`, `peritus-provider-anthropic`, `peritus-provider-google` | Provider-specific request/stream grammars and normalized expected events. | 5 |
| `peritus-provider-compatible`, `peritus-provider-opencode` | Explicit protocol/profile modes, account transport and malformed-output recovery. | 5 |
| `peritus-tool-protocol`, `peritus-tool-router` | Schema/call/result validity, exact authority/replay identity and actual dispatch count. | 5, 6 |
| `peritus-tools-fs`, `peritus-tools-git` | Tree/path/bytes/digest/page/search and raw Git correspondence. | 6 |
| `peritus-tools-shell`, `peritus-tools-quality` | Exact argv, command identity, check discovery, complete output/parser verdicts. | 6 |
| `peritus-tools-web`, `peritus-tools-computer` | Immutable sources, cache/time/redirects/paging, session/observation/browser/desktop state. | 6, 7 |
| `peritus-patch`, `peritus-workspace`, `peritus-git` | Real file/Git trees, transaction preimages, candidates, rollback and reconciliation. | 6 |
| `peritus-process` | Output/accounting/retention, process and PTY lifecycle, exact recovery identity. | 6 |
| `peritus-sandbox`, `peritus-sandbox-linux`, `peritus-sandbox-macos`, `peritus-sandbox-windows` | Independent capability matrix plus separately qualified OS enforcement and cleanup. | 6 |
| `peritus-network`, `peritus-secrets`, `peritus-local-socket` | DNS/IP/redirect/accounting, synthetic secret leases/redaction, path/custody/ownership boundaries. | 6 |
| `peritus-plugin-host`, `peritus-mcp` | Trust/quota/lifecycle, framing/JSON-RPC/initialization/cancel; downstream authority remains enforced. | 5 |
| `peritus-app-protocol`, `peritus-app-client` | Every wire payload variant, response correlation, interrupted write/reconnect/uncertainty. | 5, 7 |
| `peritus-product-state`, `peritus-product-runner`, `peritus-daemon` | Product state/records/inputs, context/plans/receipts, steering/recovery/candidate settlement. | 2, 3, 4, 7 |
| `peritus-cli`, `peritus-tui`, `peritus-web` | Argument grammar, UTF-8 editor and event paths, gateway/operation reconciliation. | 7 |
| `peritus-launcher`, `peritus-provider-onboarding` | Bootstrap/setup/choice persistence, credential-store failure and native lifecycle. | 7 |
| `peritus-debugger`, `peritus-eval`, `peritus-evolution` | Causal selection/citations/clusters, exact statistics/accounting, eligible variant/promotion/rollback. | 8 |
| `peritus-trace`, `peritus-telemetry` | Causal span/event projection, content boundaries, buffer/drop/export/checkpoint invariants. | 1, 8 |
| `peritus-external-benchmarks`, `peritus-platform-qualification` | Result parsers, evidence/verdict accounting and native observation truthfulness; active sources only. | 7, 8 |
| `peritus-release-artifacts`, `peritus-release-operator`, `xtask` | Manifest/archive/version/digest identity, truthful package/install/release bookkeeping. | 7, 8 |

Additional surfaces: `webui/` Svelte/TypeScript; browser/desktop Python and extension JavaScript; `install.sh`, `install.ps1`, `packaging/` scripts; executable local integration hooks under `.claude/`; build/resource loaders for bundled `skills/`; schema/compatibility generators and retained fixture ingestion. Static docs and archived evidence receive provenance/link/resource-consumption checks where relevant, not invented property suites. Inventory executable entrypoints, including scripts outside these initial groups, before closing Stage 1.

### 6. Search allocation and runtime profiles

Start with explicit seed sets and bounded exhaustive small cases, then varied random seeds. Initial tuning targets, to be replaced by measurements:

| Profile | Starting size / purpose | Execution bound |
| --- | --- | --- |
| Smoke/replay | All retained witnesses; about 256 generated cases per cheap property, 1–64 operations. | Aim for under 2 minutes per warmed target group; emit an incomplete result on timeout. |
| Standard | About 10,000 cases for cheap properties; 1–256 operations with several operation mixes. | About 15 minutes per target; record actual generated/rejected/reached counts. |
| Extended | New seeds, longer histories up to about 2,048 operations, small exhaustive oracles separately. | About 1 hour per process with resumable batches and bounded logs. |
| Native/fault | A smaller matrix of explicit schedules and generated traces through real children/OS/browser. | Per-case watchdog and target budget measured in Stage 1; do not multiply slow external runs blindly. |

Case count is an input, not a completion criterion. Record operation frequencies, success/rejection/no-op ratios, maximum size/depth, stale-handle use, cache queries after mutation, external commits, dispatch/replay counts and reached fault occurrences. Add targeted combination coverage such as warmed cache × changed bytes × restart × current authority.

Prioritize strengthening a weak generator or oracle before increasing volume. Keep productive established searches running while another worker builds a new mechanism. On a quiet target, inspect sampled traces and surviving deliberate defects. On a failure, shrink and triage promptly, then search adjacent operations and construction paths for the same cause.

Keep Cargo compilation at one worker initially on the current memory-constrained host; tune from measurements. Reuse a target directory only for compatible source/features and avoid competing Cargo writers; otherwise isolate outputs. Do not install a compiler cache, change the production toolchain, or enlarge CI as an incidental part of this plan. Search loops should print summaries and witnesses, not one model-visible line per generated case.

### 7. Failure triage and fix extraction

1. Preserve the initial case and locate the first production/model divergence. Investigate the oracle and adapters as seriously as production.
2. Establish the violated contract, shrink the witness, then vary the causal conditions to characterize a failure family. A returned error is not a defect merely because the model expected success.
3. Have an independent reviewer reproduce from fresh fixtures. Classify product bug, harness bug, contract ambiguity, infrastructure failure or unsupported environment; retain unresolved nondeterminism honestly.
4. Cluster by repair mechanism, not panic text. Inspect sibling APIs and alternate entrypoints. Create one focused branch per independent bug from the appropriate development base.
5. Add a readable deterministic regression with explicit expected behavior; demonstrate that it fails on the unfixed production revision for the claimed reason. Include only the minimal complete repair and focused regression in the fix deliverable.
6. Run the owning real interface, original witness, generalized family, fresh cases, and relevant consumers. Record before/after source identities. Keep broad models/generators on discovery.
7. Integrate the validated fix into discovery and continue searching beyond it. Prepare reviewable PR content; committing, pushing, creating/merging PRs and releasing are separate requested delivery actions.

Potential security findings and reproductions stay private pending the project's reporting/disclosure decision, as required by `proptest-praxis`. Do not publish those witnesses through a public discovery branch.

## Data and compatibility

No production data schema or wire format migration is proposed. Replay manifests, fixture data and discovery indexes have their own version. Production records are tested using disposable stores and exact supported versions. A fix that changes a public or persisted contract requires a focused compatibility decision and updated consumer evidence; the campaign cannot silently redefine that contract to satisfy its model.

Use repository-relative paths, caller-supplied executable locations and dynamically created temporary directories. Do not require machine-specific caches or old evidence locations. Generate disposable signatures/credentials for authority fixtures. Preserve exact bytes when normalization is not promised.

## Failure handling

Each campaign owns its children, temporary directories, sockets and ports. A controller records timeout, crash, missed fault, unsupported setup and cleanup failures separately from assertion results. Do not convert an unfinished run to pass. Persist the witness before cleanup; preserve only necessary replay artifacts with an explicit retention record. Confirm owned processes are reaped and sibling/user-data canaries are unchanged.

Replay error, missing source asset, lock mismatch and inaccessible private API block only dependent work. Continue unrelated pure searches. Report consequential contract ambiguities for a decision with the exact source/caller evidence; routine model and generator choices belong to the investigation.

## Security considerations

Exercise real admission and authority with valid disposable fixtures. Never disable verification, signatures, sandboxing, permissions or product limits to reach a deeper state. Read-only plan replay, provider output, plugin/MCP requests and browser sharing remain subject to ordinary ownership checks. Failure artifacts contain synthetic values and sanitized logs; no real account credentials or personal browser profiles.

Native enforcement claims require actual admitted and denied effects on the relevant OS. Finite tests cannot prove cryptographic collision resistance, memory zeroization, all thread schedules, or power-loss durability; record those limits instead of claiming them from sampled runs.

## Verification

Stage 1 called for real commands for each implemented harness. The planning patterns were `cargo test --locked -p <owner> --lib praxis_...` for owner-private investigations and a discovery runner's `search`/`replay` subcommands for real consumers. Current runnable commands are in the [campaign README](../discovery/proptest-praxis/README.md). Select packages/targets explicitly: root Cargo defaults to `xtask`. Add JS/Python commands only with their discovery lockfiles and verified entrypoints.

Before claiming a fix, run the applicable pinned-toolchain format/build/lint and owning consumer checks, plus the regression's before/after replay. Do not invoke retired `xtask all`, old shard/formal commands or absent test-support crates. Formal-looking modules in the source are not evidence that retired proof tooling ran. Browser/PTY/native installer verification uses the real programs and exact observed state.

Plan validation: reconcile every package name against current `Cargo.toml`, resolve all current-source links, ensure every requirement has an acceptance artifact, and check that only intended design files changed. Plan validation is not behavioral qualification. During implementation, build a file/entrypoint coverage record from actual reading and execution; this design's target tables cannot substitute for it.

## Rollout and rollback

The rollout began locally with Stage 1 and then pursued independent lanes. Broad suites remain optional discovery consumers and do not add release dependencies, startup cost, production configuration or hosted workflow gates. Do not enable disabled GitHub jobs. Any future periodic hosted campaign needs a separate decision after local timing and failure handling are proven.

Pause/resume uses the target ledger, retained cases and exact revision/commands. A deliberate mutation is removed from its disposable worktree before any deliverable is assembled. Roll back investigative scaffolding by discarding only its isolated worktree/branch after retaining required witnesses; no user checkout reset. Roll back a shipped fix using that fix's normal reviewed compatibility/recovery path.

## Open questions

The original plan had no decision blocking Stage 1. The delivered campaign is on `develop`; its source and lockfiles provide a reproducible baseline for the exercised targets. Native macOS/Windows runner availability and the broader native/browser coverage still require separate verification. Retaining the suites as hosted mainline gates remains outside this delivery.

## Out of scope

Reinstating retired qualification/formal frameworks; changing selected-model/default behavior; redesigning production algorithms before a demonstrated need; live model-answer equivalence; testing third-party libraries instead of Peritus's integration contracts; unrequested publication/releases; campaigns against unrelated repositories or external systems. Optional performance/resource measurements describe the harness and boundedness; they do not constitute a performance optimization project.
