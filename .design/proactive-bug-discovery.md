# Feature: Proactive bug discovery and regression prevention

## Summary

Build a repeatable bug-discovery program combining adversarial code review, invariant-driven scenarios, property and differential testing, fuzzing, mutation testing, and bounded chaos campaigns. Every confirmed defect must produce a reproducible failure, a fix at its cause, and a permanent regression test.

This is an implementation plan, grounded in `develop` at `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`. No fuzz, mutation, or chaos campaigns were executed for this plan. Two `gpt-daybreak-blue-latest` subagents independently reviewed recovery and external-boundary scenarios; their suspected failure windows are hypotheses to test, not confirmed bugs.

The first increment concentrates on product-run recovery, context capacity, command identity, and the native lifecycle regressions from issues [#70](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues/70) and [#59](https://github.com/Corvidae-Coding-Projects/Project-Peritus/issues/59). Subsequent increments expand to provider streams, the journal-backed agent engine, process ownership, and native packaging on all supported platforms.

## User-visible behavior

Developers get a short, reproducible explanation of each discovered defect: the violated promise, the smallest triggering input or event schedule, observed persisted state, and the regression that prevents recurrence. Routine pull requests replay known failures quickly; longer scheduled campaigns explore new failures and preserve evidence.

For users of Peritus, the desired outcomes are reliable restart and cancellation, truthful failure reporting, bounded resource use, preserved user data, and prevention of duplicated or misattributed effects. Testing does not silently introduce stronger recovery or transactional guarantees than the product currently promises.

## Requirements

| ID | Requirement |
| --- | --- |
| R1 | Inventory each tested boundary, its owner, observable invariant, independent oracle, platform, and uncovered conditions. Distinguish inspection, simulated faults, native execution, and confirmed defects. |
| R2 | Exercise deterministic success and failure scenarios, including restart, cancellation, concurrency ordering, capacity boundaries, duplicate input, and partial I/O. Assert required fault points were reached. |
| R3 | Add raw-input and structure-aware fuzz targets with seed corpora, explicit resource limits, input minimization, and replay outside the discovery engine. |
| R4 | Measure whether tests detect meaningful behavioral mutations. Report surviving, caught, equivalent, unviable, timed-out, and untested cases separately. |
| R5 | Run Daybreak Blue agents on bounded chaos assignments with isolated processes, storage, schedules, and evidence. Require independent reproduction before classifying a discovery as a defect. |
| R6 | Preserve a replay manifest and minimized reproducer for every accepted failure; fix its cause and demonstrate the regression fails before the fix and passes afterward. |
| R7 | Preserve production authority checks, public API boundaries, durable formats, stable toolchain policy, and formal/reproducibility gates. Review any necessary compatibility changes explicitly. |
| R8 | Keep PR verification bounded and hermetic; shard longer campaigns, preserve incomplete-work status, and distinguish harness failures from product failures. |
| R9 | Verify cleanup and containment: no unrelated processes stopped, unrelated files changed, external provider calls, or surviving child processes. |

## Acceptance criteria

| Requirement | Observable completion evidence |
| --- | --- |
| R1 | A checked-in inventory names all campaign targets and links each invariant to its owning code, oracle, tests, and remaining gaps. Every result records the tested source SHA and configuration. |
| R2 | Each pilot invariant has a normal case, a failing-boundary case, and a negative control proving its oracle detects a violation. Scheduled faults are hit; unexplored schedules are reported. |
| R3 | Each registered fuzz target builds with the qualified pinned toolchain, reaches its intended parser or transition, runs its allotted budget, and replays its checked-in corpus on CI. Byte and sequence minimization retain the failure signature. |
| R4 | A clean unmutated baseline passes. Every pilot invariant has at least one meaningful canary mutation rejected by its designated test. Mutation reachability and uninstrumented macro/generated code are recorded; no invented overall coverage score. |
| R5 | Every assigned chaos case records the reached boundary and independent before/after evidence. Accepted failures reproduce three times from fresh fixtures with the same logical signature. Flaky observations remain separately tracked. |
| R6 | Each confirmed defect has a minimized regression that fails on the pre-fix implementation and passes after the fix, with owning-suite and applicable native/formal checks passing. Unfixed confirmed defects remain explicit open work. |
| R7 | Workspace architecture, public-boundary, toolchain, proof-impact, and reproducibility checks pass for implementation changes. Any production format change has a separately reviewed migration and rollback plan. |
| R8 | Fast checks and scheduled shards meet the budgets below on their designated runners. A missing target, missed injection, infrastructure failure, or unfinished campaign cannot be reported as a passing campaign. |
| R9 | Each campaign ends with a process and filesystem census showing owned children reaped, temporary storage removed or deliberately retained as a failure artifact, and sibling canaries unchanged. |

Completion means these conditions hold for the named campaign inventory, not that the repository is bug-free. A campaign may validly discover no defects if it supplies the required reachability and oracle evidence.

## Current architecture

The workspace contains 83 packages and pins Rust 1.97.1. Existing infrastructure is substantial; reuse it before adding dependencies or frameworks.

| Existing surface | Reuse and constraint |
| --- | --- |
| [Test-support foundation](../docs/test-conformance-foundation.md) and [fault injector](../crates/app/testing/peritus-test-support/src/fault/injector.rs) | Deterministic clocks, IDs, scripts, temporary repositories, named fault points, occurrence counts, and `verify_all_triggered`. These provide the common test mechanics. |
| [Scripted HTTP faults](../crates/app/testing/peritus-test-support/src/http_server/model.rs) | Loopback response scripts already support close-after-headers/chunks and release barriers. Extend these for stream partitioning and explicit schedule control. |
| [Native resilience qualification](../crates/app/testing/peritus-resilience-qualification/README.md) | Existing journal, lease, snapshot, patch, and lifecycle qualification provides fixtures and evidence conventions. Its documented coverage is not evidence that a new campaign has run. |
| [Product lifecycle](../crates/app/peritus-daemon/src/product_run/lifecycle.rs), [candidate resume](../crates/app/peritus-product-runner/src/execution/resume/durable.rs), and [effect receipts](../crates/app/peritus-product-runner/src/developer_tools/receipt.rs) | Product-run recovery combines product records, resumable candidate state, and command receipts. Restored effectful checks are rerun; completed receipts can replay results, while uncertain command execution must remain ambiguous. |
| [AgentDriver](../crates/orchestration/peritus-agent/src/runtime/driver.rs) | A separate journal-backed engine with its own cursor, dispatch, and recovery semantics. Test it independently; do not invent one transaction spanning this engine and ProductRunner receipts. |
| [Context selection](../crates/orchestration/peritus-context/src/working/selection.rs) and [request assembly](../crates/app/peritus-product-runner/src/local_context/assembly/working.rs) | Required dependency closure and complete request accounting are separate obligations. Issue #70 is a corpus seed for both. |
| [Windows lifecycle tests](../packaging/tests/windows-lifecycle.Tests.ps1) and [native lifecycle checks](../crates/app/testing/peritus-platform-qualification/src/native_controller/checks/lifecycle.rs) | Issue #59 supplies locked-file, process-scope, and failure-reporting regressions. Mocked supervisor routing and real native supervisor behavior require separate evidence. |
| [CI sharding](../xtask/src/ci_shard.rs), [workflow command policy](../xtask/src/reproducibility/workflow_command_policy.rs), and [source discovery](../xtask/src/source/layout_discovery.rs) | New tools, workflows, and harness sources must receive explicit policy treatment. The existing ten-minute shard ceiling and reviewed pre-Cargo configuration remain constraints. |

Inspection found no configured cargo-fuzz, property-testing, or cargo-mutants campaign in the current manifests/workflows. The production architecture design mentions these techniques as desired verification. Locally, `cargo-fuzz` 0.13.2 is installed and `cargo-mutants` is absent from PATH; neither fact qualifies a CI toolchain.

## Proposed design

### 1. One evidence contract, domain-owned assertions

Extend `peritus-test-support` for reusable scheduling, bounded child control, and replay metadata. Reuse `peritus-conformance` for verdict conventions where suitable. Keep assertions in the owning subsystem's tests and qualification adapters; do not build a second implementation of Peritus in a generic testing framework.

Proposed new artifacts are `docs/testing/proactive-bug-discovery.md` for the maintained inventory and operator instructions, plus schema-versioned replay manifests and small fixtures under each owning test suite. A standalone `fuzz/` workspace with its own lockfile is a proposed location, subject to the source-ownership and toolchain spike below. These paths are planned, not currently implemented.

Each case records source SHA, tool versions, OS, architecture, features, fixture schema, seed, exact bytes or structured operations, named fault points and occurrences, reached-point evidence, expected invariant, actual result, bounded resource use, relevant logs, and pre/post state digests. Record logical event order rather than relying on timestamps as the ordering oracle. Preserve exact invocation and failure signature.

Inspect decoded receipts, journals, product records, filesystem/Git state, provider request logs, process identities, and supervisor observations independently of the function's returned success status. Use small reference calculations for dependency reachability, budget arithmetic, identity binding, and state transitions. Do not copy the production algorithm into the oracle.

### 2. Invariant-driven scenarios and property checks

Begin with a review of each boundary's callers and error paths, then turn promises into executable cases:

| Campaign | Initial scenarios | Independent oracle |
| --- | --- | --- |
| Product recovery | Crash around durable Started receipt, external effect, Completed receipt, resume-state save, product-record save, and final completion; repeated restart | Actual effect counter plus decoded durable state. Completed effects are not repeated; uncertain command effects remain explicitly unresolved. Restart never fabricates completion. |
| Cancellation and input | Cancel versus shutdown inventory, completion, retry, and queued follow-up; graceful stop and process kill | Explicit cancellation remains distinct from shutdown interruption; no new dispatch after the owning dispatcher observes cancellation; already-authorized effects settle or remain explicitly ambiguous/indeterminate under that engine's contract; one active run per workspace; admitted input is durably accounted for according to its API contract. |
| Command identity | Same provider ID with changed arguments; new ID with same arguments; changed ordinal, revision, or ordering | Scope/identity conflicts are rejected, and a receipt cannot authorize a different command. The actual effect count agrees with the documented replay decision. |
| Context capacity | Limit minus one, exact limit, and limit plus one; required/shared/deep dependencies; framing and tool-policy size; Unicode and arithmetic edges | A simple graph-closure oracle and independently itemized request estimate. Required context fits when permitted and cannot silently disappear. Hard limits remain enforced. |
| AgentDriver recovery | Duplicate and reordered events, crash after dispatch, replay from checkpoint/cursor | Journal prefix, cursor, terminal state, and budget accounting remain consistent with this engine's contract; lost in-flight effects are not silently redispatched. |
| Provider behavior | Every stream split point, truncated frame/tool JSON, duplicate/conflicting IDs, extra terminal frames, cancellation, rate-limit/failover sequences | Only complete valid messages reach execution; dispatch and accounting occur as specified; failover remains bounded and restricted to eligible configured providers. |
| Processes and storage | Output floods, descendants holding pipes, exit/control races, create/write/fsync/rename/delete failures, quota exhaustion | Owned children are reaped; output and storage limits are respected; failures remain visible; recovery does not seize an unrelated process or path. |
| Native lifecycle | Stop, stage, publish, permissions, supervisor registration/readiness, cleanup, and deletion failures | Correct exit status and actual installed state, preserved user data, unchanged sibling installations, correctly scoped process ownership, and documented retry behavior. |

Add metamorphic properties where justified: valid stream chunking does not change decoded output; replaying an already applied journal event does not repeat its semantic effect; increasing available context cannot make a previously feasible required closure infeasible. Compare pure calculations to small reference models, and protocol behavior to existing contract fixtures. Avoid comparing nondeterministic live model answers as a correctness oracle.

For cancellation, record the request, observation at the owning dispatch boundary, and acknowledgement separately. Establish that boundary's ordering contract before asserting an effect happened too late; a previously authorized effect may become observable after the cancellation request. ProductRunService and AgentDriver each retain their own contract and test schedule.

For context tests, preserve the provider's input/output capacity semantics; do not subtract output allowance twice. Estimated tokens are not a claim of universal agreement with every provider tokenizer.

For native installation, assert old-or-new atomicity only where an existing transaction contract requires it. A documented partial installation with a truthful error and working retry is different from false success. Requiring globally atomic publication would be a separate product design decision.

### 3. Fuzzing

Start with fast pure boundaries: [SSE framing](../crates/model/peritus-provider-core/src/framing/sse.rs), [NDJSON framing](../crates/model/peritus-provider-core/src/framing/ndjson.rs), and the [process recovery manifest codec](../crates/runtime/peritus-process/src/recovery/manifest/codec.rs). Add public context-selection inputs and structured provider event sequences after the initial targets are qualified. Inventory other checkpoint decoders before adding them; do not assume a private API is accessible from a harness.

Use two input families:

- Raw bytes: malformed encoding, truncated lengths, oversized fields, invalid identifiers, framing boundaries, and bounded nesting.
- Structured cases: valid messages with adversarial chunk schedules, dependency graphs and capacities, and short state-machine operation sequences that reach deeper than immediate parser rejection.

Seed from existing conformance fixtures and the recent regressions. Minimize bytes, then sequence operations and fault schedules while retaining the same oracle failure and required fault hit. Convert accepted failures into ordinary stable-toolchain regressions; retain the discovery seed and minimized corpus entry.

`cargo-fuzz` requires nightly sanitizer support and a C++ compiler; use an explicitly pinned harness-only nightly without changing the production default. Confirm the exact nightly, tool version, target triple, dependency lockfile, and architecture policy in the qualification spike. See the [official setup](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html) and [structure-aware fuzzing guidance](https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html).

Private product memory helpers stay private. Exercise them through owner-crate property/scenario tests or the real public boundary. Fuzz instrumentation must not disable signatures, authorization, production limits, or proof obligations; construct valid signed fixtures with disposable test keys when deeper exploration needs them.

### 4. Mutation testing

Qualify `cargo-mutants` against three small slices first: required-context selection, receipt identity/replay decisions, and product cancellation/recovery transitions. Run the unmodified baseline with exactly the tests that will evaluate mutants. Keep each mutation sandbox isolated from the working checkout.

Enumerate discovered mutations and source ranges before reporting effectiveness. Confirm the important transitions are actually mutable: macro-generated and build-generated code can be missed. Where needed, use a small curated patch canary in an ephemeral checkout to remove a required guard, flip a boundary comparison, omit a persistence step, or turn a failure into success. Canary edits must never enter the deliverable branch.

Require named assertions to catch those meaningful behavioral canaries. A compiler error, proof-gate rejection, timeout, or mutation that cannot be built is useful evidence with its own classification; it does not demonstrate that the behavioral test caught the defect. A surviving mutant requires investigation, since it may be equivalent or may expose a missing assertion. The initial goal is explained outcomes and strong invariants, not an arbitrary mutation-score target. See [tool limitations](https://mutants.rs/limitations.html) and [result interpretation](https://mutants.rs/using-results.html).

### 5. Daybreak Blue chaos campaigns

Use `gpt-daybreak-blue-latest` subagents for adversarial scenario generation and bounded execution. Assign at most two discovery lanes concurrently, leaving capacity for the coordinating agent and an independent reproducer/reviewer. Each execution lane receives an isolated worktree, temporary runtime, exact target SHA, case manifest, time/resource limits, and an owned file list. Findings agents do not make overlapping production edits.

| Lane | First assignment | Expansion |
| --- | --- | --- |
| Recovery and identity | Product-run crash windows, receipt identity, cancellation/retry races | Separate AgentDriver cursor/dispatch campaign; admitted follow-up versus finalization |
| External boundaries | Windows #59 lifecycle seeds and provider disconnects | Linux/macOS lifecycle, process/PTY control, scoped filesystem failures, context/provider combinations |

Prefer explicit barriers over sleeps. Extend the existing named fault mechanism with narrow adapter-level barriers only where missing. A child reports that it reached the intended boundary through a harness-owned channel; the controller then disconnects, injects the selected error, or kills that exact owned process. Record the reached point and reject a case that misses it.

Use actual child termination for crash campaigns, rather than representing every crash as a returned error. SIGKILL/process termination establishes process-crash behavior, not power-loss durability. Use disposable native runners for real Windows service/Task Scheduler, launchd, and systemd-user behavior; a fake supervisor is valuable deterministic evidence but does not qualify the native integration.

The planning reviews identified cancellation/startup races and suppressed POSIX supervisor errors as useful probes. Reproduce these before making a defect claim; for example, absent registration must be distinguished from an actual access-denied or failed-stop condition.

Run single faults before pairwise combinations. Initial combinations are output limit plus cancellation, provider disconnect plus a pending effect, context boundary plus provider framing, and installer failure plus a sibling installation. Expand only when the component cases have reliable oracles and cleanup.

### 6. Reproduction and fix loop

1. Classify the observation as product invariant violation, harness defect, infrastructure failure, unsupported environment, or unresolved nondeterminism.
2. Preserve evidence and minimize the input or schedule without losing fault reachability.
3. Have a separate agent reproduce accepted product failures three times from fresh fixtures. Keep intermittent failures visible even if they have not met this confirmation threshold.
4. Add the smallest stable regression at the owning boundary and demonstrate failure against the original revision.
5. Implement the smallest complete fix at the cause; do not relax the oracle, add sleeps, swallow errors, or weaken production limits to make the case pass.
6. Run the owning suite, related consumer regressions, and applicable native, architecture, reproducibility, and formal gates. Replay the original discovery artifact against the fix.
7. Integrate a reviewed, coherent fix into `develop` during implementation. Record the reproducer, before/after evidence, remaining campaign gaps, and exact commit. Rebase or replay evidence if the tested base changes.

### 7. Delivery order and CI

| Increment | Work and compatibility impact | Exit evidence |
| --- | --- | --- |
| A: Qualify the foundation | Inventory tests, pin candidate tools, build one parser fuzz target, enumerate one mutation slice, validate harness ownership/workflow rules. No production format changes. | Reproducible builds and clean baseline; explicit macro/platform gaps; audited tool and source-policy changes scoped for review. |
| B: Deliver the pilot | Add replay manifests, barriers, independent oracles, and deterministic recovery/context/identity scenarios; retain #59 native seeds. Test-only additions or narrow adapter seams. | R1/R2/R9 pilot evidence and normal stable-toolchain replay. |
| C: Add discovery engines | Deliver raw and structured fuzzing, scoped mutation runs, and behavioral canaries. Isolated harness lockfile and pinned tools. | R3/R4 evidence, minimized sample replay, bounded runtime, no validation bypass. |
| D: Run chaos and fix defects | Run the two Daybreak lanes, reproduce findings independently, then deliver small root-cause fixes. Any API/storage impact reviewed per finding. | R5/R6 evidence, cleanup census, full applicable verification for each fix. |
| E: Automate and expand | Add audited PR replay and scheduled/manual campaign shards; extend provider, AgentDriver, process, Linux/macOS, and native supervisor coverage. | R7/R8 evidence and a published inventory of executed and outstanding cases. |

Proposed starting budgets, to calibrate in increment A:

| Execution mode | Initial budget and policy |
| --- | --- |
| PR checks | Ordinary regressions and checked-in corpus replay; target at most five minutes of execution per shard, with the existing ten-minute job ceiling including build/setup. New discoveries are not required on every PR. |
| Scheduled fuzz | Five minutes of engine execution per target per shard, with bounded input size, memory, and output; ten-minute job ceiling. Longer campaigns are explicitly separate manual jobs. |
| Scheduled mutation | Changed/high-risk slices selected deterministically. Stop launching mutants with sufficient cleanup time before the ten-minute ceiling; retain a resume inventory and mark unfinished scope incomplete. Derive per-mutant limits from measured baseline duration. |
| Chaos batches | At most two lanes; initial limits of 60 seconds per pure/runtime case and 120 seconds per native case, each batch fitting a ten-minute worker job including cleanup. Use a watchdog outside the faulted child. |
| Per-case resources | Initial child-tree caps: 32 processes, 2 GiB memory, 512 MiB fixture/disk usage, 8 MiB captured output, and 8 MiB diagnostic logs. Reserve ten seconds for cleanup, or thirty seconds for native cases, within the case deadline. These caps exclude compilation and agent reasoning. |
| Artifacts | Keep scheduled raw logs/manifests for 30 days initially; keep accepted minimized regressions and their provenance in the repository. Bound artifact sizes and redact secrets. |

Enforce child-tree limits using qualified OS/container/VM controls and bounded capture; fault injection alone is not a containment limit. Cap each retained failure bundle at 64 MiB, preserve the manifest and minimal evidence first, and explicitly mark truncation. If the environment cannot enforce a campaign's containment, classify it unsupported there. Exceeding a harness ceiling, failing cleanup, or violating the post-case census fails the case; the product's own configured limits should be lower so resource-limit scenarios can test an orderly response before hitting the harness ceiling.

Campaign execution budget, setup time, completed case count, and unexplored work are separate fields. Random seeds alone are insufficient if an external scheduler determines order; record the actual schedule. A successful short PR replay does not imply a long discovery campaign ran.

Add scheduled or manual entry points deliberately: the existing CI does not automatically run every job on a push to `develop`. Extend the reviewed workflow policy and its tests for the exact tool invocations; preserve immutable proof-impact bases and the pre-Cargo review boundary. Do not bypass these checks with an arbitrary wrapper or a nested Cargo configuration.

### Design choice

Prefer shared test mechanics plus domain-owned campaigns over a new universal chaos framework. This reuses existing fault schedules, fake providers, native controllers, and conformance evidence while keeping production invariants with their owners. It also makes rollback a matter of disabling a campaign or removing a narrow test seam.

A credible alternative is a separate all-in-one testing service running full-workspace fuzzing and mutation on every change. It centralizes orchestration, but introduces another policy boundary, weakens source ownership, duplicates fixtures, and makes runtime unpredictable. Use existing CI with small shards first; reconsider a service only after measured campaign volume justifies it.

## Data and compatibility

The initial testing infrastructure needs no production database, journal, receipt, checkpoint, or public API migration. Replay manifests are test artifacts with a versioned schema. Keep old accepted corpus entries replayable or explicitly migrate them with before/after evidence.

Register every new Rust harness source under a reviewed ownership model. The standalone fuzz workspace is conditional on proving it fits source discovery, architecture checks, lockfile policy, and the pinned nightly. If it does not fit, resolve the ownership design before adding targets; do not create an unreviewed exclusion.

Keep the production Rust pin and reviewed `.cargo/config.toml` unchanged unless a separately justified implementation requirement is found. Test barriers must not become a remotely accessible fault-control API or affect normal builds.

## Failure handling

Every case has an explicit terminal classification: passed, reproduced product failure, suspected/flaky failure, harness failure, infrastructure failure, unsupported, or incomplete. Mutation outcomes remain separately typed. Missing instrumentation or no discovered mutants is a coverage gap, not success.

Retain artifacts before cleanup on failure. Use a controller outside the faulted child to enforce timeout and collect state even if the product hangs. Verify cleanup; a leaked owned process is itself a failed case. Quarantine only with a recorded reason and repair task, never by silently removing the failing invariant from the inventory.

## Security considerations

Targets use synthetic provider credentials, loopback fake servers, disposable repositories, scoped temporary storage, and exact owned process identities. No live provider account, production daemon, user repository, operator service, or unrelated installation is a chaos target.

Fault filesystem operations only within owned fixtures or dedicated disposable disks; never exhaust the workstation filesystem. Identify a process by ownership and executable/start identity, not by a reused PID alone. Package scripts that derive paths from the user's profile run under a dedicated disposable OS user or VM with its natural profile; do not repurpose the operator's `HOME` or `CODEX_HOME`.

Keep authentication and authority enforcement enabled. Agent model access remains outside the child application under test; the child's environment does not inherit unrelated credentials. Failure artifacts contain synthetic inputs and bounded, sanitized logs.

## Verification

At implementation time, each increment runs its owning tests and the repository's required gates for the actual diff. Qualification evidence identifies host OS, native versus mocked boundaries, source SHA, toolchain, features, and exact commands. Native lifecycle behavior must be exercised on the corresponding OS, including the supported PowerShell variants for Windows regressions.

Negative controls validate the validators: miss a required fault point, corrupt expected persisted state, and apply a meaningful behavioral canary. The harness must report each as failed or incomplete with the intended reason. Only then are zero-discovery campaign results useful evidence.

For this planning pass, validate Markdown/local links and requirement-to-evidence coverage. No campaign results or new product-correctness guarantees are claimed.

## Rollout and rollback

Land the inventory and deterministic pilot first, then discovery tools, then bounded scheduled automation. Keep each increment independently replayable. New campaign lanes begin as explicit scheduled/manual checks until their baseline and resource budget are qualified; deterministic regressions become normal PR checks immediately once stable.

Rollback of testing infrastructure disables the specific new workflow or harness integration while retaining accepted regressions and evidence. Roll back a production fix only after identifying its compatibility consequences; do not discard a reproducer because its first fix needs revision.

This document does not launch implementation, campaigns, commits, or publishing. Crosslink session initialization currently reports a stale readiness record; local planning can proceed, but pipeline/session tracking must be repaired before using automated kickoff. No pipeline state is fabricated here.

After tracking and provider routing are qualified, the CLI supports previewing the first increment with `crosslink kickoff run "Implement increment A of proactive bug discovery" --doc .design/proactive-bug-discovery.md --dry-run`. Resolve the Daybreak Blue model route before launching the chaos assignments; the preview is not a campaign execution command.

## Open questions

No user decision blocks the plan. Increment A must resolve three technical qualifications: the exact compatible nightly/tool versions, reviewed ownership for the fuzz harness, and mutation reachability through this repository's macros. Budgets above are initial defaults to adjust from measured runtime.

Real native supervisor tests require disposable runner availability for each OS. Until those runners are exercised, report that portion as pending; deterministic fake-supervisor results cannot close it. Power-loss durability remains a separate experiment if a contract later requires it.

Architecture verdict: **ready for staged implementation**, beginning with increment A. The outstanding qualifications are explicit first-increment work, not claims that tooling or native campaigns already pass.

## Out of scope

- Running the proposed campaigns during this planning pass.
- A new generic testing platform, wholesale architecture rewrite, or universal exactly-once guarantee for external effects.
- Changing token-capacity semantics, production security checks, proof obligations, or the default Rust toolchain to accommodate a harness.
- Live-account chaos, host-wide outages, unrelated service control, or destructive tests against the developer's data.
- Treating mutation score, coverage percentage, elapsed fuzz time, or a model's judgment as proof of correctness.
