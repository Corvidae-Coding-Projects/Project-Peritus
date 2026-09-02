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
| Recovery from empty, malformed, interrupted, timeout, and transport responses | Working | `peritus-agent/developer` retries recoverable model turns through the checked provider-neutral planner. If an empty, connection, malformed, interrupted/incomplete, transient-provider, transport, rate-limit, timeout, or generic provider result exhausts one invocation, G4 gives designer, writer, fixer, and reviewer up to two fresh repository-grounded invocations while preserving useful workspace progress. Authentication, permission, missing-model, quota, invalid-request, safety, refusal, cancellation, and ambiguous-acceptance terminals remain terminal. Product parsers return exact correction text, and daemon restart marks unfinished work recoverable. HarnessBench exercised malformed and interrupted turns; Terminal-Bench `TBF-009` exposed the exhausted-invocation boundary. | Preserve the finite same-provider bound and compare native recovery rates in the final campaign. |
| Exponential backoff, bounded jitter, and `Retry-After` | Working | Direct HTTP providers and the outer G4 developer loop use checked exponential policy. The product path applies stable bounded jitter, honors bounded provider `Retry-After`, cancels during waits, traces reason, attempt, elapsed time, and selected delay before sleeping, and includes cumulative retries in live run status. | Preserve and compare recovery rates in the final campaign. |
| Progress-aware retry and no-progress stopping | Working | Workspace checkpoints, finding identity, permission-aware progress, three-invocation same-provider role bounds, bounded segments, and two unchanged fixer cycles stop unproductive work. Within one writable invocation, twelve tool calls without a workspace mutation or successful declared external effect trigger a deterministic correction toward the shortest delivery step; at most two corrections are emitted. Terminal-Bench `TBF-002`, `TBF-003`, and `TBF-009` exercised recovery and bounded stopping, while `TBF-024` exposed a long inspection spiral before any output existed. | Preserve. Add provider-level circuit state rather than conflating provider failure with candidate progress. |
| Crash-safe replay and daemon restart | Working with a boundary | Product snapshots and conversations are atomically persisted; unfinished runs reopen as `RecoveryRequired` and can be retried against the managed worktree. Compaction records and developer commands are durably traced, and writable tool calls now have synced start/terminal receipts. Exact in-flight provider state is not resumed byte-for-byte. | Preserve effect reconciliation; do not claim exact provider-stream resumption where a provider lacks it. |
| Context-window accounting | Working | Before every real developer turn, G4 binds C6's checked `TokenBudget` to the selected provider profile, reserves output and protocol capacity, and accounts for the complete message/tool request with a conservative deterministic estimate. | Compare estimates with observed provider usage during the final campaign and revise only from evidence. |
| Deterministic context compaction | Working | When the checked trigger is crossed, `DeveloperLoop` replaces only complete old assistant/tool exchanges. System policy, the active user task, incomplete batches, corrections, and the recent working set remain exact. | Preserve and exercise across long final-campaign tasks. |
| Compaction provenance, freshness, and invalidation | Working within a role | Every replacement carries a versioned policy digest, exact source digest, replacement digest, source count, and before/after token estimates. It is traced before the next provider turn, and each fresh role rebuilds from its current exact inputs. | Exact mid-provider stream resumption remains unsupported where the provider lacks it. |
| Provider-aware prompt caching | Working | G4 optionally negotiates `PromptCaching`, selects `Automatic` only when supported, and leaves it disabled otherwise. Generated first-party account/API profiles advertise the capability; account executables remain no-flag dumb routers, and normalized usage retains cache details. | Validate observed cache accounting across both live account routes after the baseline freezes. |
| Token, cost, time, process, memory, disk, and concurrency budgets | Working with a boundary | The ordinary G4 path aggregates provider requests, retries, compactions, application tool calls, normalized tokens/cache usage, provider-estimated cost, elapsed time, current workspace bytes, positive workspace growth, and the highest resident-memory observation across every role. It enforces deliberately generous eight-hour, 4,096-request, 20,000-tool, 100-million-token, reported-cost, 12 GiB observed-memory, and 50 GiB workspace-growth ceilings with a distinct budget failure. Measurements occur at completed effect boundaries, remain visible and restart-safe in daemon progress, exclude Git object history, and retain generated build output. Commands also have process-tree, deadline, and output bounds; external campaigns remain serialized. Terminal-Bench `TBA-001` shows that Harbor computes an outer deadline but does not expose it through the public custom-agent `run` arguments, so Peritus does not guess that hidden value. | Preserve the generous defaults. Expose typed operator tuning only after real product evidence establishes useful safe ranges; a completed-boundary memory sample is not a kernel-enforced per-child limit. |
| Heartbeats, stall visibility, cancellation, and reconnect | Working | The daemon protocol retains connection heartbeat and cancellation; the TUI polls active product runs and now receives status derived from the run start, last completed durable effect, current time, cumulative counters, and the eight-hour horizon. A slow call is reported as quiet rather than falsely called failed, while restart/reconnect remains explicit. | Preserve and verify the counter/horizon text during the final live soak. |
| Rate limits, queue backpressure, provider availability, and failover | Working with a boundary | Direct providers classify rate limits and use bounded retry policy; D3 and application protocols model backpressure. G4 offers durable, default-off automatic-failover consent when at least two providers are configured. After ordinary recovery ends, each designer, writer, reviewer, or fixer role may advance through deterministic tool-compatible providers. Capability mismatch, authentication, quota, rate, timeout, malformed, and safe provider failures are eligible; safety, refusal, cancellation, raw ambiguous transport, and normalized ambiguous acceptance are not. Every switch is durably traced and counted in persisted live status. The serialized Terminal-Bench adapter now preserves official Claude CLI OAuth rotation across disposable containers through a bounded, non-rollback, compare-before-replace checkpoint (`TBF-032`). | Preserve the explicit consent and terminal taxonomy. A predictive fleet-health queue remains out of scope until real multi-user or high-concurrency evidence requires it. |
| Idempotent tool execution and duplicate-effect protection | Working | C4 and the application protocol have idempotency identities. G4 now syncs deterministic role/invocation/effect identity, provider call ID, canonical request digest, start/terminal state, and a bounded result around every writable call. Exact completed calls replay without a second effect; conflicting recovery is refused; interrupted filesystem calls use exact/no-op semantics; interrupted commands become durable ambiguous results and are not relaunched. | Preserve and exercise both completed replay and interrupted-command recovery in product qualification. |
| Bounded tool output and long-running command control | Working | Structured commands cap both streams, close stdin, own the process tree, accept 1-600 second requested deadlines, and kill/reap on expiry. Each actual allowance also shrinks against the live caller-derived product horizon while preserving a bounded completion reserve; the result exposes both timeouts and the remaining budget. A capped stream retains its opening context and final diagnostics while omitting the noisy middle. Roles must filter predictable bulk output before execution and select decisive fields, keys, counts, or bounded samples from structured and API responses. Workspace growth and observed harness memory now join the cumulative run projection. Terminal-Bench `TBF-003` reproduced the freeze, `TBF-020` exposed low-signal context consumption, `TBF-023` showed why a long dependency build must retain its terminal error, and `TBF-044` showed why individually bounded commands must share the enclosing deadline. | Preserve and compare the observations with host-level soak evidence. |
| Interactive subprocess control | Partial | C2 and C4 already own PTY allocation, bounded stdin, incremental polling, resize, portable signals, cancellation, output retention, and recovery. The ordinary G4 product developer loop does not route through that active-execution surface: it exposes only a synchronous one-shot structured command with closed stdin. Terminal-Bench `TBF-047` required the model to replace a missing durable terminal handle with repeated handwritten socket scripts, consuming its task window without completing the live system. | Route G4 through the existing daemon-owned C4 active execution controls with stable invocation handles and bounded incremental observations. Do not add a second raw PTY implementation inside the product runner. |
| Large-repository navigation and exact grounding | Working | Every fresh role must list then read; search/read/list are bounded; ignored/generated trees are filtered; exact changed paths drive gates; changed sources have a hard 500-line acceptance limit. Terminal-Bench `TBF-014` now ensures that dirty files and real patches inside a cloned nested Git repository remain visible instead of collapsing to one outer directory marker. `TBF-017` adds the exact managed root and relative-path contract to every mandatory listing so an absolute requested path cannot be guessed into a duplicated root directory. `TBF-031` keeps large generated-artifact workspaces usable by making their deterministic design inventory a clearly truncated navigation sample rather than a pre-model rejection. `TBF-050` distinguishes descriptive plural extension patterns from concrete dotfile paths during exact-output reconciliation. HarnessBench and Terminal-Bench exercise this path. | Preserve. Improve semantic navigation only if measured tasks show literal search is inadequate. |
| Changed-target verification and independent review | Working | Exact project discovery runs native checks; independent review uses a distinct read-only executor and conserved typed findings. `TBF-004` proves exact permissions and developer command observations reach review without becoming deterministic gates. `TBF-013` adds a separate, default-off delivery scope for explicitly authorized external effects: a zero-diff result needs a successful effect command, a later fresh verification command, and blocker-free independent review. `TBF-016` requires ordinary prerequisites inside an authorized disposable subject to be attempted before escalation without extending that authority to the user's durable host. `TBF-025` closes the mixed-delivery gap: an explicit live operational request needs the same effect and later verification even when useful helper files changed. `TBF-026` prevents one calibration example from masquerading as generalization evidence for empirical or heuristic behavior. `TBF-028` makes closed path, value, and transformation restrictions acceptance-critical for every changed token instead of permitting helpful adjacent cleanup. `TBF-045` prevents repeated modes of one lossy extractor from masquerading as independent evidence for an exact literal and requires source-level or genuinely independent disambiguation. `TBF-048` separates empirical candidate selection from final holdout acceptance, retains a measured candidate ledger, and preserves the best valid candidate while bounded experiments proceed. `TBF-049` attributes exact files created by a harness-owned command so its own temporary products can be cleaned up without weakening protection for unrelated late evidence. `TBF-052` requires performance evidence across every value in a small bounded domain and a margin beyond timing noise before a consistent-win claim. Ordinary workspace runs remain strict. | Preserve both explicit scopes. Continue to distinguish execution observations from harness-owned checks. |
| Declared opaque-interface boundaries | Working with a boundary | `TBF-051` derives a scoped policy when a task combines an explicit black-box or unknown-state restriction with a named query/import/call interface. Roles may list opaque inputs as metadata, but direct reads and mutations are refused, workspace search omits their contents, direct hidden-identifier and implementation-path command references are rejected, and review treats contaminated trace evidence as blocking. This prevents ordinary accidental leakage; it is not a high-assurance information-flow sandbox against intentionally obfuscated native code. | Preserve for normal coding agents. Use a separately brokered process or RPC boundary when a task requires adversarial-grade secrecy. |
| Durable conversations and correction during work | Working | User and agent messages are persisted, revisioned, accepted during an active run, and used to trigger a fresh design/write pass. Waiting, failure, retry, and continuation are visible in the TUI. | Preserve; compaction must never alter the canonical human conversation. |
| Trace inspection, failure analysis, and harness evolution | Working at subsystem level | C7 traces, E2 debugger, E3 evaluation, F0 evolution, HarnessBench reports, and the failure journal provide durable causal evidence. Automatic promotion remains intentionally gated by H4 evidence and authority. | Add product-facing trace summaries and benchmark comparisons; keep promotion fail-closed. |
| Interactive and noninteractive operation | Working | `peritus` provides the single-command TUI; `peritus-benchmark-agent` provides the real noninteractive Rust-owned path used by both external suites. | Preserve and document ordinary CI invocation after the benchmark campaign stabilizes. |
| Configuration migration, compatibility, diagnostics, and rollback | Working with final-campaign evidence pending | C0/G0/H2/H4 provide versioned state, recovery, package layout, migration, diagnostics, and rollback contracts. The public bootstraps use those same native transactional adapters, whose repeat install, upgrade, rollback, state preservation, and uninstall paths are exercised on all hosted platforms. | Re-run the complete lifecycle against the exact release candidate and retain the H2/H4 evidence. |
| One-command install and safe startup updates | Working with first-release evidence pending | Root POSIX and PowerShell bootstraps resolve native GitHub release assets and reject checksum mismatch. G4 performs a nonblocking six-hour cached startup check, offers an update, exposes `peritus update`, and persists explicit enable/disable commands; downloads are streamed within a 1 GiB bound, verified, and installed through native rollback adapters. Windows finishes after the running executable exits. The release workflow retains a draft until all three native jobs pass. | Publish and exercise the first exact release, including hosted offline-startup and interrupted-update cases, before a production-ready claim. |

## Interim Terminal-Bench control-flow evidence

The frozen campaign had completed 202 trials when this audit was refreshed. Harbor had awarded
111 rewards of 1.0, but only 19 of those trials reached a clean native Peritus terminal. Eighty
passing artifacts were followed by a native provider failure, six by a native no-progress gate, and
six by an external deadline or setup failure before a native report was published. The same snapshot
contained 62 reward-zero provider terminals and three unscored provider terminals.

Those counts do not turn failed tasks into passes, and they do not prove that every zero is
recoverable. They do show that exhausted role recovery and terminal handoff are broad product
problems rather than isolated task behavior. The final campaign must improve native completion as
well as external accuracy; preserving a valid artifact while falsely reporting failure is not a
production-quality result.

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
6. Completed on the draft qualification branch: default-off compatible role failover after
   same-provider recovery, durable switch evidence, persisted counters, and an explicit exclusion
   for safety, refusal, cancellation, and ambiguous acceptance. Predictive queue health remains
   evidence-gated rather than speculative.
7. Route the G4 product developer loop through C4's existing daemon-owned active shell execution so
   models can start, poll, write bounded stdin, resize, signal, cancel, and recover PTY processes by
   stable invocation handle without bypassing C2/C3 ownership.
8. Run a final Terminal-Bench k=5 campaign with the final binary so baseline and final scores refer
   to distinct, exact commits.
9. Completed on the draft qualification branch: public POSIX and PowerShell installers, cached
   startup checks, explicit self-update, checksum verification, native rollback adapters, a
   retained-draft three-platform release workflow, and an executable local bootstrap smoke.

## Non-goals

- No task-name branches, oracle leakage, benchmark fixture edits, or verifier changes.
- No provider switch without durable user consent and durable switch evidence; no unverified
  executable replacement.
- No model-authored compacted summary accepted without source binding and freshness checks.
- No claim that substrate code is a product feature until the ordinary launcher/daemon path uses it.
- No speculative hardening that displaces observed product failures or common long-run behavior.

## Benchmark integrity reporting

The final report will preserve every upstream score and separate real Peritus defects, legitimate
capability limitations, provider or infrastructure failures, and benchmark underspecification. For
each benchmark gotcha it will show the published contract, the unpublished or contradictory
expectation, the retained result, and the shortcut that was refused. Refused shortcuts include
reading hidden verifiers or reference solutions, hard-coding task names or private vocabulary,
changing fixtures, resources, deadlines, or scoring code, and adding behavior whose only evidence
is a benchmark-specific win. General fixes supported by ordinary application behavior remain in
the product and are reported separately from score-only compatibility guesses. The living
[benchmark integrity appendix](benchmark-integrity-appendix.md) indexes each current gotcha and the
score-only shortcut that Peritus refused.

This file is a living delivery record. Final benchmark aggregates, implemented-gap commits, and
remaining limitations will be added only when the corresponding reproducible evidence exists.
