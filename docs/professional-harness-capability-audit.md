# Professional coding-harness capability audit

This audit compares the product people actually run with the capabilities expected from a
professional coding harness. It does not give Peritus credit merely because a lower-level crate
contains a useful type. A capability is **working** only when the ordinary `peritus` product path
uses it and there is execution evidence. **Partial** means the necessary machinery exists but the
product path does not yet provide the complete behavior. **Missing** means the user-facing path has
no implementation.

The current product path is:

```text
peritus launcher -> peritusd -> durable product-run service
                 -> product runner -> D0 developer loop
                 -> selected account or direct provider
                 -> structured workspace tools -> gates -> independent review -> fixer
```

The comparison sources are the pinned local Codex CLI, NexAU-AHE, LemonHarness, HarnessBench, and
Terminal-Bench checkouts under `reference-repos/`, plus their primary documentation and Peritus's
retained benchmark evidence. External projects are references, not authorities: Peritus adopts
useful behavior only when it fits its own local-first and evidence-driven design.

## Current capability map

| Capability | Product-path status | Current evidence | Delivery decision |
| --- | --- | --- | --- |
| Durable objective beyond one model call | Working | `peritus-daemon/product_run` persists the task, conversation, phase, findings, deliverable, and latest aggregate accounting; the product runner divides work into bounded design, writer, review, and fixer turns. | Preserve and exercise long continuation in the final campaign. |
| Recovery from empty, malformed, interrupted, timeout, and transport responses | Working | `peritus-agent/developer` retries recoverable model turns through the checked provider-neutral planner; product-level parsers return exact correction text; daemon restart marks unfinished work recoverable. HarnessBench exercised malformed and interrupted turns. | Preserve and compare recovery rates in the final campaign. |
| Exponential backoff, bounded jitter, and `Retry-After` | Working | Direct HTTP providers and the outer G4 developer loop use checked exponential policy. The product path applies stable bounded jitter, honors bounded provider `Retry-After`, cancels during waits, traces reason, attempt, elapsed time, and selected delay before sleeping, and includes cumulative retries in live run status. | Preserve and compare recovery rates in the final campaign. |
| Progress-aware retry and no-progress stopping | Working | Workspace checkpoints, finding identity, permission-aware progress, bounded segments, and two unchanged fixer cycles stop unproductive work. Terminal-Bench `TBF-002` and `TBF-003` exercised recovery and bounded stopping. | Preserve. Add provider-level circuit state rather than conflating provider failure with candidate progress. |
| Crash-safe replay and daemon restart | Working with a boundary | Product snapshots and conversations are atomically persisted; unfinished runs reopen as `RecoveryRequired` and can be retried against the managed worktree. Compaction records and developer commands are durably traced, and writable tool calls now have synced start/terminal receipts. Exact in-flight provider state is not resumed byte-for-byte. | Preserve effect reconciliation; do not claim exact provider-stream resumption where a provider lacks it. |
| Context-window accounting | Working | Before every real developer turn, G4 binds C6's checked `TokenBudget` to the selected provider profile, reserves output and protocol capacity, and accounts for the complete message/tool request with a conservative deterministic estimate. | Compare estimates with observed provider usage during the final campaign and revise only from evidence. |
| Deterministic context compaction | Working | When the checked trigger is crossed, `DeveloperLoop` replaces only complete old assistant/tool exchanges. System policy, the active user task, incomplete batches, corrections, and the recent working set remain exact. | Preserve and exercise across long final-campaign tasks. |
| Compaction provenance, freshness, and invalidation | Working within a role | Every replacement carries a versioned policy digest, exact source digest, replacement digest, source count, and before/after token estimates. It is traced before the next provider turn, and each fresh role rebuilds from its current exact inputs. | Exact mid-provider stream resumption remains unsupported where the provider lacks it. |
| Provider-aware prompt caching | Working | G4 optionally negotiates `PromptCaching`, selects `Automatic` only when supported, and leaves it disabled otherwise. Generated first-party account/API profiles advertise the capability; account executables remain no-flag dumb routers, and normalized usage retains cache details. | Validate observed cache accounting across both live account routes after the baseline freezes. |
| Token, cost, time, process, memory, disk, and concurrency budgets | Partial | The ordinary G4 path now aggregates provider requests, retries, compactions, application tool calls, normalized tokens/cache usage, provider-estimated cost, and elapsed time across every role. It enforces deliberately generous eight-hour, 4,096-request, 20,000-tool, 100-million-token, and reported-cost runaway ceilings with a distinct budget failure. Command process deadlines/output and serialized external runs remain bounded. Per-run memory and disk-growth ceilings are not yet unified into this projection. | Preserve the long-run defaults. Add memory or disk dimensions only with portable measurements and a useful operator override. |
| Heartbeats, stall visibility, cancellation, and reconnect | Working | The daemon protocol retains connection heartbeat and cancellation; the TUI polls active product runs and now receives status derived from the run start, last completed durable effect, current time, cumulative counters, and the eight-hour horizon. A slow call is reported as quiet rather than falsely called failed, while restart/reconnect remains explicit. | Preserve and verify the counter/horizon text during the final live soak. |
| Rate limits, queue backpressure, provider availability, and failover | Partial | Direct providers classify rate limits and use bounded retry policy; D3 and application protocols model backpressure. G4 lets users select providers but has no health-aware queue or explicit failover chain for an active role. | Add opt-in role failover with compatibility checks and a durable provider-change event. Never silently switch models. |
| Idempotent tool execution and duplicate-effect protection | Working | C4 and the application protocol have idempotency identities. G4 now syncs deterministic role/invocation/effect identity, provider call ID, canonical request digest, start/terminal state, and a bounded result around every writable call. Exact completed calls replay without a second effect; conflicting recovery is refused; interrupted filesystem calls use exact/no-op semantics; interrupted commands become durable ambiguous results and are not relaunched. | Preserve and exercise both completed replay and interrupted-command recovery in product qualification. |
| Bounded tool output and long-running command control | Working | Structured commands cap both streams, close stdin, own the process tree, accept 1-600 second deadlines, and kill/reap on expiry. Terminal-Bench `TBF-003` reproduced the freeze and proved the corrected behavior. | Preserve and include observations in cumulative run budgets. |
| Large-repository navigation and exact grounding | Working | Every fresh role must list then read; search/read/list are bounded; ignored/generated trees are filtered; exact changed paths drive gates; changed sources have a hard 500-line acceptance limit. HarnessBench and Terminal-Bench exercise this path. | Preserve. Improve semantic navigation only if measured tasks show literal search is inadequate. |
| Changed-target verification and independent review | Working | Exact project discovery runs native checks; independent review uses a distinct read-only executor and conserved typed findings. `TBF-004` proves exact permissions and developer command observations reach review without becoming deterministic gates. | Preserve. Continue to distinguish execution observations from harness-owned checks. |
| Durable conversations and correction during work | Working | User and agent messages are persisted, revisioned, accepted during an active run, and used to trigger a fresh design/write pass. Waiting, failure, retry, and continuation are visible in the TUI. | Preserve; compaction must never alter the canonical human conversation. |
| Trace inspection, failure analysis, and harness evolution | Working at subsystem level | C7 traces, E2 debugger, E3 evaluation, F0 evolution, HarnessBench reports, and the failure journal provide durable causal evidence. Automatic promotion remains intentionally gated by H4 evidence and authority. | Add product-facing trace summaries and benchmark comparisons; keep promotion fail-closed. |
| Interactive and noninteractive operation | Working | `peritus` provides the single-command TUI; `peritus-benchmark-agent` provides the real noninteractive Rust-owned path used by both external suites. | Preserve and document ordinary CI invocation after the benchmark campaign stabilizes. |
| Configuration migration, compatibility, diagnostics, and rollback | Working with final-campaign evidence pending | C0/G0/H2/H4 provide versioned state, recovery, package layout, migration, diagnostics, and rollback contracts. The public bootstraps use those same native transactional adapters, whose repeat install, upgrade, rollback, state preservation, and uninstall paths are exercised on all hosted platforms. | Re-run the complete lifecycle against the exact release candidate and retain the H2/H4 evidence. |
| One-command install and safe startup updates | Working with first-release evidence pending | Root POSIX and PowerShell bootstraps resolve native GitHub release assets and reject checksum mismatch. G4 performs a nonblocking six-hour cached startup check, offers an update, and exposes `peritus update`; downloads are streamed within a 1 GiB bound, verified, and installed through native rollback adapters. Windows finishes after the running executable exits. The release workflow retains a draft until all three native jobs pass. | Publish and exercise the first exact release, including hosted offline-startup and interrupted-update cases, before a production-ready claim. |

## Material implementation queue

The following items are accepted because they affect ordinary long coding runs or the release
experience. They are ordered to avoid invalidating benchmark evidence more often than necessary.

1. Finish the Terminal-Bench baseline and classify every failure without changing the running
   binary.
2. Completed on the draft qualification branch: G4 context accounting, deterministic compaction,
   preservation rules, digest-bound trace records, and focused regression coverage.
3. Completed on the draft qualification branch: negotiated automatic prompt caching while
   preserving local-first provider storage policy and disabled fallback.
4. Completed on the draft qualification branch: checked exponential outer retries, bounded stable
   jitter, provider `Retry-After`, cancellation-aware waiting, and durable visible retry reasons.
5. Completed on the draft qualification branch: cumulative model/time/tool/cost accounting, live
   stall status, and the durable effect-receipt ledger with duplicate and ambiguous-command
   recovery.
6. Add explicit compatible role failover and queue health only after the failure taxonomy shows
   which provider transitions are safe and useful.
7. Run a final Terminal-Bench k=5 campaign with the final binary so baseline and final scores refer
   to distinct, exact commits.
8. Completed on the draft qualification branch: public POSIX and PowerShell installers, cached
   startup checks, explicit self-update, checksum verification, native rollback adapters, a
   retained-draft three-platform release workflow, and an executable local bootstrap smoke.

## Non-goals

- No task-name branches, oracle leakage, benchmark fixture edits, or verifier changes.
- No silent provider switch or unverified executable replacement.
- No model-authored compacted summary accepted without source binding and freshness checks.
- No claim that substrate code is a product feature until the ordinary launcher/daemon path uses it.
- No speculative hardening that displaces observed product failures or common long-run behavior.

This file is a living delivery record. Final benchmark aggregates, implemented-gap commits, and
remaining limitations will be added only when the corresponding reproducible evidence exists.
