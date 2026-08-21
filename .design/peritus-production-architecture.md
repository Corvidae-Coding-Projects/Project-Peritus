# Feature: Peritus Production-Grade Verus-First Coding Harness

- **Status:** Architecture draft ready for review
- **Date:** 2026-08-21
- **Repository:** `Project-Peritus`
- **Audience:** senior Rust and formal-methods engineers, security reviewers, agent-runtime engineers, platform engineers, and implementation leads
- **Release posture:** no public release, preview release, or production claim is permitted until every production acceptance gate in this document is satisfied

## Summary

Peritus is a local-first, production-grade coding-agent harness that combines four systems into one rigorously separated architecture:

1. NexAU-AHE-style component, experience, and decision observability, including evidence-backed harness evolution and falsifiable change attribution.
2. LemonHarness-style explicit workspace, state, time, snapshot, provenance, and recovery semantics.
3. A Codex-style tight model/tool loop for inspecting repositories, applying patches, executing commands, reading results, testing, and iterating.
4. Verification-Driven Development and Iterative Adversarial Refinement (VDD/IAR), implemented as policy-enforced writer, reviewer, and fixer roles with fresh contexts and disjoint capabilities.

Peritus is a Verus-first Rust system. The design does not confine Verus to a ceremonial state-machine crate. Every deterministic control-plane operation that Verus can physically express is to be implemented and verified in Verus: domain types, lifecycle transitions, capability authorization, lease ownership, budget accounting, acceptance freshness, finding resolution, scheduling decisions, event ordering, promotion policy, and pure planning/validation portions of workspace, patch, context, memory, evaluation, and evolution logic.

I/O and ecosystem seams that Verus cannot verify directly are isolated behind narrow effect interfaces. They return observations as data; verified reducers decide whether those observations may change authoritative state. Unverified code cannot directly accept a run, grant a capability, close a finding, promote a harness, or rewrite authoritative state.

Implementation is staged only to make parallel engineering safe. No stage is an MVP and no intermediate stage is a releasable product. Production readiness is the cumulative result of all slices, proofs, platform backends, adversarial tests, operational evidence, documentation, and release gates defined here.

## User-visible behavior

### Primary workflow

1. A user initializes Peritus in a Git repository with `peritus init`.
2. The user writes or imports a versioned acceptance contract describing required behavior, deterministic gates, review policy, resource limits, allowed capabilities, and completion evidence.
3. `peritus run --spec <path>` creates an isolated worktree and a durable run. The run can survive terminal closure, client restart, daemon restart, provider interruption, and host reboot.
4. A writer agent receives the immutable specification, repository context, scoped memory, and least-privilege tools. It works through a Codex-style inspect/edit/run/test loop.
5. Deterministic gates execute against the exact candidate revision. Gate results are content-addressed and revision-bound.
6. One or more fresh-context reviewers inspect the specification, candidate diff, relevant source, and gate evidence through a read-only view. They return typed, evidence-backed findings. They cannot mutate the workspace or accept their own work.
7. A fixer receives unresolved findings and a writable lease. It resolves, disputes with evidence, or requests an authorized waiver for each finding. Any mutation invalidates stale gate and review evidence.
8. The gate/review/fix loop continues until the acceptance policy is satisfied or ends explicitly as blocked, failed, cancelled, or requiring human authority. Exhausting a cycle or time budget never converts an incomplete run into success.
9. The user can attach to live output, inspect causal traces, replay state, compare attempts, review evidence, approve consequential actions, resume work, or roll back to a known snapshot.
10. Across a population of completed runs, a separately authorized evolution campaign may diagnose failure classes, propose componentized harness revisions, evaluate isolated variants, attribute changes, and recommend promotion. It cannot mutate the active production harness or its evaluators in place.

### Primary commands

The production CLI surface is expected to include:

- `peritus init`, `doctor`, `config explain`, and `capabilities`.
- `peritus run`, `attach`, `pause`, `resume`, `cancel`, `status`, and `inspect`.
- `peritus diff`, `events`, `trace`, `replay`, `artifacts`, and `export-evidence`.
- `peritus approve`, `deny`, `waive`, and `review`.
- `peritus snapshot`, `rollback`, `workspace status`, and `workspace recover`.
- `peritus spec validate`, `gate list`, `gate run`, and `gate explain`.
- `peritus harness list`, `diff`, `evaluate`, `evolve`, `promote`, `rollback`, and `history`.
- `peritus memory inspect`, `forget`, `quarantine`, and `rebuild`.
- `peritus plugin list`, `inspect`, `trust`, `disable`, and `remove`.
- `peritus daemon status`, `logs`, `stop`, and `recover`.

The TUI is a client of the same versioned application protocol as the CLI. Headless operation emits structured JSON/JSONL without changing runtime semantics.

### On-disk project contract

Proposed project-owned paths are:

```text
peritus.toml                     # committed project configuration
.peritus-harness/                # committed, componentized harness definition
  manifest.toml
  roles/
  prompts/
  tools/
  policies/
  gates/
  skills/
  memory/
  orchestration/
  providers/
.peritus/                        # ignored runtime state, never exposed writable to agents
  state.db
  objects/sha256/
  runs/
  worktrees/
  sockets/
  logs/
  backups/
```

Project source changes occur in dedicated Git worktrees, not inside `.peritus/`. Harness source is ordinary reviewed Git content. Runtime state is never made authoritative through ad hoc Markdown files.

## Requirements

### Product and orchestration requirements

- **R-PROD-001 — Contract-first execution.** Every run is governed by an immutable, content-addressed acceptance contract. A contract revision creates a new run revision and invalidates evidence from the previous contract.
- **R-PROD-002 — Tight developer loop.** The writer and fixer can inspect files, search, patch, execute, stream output, provide terminal input, run gates, and interpret failures without leaving the run.
- **R-PROD-003 — Role separation.** Writer, reviewer, fixer, evaluator, evolution agent, and human authority are distinct actor roles with policy-defined capabilities.
- **R-PROD-004 — Fresh adversarial review.** Reviewers receive fresh model contexts and read-only workspace snapshots. Reviewer independence requirements are part of the acceptance contract.
- **R-PROD-005 — Typed findings.** Reviews produce schema-valid findings with stable IDs, severity, category, evidence, affected revision, disposition, and resolution history.
- **R-PROD-006 — Evidence-bound acceptance.** A run can be accepted only when all required deterministic gates and review requirements pass against the same current specification, harness, and workspace revision tuple.
- **R-PROD-007 — Durable control.** Runs support attach, pause, resume, cancel, crash recovery, and deterministic replay.
- **R-PROD-008 — Explicit termination.** Budget exhaustion, repeated failures, reviewer disagreement, or unavailable authority yields a truthful non-success terminal state.
- **R-PROD-009 — Human authority.** Irreversible operations, policy exceptions, finding waivers, sensitive permission escalation, and harness promotion can require explicit human approval.
- **R-PROD-010 — Multi-provider runtime.** Model behavior is accessed through a capability-negotiated provider interface with first-party adapters and an OpenAI-compatible adapter. Provider differences cannot leak into the domain kernel.
- **R-PROD-011 — Extensible tools.** Built-in tools, MCP tools, and plugins share typed schemas, capability declarations, provenance, timeouts, cancellation, and audit events.
- **R-PROD-012 — Local and headless operation.** The full workflow is available through a local CLI and versioned machine protocol without requiring a hosted control plane.

### Workspace, execution, and persistence requirements

- **R-STATE-001 — Single authoritative journal.** Every authoritative state change is derived from an immutable, ordered event appended transactionally to the run journal.
- **R-STATE-002 — Rebuildable projections.** Query tables and cached summaries are disposable projections that can be rebuilt and checked against the journal.
- **R-STATE-003 — Content-addressed evidence.** patches, snapshots, command output, model records, reviews, and reports are addressed by digest and bound to their producing event.
- **R-STATE-004 — Exclusive mutation lease.** At most one actor owns a writable lease for a workspace revision at a time.
- **R-STATE-005 — Snapshot and rollback.** Every candidate boundary has a recoverable snapshot. Rollback creates new audit events and never deletes history.
- **R-STATE-006 — Atomic patching.** Patch application validates paths, preimage hashes, modes, line endings, and workspace revision before atomically committing a new candidate revision.
- **R-STATE-007 — Owned processes.** Every spawned process has an owner, lifecycle, cancellation token, bounded output path, resource policy, and observed terminal result.
- **R-STATE-008 — Crash consistency.** A crash at every persistence and execution boundary has a defined recovery outcome with no ambiguous accepted state.
- **R-STATE-009 — Schema evolution.** Stored events, configuration, protocols, and artifacts have explicit versions, compatibility fixtures, and forward migration rules.

### Observability and evolution requirements

- **R-OBS-001 — Full causal trace.** Every run has correlated spans and events across model calls, tool calls, approvals, processes, files, gates, reviews, fixes, memory retrieval, and state transitions.
- **R-OBS-002 — Layered evidence.** Raw records, normalized events, per-attempt analyses, cross-run failure classes, and executive summaries retain links to source event and artifact IDs.
- **R-OBS-003 — Secret-aware capture.** Default traces are redacted before normal persistence; controlled raw capture is separately encrypted, access-controlled, and retention-limited.
- **R-OBS-004 — Failure taxonomy.** Failures distinguish specification, model, provider, tool, policy, sandbox, workspace, verification, persistence, orchestration, infrastructure, and user-authority causes.
- **R-EVO-001 — Component observability.** Every evolvable harness component has an explicit type, owner path, schema, content hash, revision, compatibility range, and evaluation history.
- **R-EVO-002 — Evidence-backed changes.** Every evolution change declares failure evidence, root cause, target component, proposed mechanism, expected fixes, regression risks, resource impact, and falsification criteria.
- **R-EVO-003 — Isolated variants.** Candidate harnesses run in isolated worktrees against frozen evaluation definitions and fixed model/resource profiles.
- **R-EVO-004 — Honest evaluation.** Infrastructure failures remain visible; sealed holdouts and evaluator assets are inaccessible to candidate agents; task-specific overfitting is prohibited and detected.
- **R-EVO-005 — Statistical promotion.** Promotion uses paired outcomes, uncertainty estimates, safety constraints, cost/latency constraints, and explicit authority. A point-estimate improvement alone is insufficient.
- **R-EVO-006 — Reversible promotion.** Production harness revisions are immutable, content-addressed, and atomically promotable/rollbackable without rewriting prior campaigns.

### Verus and Rust requirements

- **R-VER-001 — Maximum feasible verification.** All deterministic control logic expressible in supported Verus Rust is implemented in verified crates or verified modules. Exclusion requires a recorded technical reason, owner, compensating test, and plan to revisit.
- **R-VER-002 — Functional core/effect shell.** Effectful crates request decisions from verified planners, execute only authorized effects, and submit observations to verified validators/reducers.
- **R-VER-003 — Centralized trust boundary.** `assume`, `admit`, axioms, `external_body`, external function specifications, and other trusted Verus mechanisms are forbidden outside the dedicated trust-boundary crate and its audited manifest.
- **R-VER-004 — No proof cheating.** CI rejects new trusted escape hatches, weakened specifications, or changed executable semantics unless an approved proof-change record explains and reviews them.
- **R-VER-005 — Verified release build.** Production artifacts are built through a pinned `cargo verus build --release` path, with a clean end-to-end verification run rather than focused or cached partial verification alone.
- **R-VER-006 — Boundary contracts.** Calls from ordinary Rust into verified code satisfy runtime-enforced preconditions; trusted adapters are checked by conformance, fault-injection, and refinement tests.
- **R-RUST-001 — Explicit domain types.** IDs, revisions, digests, sequence numbers, capabilities, budgets, and states use validated types with private fields and intentional constructors.
- **R-RUST-002 — Stable library APIs.** Public APIs do not expose implementation dependencies, use typed errors, document invariants, and follow semantic versioning from the first production release.
- **R-RUST-003 — Owned concurrency.** No detached tasks, unbounded queues, synchronous locks across `.await`, or unobserved process/task failures are permitted.
- **R-RUST-004 — Unsafe containment.** Unsafe code is prohibited outside explicitly designated platform/FFI modules, each with independently reviewed safety invariants, Miri coverage where possible, and an unsafe inventory.
- **R-RUST-005 — Maintainable source layout.** Crate and module boundaries follow responsibility and dependency direction. `lib.rs`/`main.rs` are composition surfaces, not implementation dumps. Generic `utils`, `helpers`, and `common` dumping grounds are prohibited.

### Security and operational requirements

- **R-SEC-001 — Model is untrusted.** Model output proposes actions; it never grants authorization or directly mutates authoritative state.
- **R-SEC-002 — Capability-based authorization.** Tools and effects require scoped, expiring, actor-bound capabilities checked immediately before execution.
- **R-SEC-003 — Symlink-safe path policy.** Filesystem authorization operates on canonical/handle-resolved targets and resists traversal, symlink races, mount tricks, case folding, and platform path aliases.
- **R-SEC-004 — Sandbox defense in depth.** Filesystem, process, network, environment, secret, and resource controls are enforced by platform backends rather than prompts alone.
- **R-SEC-005 — Metadata protection.** `.git`, `.peritus`, policy, trust, evaluator, secret, and approval metadata are read-only or invisible unless an explicit human-authorized operation requires access.
- **R-SEC-006 — Provenance separation.** User instructions, repository instructions, external content, memory, tool output, and application policy are typed by provenance and cannot silently change authority precedence.
- **R-SEC-007 — Supply-chain integrity.** Dependencies, plugins, release artifacts, SBOMs, and signatures are auditable and reproducible.
- **R-OPS-001 — Bounded resources.** Concurrency, retries, queues, output, disk use, model tokens, wall time, CPU, memory, and process counts have explicit limits and observable backpressure.
- **R-OPS-002 — Tier-one platforms.** Linux, macOS, and Windows have native production sandbox and process-lifecycle backends with cross-platform conformance tests.
- **R-OPS-003 — Diagnosability.** Every user-facing failure includes a stable error code, causal context, relevant IDs, and an actionable recovery route.

## Acceptance criteria

Peritus is production-ready only when all of the following are demonstrated on the release commit:

1. A clean checkout passes formatting, strict Clippy, unit, integration, documentation, compatibility, property, concurrency, Miri-eligible, fuzz smoke, security, and end-to-end suites on all tier-one platforms.
2. `cargo verus verify --workspace` and `cargo verus build --release` succeed from a clean locked dependency graph with no unapproved trusted construct.
3. The proof obligation inventory reports every deterministic decision function as verified or records an approved, narrowly scoped exclusion with compensating evidence.
4. Machine checks show no `assume`, `admit`, axiom, `external_body`, or equivalent outside the trust-boundary allowlist; every allowlisted entry is linked to a threat analysis and refinement test.
5. Model, tool, and ordinary Rust callers cannot construct privileged tokens, accepted states, closed findings, current evidence, or promoted harness states without going through verified transitions.
6. A recorded state-machine test suite attempts every illegal lifecycle edge and proves/rejects it consistently in Verus, property tests, and protocol conformance tests.
7. Power-loss and crash injection at every journal, blob, snapshot, lease, patch, gate, and promotion commit point recovers to the documented state without journal divergence or false success.
8. Deterministic replay from an empty projection database reproduces authoritative state and all acceptance decisions byte-for-byte for the compatibility corpus.
9. A malicious-repository suite covers traversal, symlink races, submodule/worktree tricks, case-insensitive aliases, device paths, shell injection, poisoned instructions, oversized output, terminal escapes, and secret-exfiltration attempts.
10. Each tier-one sandbox passes the common capability conformance suite and an independent escape-focused security review.
11. Writer, reviewer, and fixer isolation tests prove that read-only actors cannot mutate and that writable actors cannot approve or waive their own results.
12. Any candidate mutation invalidates prior gate and review evidence; stale evidence cannot be used to accept a new revision.
13. Budget or retry exhaustion produces a non-success terminal state and a complete evidence bundle; no timeout path marks work accepted.
14. A daemon killed and restarted during every active lifecycle phase resumes, reconciles, or explicitly fails owned tasks without orphaned authoritative work.
15. Provider contract tests cover streaming interruption, duplicated events, out-of-order chunks, retry-after, malformed structured output, partial tool calls, cancellation, and idempotent retry.
16. The event store migrates every historical schema fixture forward, rejects corrupt/hash-divergent journals, and can export a portable evidence bundle.
17. Evolution red-team tests demonstrate that candidates cannot read sealed answers, edit evaluators, change model/resource profiles, bypass safety policy, or promote themselves.
18. Promotion requires all configured statistical, correctness, safety, resource, and authority gates against immutable candidate and baseline revisions; rollback is atomic and preserves both histories.
19. Observability reports cite source event/artifact IDs, distinguish infrastructure failure from task failure, and redact seeded secrets from default logs and exported evidence.
20. Load and soak tests meet documented service-level objectives for concurrent runs, event append latency, terminal streaming, memory bounds, cancellation latency, and recovery time.
21. Every public command and protocol method has reference documentation, examples, stable error codes, and end-to-end tests.
22. Architecture checks report no dependency cycles, forbidden upward dependencies, god root modules, unowned generated files, or public API leakage of implementation crates.
23. The final independent writer/reviewer/fixer campaign completes representative Rust, TypeScript, Python, Java, and mixed-repository tasks with reproducible evidence and no manual state repair.
24. Release artifacts are reproducible, signed, accompanied by SBOM/provenance, license notices, migration/recovery documentation, and a completed security review.
25. There are no quarantined tests, ignored failing tests, unresolved release-blocking findings, undocumented unsafe blocks, or placeholder production implementations.

### Requirement traceability

Every requirement group has implementation ownership and observable release evidence. Individual requirements inherit the evidence rows for their group; narrower tests and proof obligations are enumerated by the owning slice before Gate B/C interface freeze.

| Requirements | Primary owning slices | Acceptance evidence |
|---|---|---|
| `R-PROD-001`, `R-PROD-002`, `R-PROD-003`, `R-PROD-004`, `R-PROD-005`, `R-PROD-006` | B0, B2, D0–D2, E0 | criteria 4–6 and 11–13; contract, lifecycle, role, finding, freshness, and acceptance proofs/tests |
| `R-PROD-007`, `R-PROD-008`, `R-PROD-009` | B0, B1, C0–C2, E0, G0 | criteria 7, 13, and 14; pause/resume/cancel/recovery and approval scenario matrices |
| `R-PROD-010`, `R-PROD-011`, `R-PROD-012` | C4, C5, G0–G3 | criteria 15 and 21; provider/tool/plugin/client conformance and black-box CLI/headless tests |
| `R-STATE-001`, `R-STATE-002`, `R-STATE-003` | C0, C7 | criteria 7, 8, 16, and 19; crash-consistent append, replay, integrity, artifact, and evidence-export suites |
| `R-STATE-004`, `R-STATE-005`, `R-STATE-006`, `R-STATE-007`, `R-STATE-008`, `R-STATE-009` | B1, C0–C2 | criteria 7, 9, 12, 14, and 16; lease, path, patch, process, recovery, and migration evidence |
| `R-OBS-001`, `R-OBS-002`, `R-OBS-003`, `R-OBS-004` | C7, E2 | criteria 19 and 20; causal trace, taxonomy, citation, redaction, backpressure, and export tests |
| `R-EVO-001`, `R-EVO-002`, `R-EVO-003`, `R-EVO-004`, `R-EVO-005`, `R-EVO-006` | E1–E3, F0 | criteria 17 and 18; component, sealed-eval, attribution, statistical promotion, and rollback campaigns |
| `R-VER-001`, `R-VER-002`, `R-VER-003`, `R-VER-004`, `R-VER-005`, `R-VER-006` | A0, A1, B0–B3, all V/H owners | criteria 2–6 and 22; clean verification/build, trust audit, proof inventory, wrapper, and refinement evidence |
| `R-RUST-001`, `R-RUST-002`, `R-RUST-003`, `R-RUST-004`, `R-RUST-005` | A0/A1 and every crate owner | criteria 1, 20, 22, 24, and 25; strict lint/test/docs, architecture checks, unsafe inventory, and release audit |
| `R-SEC-001`, `R-SEC-002`, `R-SEC-003`, `R-SEC-004`, `R-SEC-005`, `R-SEC-006`, `R-SEC-007` | B1, C1–C4, C6, G3, H0 | criteria 9–12, 17–19, and 24; malicious-repository, sandbox, role, secret, extension, evolution, and supply-chain reviews |
| `R-OPS-001`, `R-OPS-002`, `R-OPS-003` | C2/C3, D3, G0, H1–H3 | criteria 10, 14, 20, 21, and 23; resource, platform, recovery, SLO, diagnostics, and representative-task evidence |

## Current architecture

Project-Peritus currently contains only a one-line README, MIT license, Crosslink repository metadata, and local ignored reference clones. There is no Cargo workspace, public API, stored production data, compatibility obligation, or existing implementation to migrate.

The reference repositories establish concrete patterns but are evidence, not upstream architecture authority:

- NexAU-AHE exposes seven file-level harness component classes, persists raw and distilled trace evidence, records predicted impact in change manifests, compares task-level flips, and rolls back harmful changes. Its own paper notes non-additive component interaction and poor regression foresight, so Peritus must use stronger isolated evaluation and promotion gates.
- LemonHarness demonstrates explicit workspace operations, snapshots, execution records, phase budgeting, memory, privilege tiers, and a fresh-context implementer/reviewer loop. Peritus adopts the semantics while replacing in-memory authority and string-prefix path policy with durable state and hardened capabilities.
- Codex CLI demonstrates a mature Rust decomposition around protocol events, session/turn state, terminal streaming, patch application, approvals, sandboxing, rollouts, and clients. Peritus adopts those separation principles without forking Codex or reproducing its monolithic protocol surfaces.
- Verus supports multi-crate Cargo workspaces, executable/specification/proof modes, external function specifications, and state-machine/tokenized-state-machine proof patterns. Trusted assumptions can invalidate guarantees, so Peritus treats the trust boundary as a versioned, reviewed product artifact.

Because the repository is empty, all paths below are proposed. Once scaffolded, the design must be revised against actual interfaces before any incompatible public change.

## Proposed design

### Architectural principles

1. **Verified decisions, isolated effects.** Deterministic planning and authorization live in Verus. Effect adapters perform exactly the authorized request and return an observation. A verified reducer validates the observation before emitting authoritative events.
2. **Events before projections.** The journal is the record; UI state, summaries, indexes, metrics, and memory views are rebuildable.
3. **Revision tuples prevent stale evidence.** Every claim binds `(spec_revision, harness_revision, workspace_revision, policy_revision, provider_profile_revision)`.
4. **Roles are capabilities, not prompt labels.** A reviewer is read-only because it lacks a mutation capability, regardless of what its prompt says.
5. **No implicit success.** Only a verified acceptance transition can create an accepted state.
6. **No hidden self-modification.** Production harness, evaluator, security root, and trust policy are immutable during ordinary runs and independently controlled during evolution.
7. **Protocols precede implementations.** Parallel slices implement frozen contracts and common conformance fixtures.
8. **Local-first, client/server internally.** A durable local daemon owns state and work; CLI and TUI are replaceable clients.
9. **Cohesive crates, small modules.** Crates represent ownership and dependency boundaries, not arbitrary line-count sharding.
10. **Evidence is a first-class domain object.** Tests, reviews, traces, approvals, and analyses are typed, revision-bound, and queryable.

### Process topology

```text
 CLI / TUI / automation
          │ versioned local app protocol
          ▼
 ┌──────────────────────── Peritus daemon ──────────────────────────┐
 │ command intake → verified authorization → scheduler             │
 │        │                    │                    │                │
 │        ▼                    ▼                    ▼                │
 │ orchestrator         provider workers      tool/effect workers   │
 │        │                    │                    │                │
 │        └──────── observations and proposed commands ────────────┤
 │                             │                                    │
 │                    verified state reducer                        │
 │                             │                                    │
 │              journal + projections + artifact store              │
 │                             │                                    │
 │                traces / metrics / evidence views                 │
 └─────────────────────────────┼────────────────────────────────────┘
                               │ authorized effects only
                 isolated Git worktrees and OS sandboxes
```

The daemon is the sole writer to authoritative state. Worker crashes are observations, not implicit state transitions. Workers communicate through bounded typed channels and cannot hold database transactions or synchronous locks across asynchronous work.

### Nested control loops

Peritus deliberately implements three loops at different authority levels:

```text
Inner execution loop (one actor, one turn)
  inspect → reason → propose tool → authorize → execute → observe → repeat

Adversarial delivery loop (one run)
  writer → deterministic gates → reviewer quorum → fixer → gates → reviewer

Harness evolution loop (many completed runs)
  evaluate → normalize → diagnose → propose variants → evaluate → attribute
  → security/statistical/human promotion → immutable production revision
```

The inner loop cannot change its acceptance contract. The delivery loop cannot change its production harness or evaluator. The evolution loop cannot edit sealed evaluation data, the security root, or its own promotion rules.

### Verification classes

Every crate declares one of four verification classes in workspace metadata:

- **V — Verified:** executable decision logic and its specifications/proofs are checked by Verus. Ordinary Rust builds use the same executable bodies after ghost erasure.
- **H — Hybrid:** effect-free planning/validation modules are verified; I/O modules are ordinary Rust and satisfy audited boundary contracts.
- **T — Trusted effect boundary:** unavoidable OS, database driver, TLS, Git, provider, or FFI integration. It contains no authority decisions and is part of the explicit trusted computing base.
- **C — Client/presentation:** cannot mutate authoritative state except by submitting protocol commands that are reauthorized by the daemon.

The goal is not a cosmetic verified-line percentage. The release requirement is 100% verification of supported deterministic authority logic and a shrinking, enumerated TCB. Any H/T function excluded from proof must appear in `verification/exclusions.toml` with symbol, unsupported feature, risk, compensating evidence, owner, review date, and upstream/revisit plan.

### Proposed Cargo workspace

The workspace is grouped physically by subsystem. Package names remain globally unique. Root manifests hold only shared metadata, dependency pins, profiles, and lint policy.

#### Foundation and formal model

| Proposed crate | Class | Responsibility | Permitted dependency direction |
|---|---:|---|---|
| `peritus-types` | V | Validated IDs, revisions, digests, sequence numbers, actor identities, capability names, resource quantities, and time-independent value types | `vstd` and tightly audited primitive crates only |
| `peritus-codec` | H | Versioned event envelopes, canonical raw-byte hashing, bounded decoding, compatibility tags, and wire/domain conversion | `peritus-types`; serialization libraries confined to adapter modules |
| `peritus-spec` | V | Acceptance-contract model, gate graph, evidence requirements, reviewer independence policy, and contract validation | `peritus-types` |
| `peritus-kernel` | V | Session, run, attempt, turn, action, review, waiver, and acceptance state machines; command authorization and event reduction | `peritus-types`, `peritus-spec`, `peritus-policy`, `peritus-budget` |
| `peritus-policy` | V | Capability algebra, authority hierarchy, policy intersection, escalation rules, protected resources, and authorization proofs | `peritus-types` |
| `peritus-budget` | V | Monotonic token/time/cost/attempt budgets, reservations, refunds, and exhaustion semantics | `peritus-types` |
| `peritus-protocol` | H | Public domain commands/events, schemas, stable error codes, forward-compatible envelopes, and generated client types | foundation crates only |
| `peritus-tcb` | T | Sole location for Verus external specifications and trusted assumptions; generated trust manifest | foundation types only; cannot depend on orchestration or clients |

#### Persistence and durable state

| Proposed crate | Class | Responsibility |
|---|---:|---|
| `peritus-journal` | H/T | Transactional append, per-aggregate sequencing, hash chaining over stored bytes, outbox, fsync policy, integrity scan, and export |
| `peritus-projection` | V/H | Pure event folding, projection schemas, rebuild/check logic, and query-model adapters |
| `peritus-artifact-store` | H/T | Streaming content-addressed blobs, atomic finalize, digest verification, encryption metadata, quotas, and garbage-collection plans |
| `peritus-migrations` | H | Ordered schema migrations, preflight, copy-on-risk backup, compatibility fixture runner, and recovery metadata |
| `peritus-leases` | V/H | Verified lease state and expiry decisions plus durable compare-and-swap persistence adapter |
| `peritus-evidence` | V/H | Revision tuples, evidence manifests, freshness validation, causal links, and portable evidence-bundle assembly |

#### Workspace, patching, execution, and security

| Proposed crate | Class | Responsibility |
|---|---:|---|
| `peritus-git` | H/T | Repository discovery, object IDs, worktree lifecycle, diff/status, signed revision metadata, and Git subprocess/libgit boundary |
| `peritus-patch` | V/H | Patch AST, path/preimage validation, edit planning, conflict detection, text/binary policy, and atomic application adapter |
| `peritus-workspace` | V/H | Workspace lifecycle, mutation leases, protected paths, snapshots, rollback plans, dirty-state reconciliation, and revision emission |
| `peritus-process` | H/T | Owned child processes, PTY/pipe streaming, process groups/job objects, input, cancellation, timeout, output spooling, and terminal result |
| `peritus-sandbox` | V/H | Platform-neutral sandbox plan, capability-to-policy compilation, conformance interface, and lifecycle |
| `peritus-sandbox-linux` | T | Namespaces/bubblewrap or equivalent, Landlock/seccomp where available, cgroups, mount policy, and Linux probes |
| `peritus-sandbox-macos` | T | Seatbelt profile compilation, process groups, filesystem/network policy, and macOS probes |
| `peritus-sandbox-windows` | T | Restricted token/AppContainer strategy, job objects, ACL policy, path normalization, and Windows probes |
| `peritus-network` | V/H/T | Verified allow/deny matching and request planning plus managed proxy/DNS/TLS observation boundary |
| `peritus-secrets` | H/T | Secret references, OS keychain integration, environment injection, redaction fingerprints, zeroization, and leak detection |
| `peritus-approval` | V/H | Approval request model, risk classification inputs, actor-bound decisions, expiry, replay protection, and UI-safe rendering |

#### Tools and extension system

| Proposed crate | Class | Responsibility |
|---|---:|---|
| `peritus-tool-protocol` | V/H | Tool identity, schema digest, invocation/result/event types, output limits, idempotency, and capability declarations |
| `peritus-tool-router` | V | Tool exposure, role filtering, capability checks, invocation lifecycle, and result acceptance decisions |
| `peritus-tools-fs` | H/T | Read, list, search, metadata, and patch-backed file operations through workspace handles |
| `peritus-tools-shell` | H/T | Structured argv execution and separately privileged shell-script execution through `peritus-process` |
| `peritus-tools-git` | H/T | Read-only and authorized mutation Git tools backed by `peritus-git` |
| `peritus-tools-quality` | H | Gate discovery and explicit gate invocation without owning gate policy |
| `peritus-mcp` | H/T | MCP transport/client lifecycle, schema normalization, elicitation, cancellation, provenance, and capability mediation |
| `peritus-plugin-sdk` | C | Stable out-of-process/Wasm component contracts, generated bindings, capability manifest, and test kit |
| `peritus-plugin-host` | H/T | Plugin discovery, signature/trust checks, process/Wasm isolation, quotas, protocol mediation, and lifecycle |

#### Models, context, memory, and agent loop

| Proposed crate | Class | Responsibility |
|---|---:|---|
| `peritus-model-protocol` | H | Provider-neutral requests, streaming items, tool-call deltas, usage, reasoning summaries, errors, capabilities, and idempotency keys |
| `peritus-provider-core` | H/T | Shared HTTP/TLS/retry/backoff/rate-limit mechanics with no provider-specific domain policy |
| `peritus-provider-openai` | T | OpenAI Responses adapter and capability negotiation |
| `peritus-provider-anthropic` | T | Anthropic Messages adapter and capability negotiation |
| `peritus-provider-google` | T | Google model adapter and capability negotiation |
| `peritus-provider-compatible` | T | Explicitly profiled OpenAI-compatible endpoints; no silent feature assumptions |
| `peritus-context` | V/H | Provenance-typed context graph, precedence, selection, token-budget planning, compaction validation, and render plans |
| `peritus-memory` | V/H | Scoped memory records, provenance, confidence, expiry, retrieval ranking plan, quarantine, feedback, and rebuildable indexes |
| `peritus-agent` | V/H | Inner inspect/edit/run/test turn loop, model/tool state, streaming assembly, retry decisions, and completion proposal |
| `peritus-role` | V/H | Writer/reviewer/fixer/evaluator/evolver role definitions, context policy, allowed capabilities, and independence attestations |

#### Gates, review, orchestration, and collaboration

| Proposed crate | Class | Responsibility |
|---|---:|---|
| `peritus-gates` | V/H | Gate DAG planning, discovery, execution requests, evidence parsing, freshness, aggregation, and failure classification |
| `peritus-review` | V/H | Finding schema, reviewer quorum, severity policy, duplicate reconciliation, disposition, resolution, trend/oscillation detection, and review evidence |
| `peritus-orchestrator` | V/H | Writer → gates → reviewer → fixer control loop and only route to acceptance transition |
| `peritus-scheduler` | V/H | Fair bounded scheduling, reservations, priorities, dependencies, cancellation, worker ownership, and backpressure |
| `peritus-collaboration` | V/H | Agent identities, parent/root causality, messages, task delegation, join semantics, and bounded fan-out |
| `peritus-quality-policy` | V | Production gate policy, waiver constraints, reviewer independence requirements, and release evidence rules |

#### Observability, diagnosis, evaluation, and evolution

| Proposed crate | Class | Responsibility |
|---|---:|---|
| `peritus-trace` | H | Normalized trace events, causal graph, redaction pipeline, raw-vault references, trace query, and export |
| `peritus-telemetry` | H/T | OpenTelemetry spans/metrics/log export, offline spool, exporter backpressure, and privacy controls |
| `peritus-debugger` | V/H | Trace selection, failure clustering inputs, evidence-linked analysis jobs, report validation, and attribution graph |
| `peritus-eval` | V/H | Dataset/task manifests, frozen profiles, rollout accounting, pass@k and uncertainty calculations, stability and paired comparison |
| `peritus-evolution` | V/H | Component registry, change manifests, candidate variants, campaign state machine, falsification, selection, promotion proposal, and rollback |
| `peritus-harness` | V/H | Harness manifest loading, component schemas, content graph, compatibility, immutable revisions, and materialization |

#### Application surfaces and engineering support

| Proposed crate | Class | Responsibility |
|---|---:|---|
| `peritus-app-protocol` | H | Local daemon request/response/subscription protocol and compatibility fixtures |
| `peritus-daemon` | H | Composition root, command intake, worker supervision, recovery, local transport, and graceful shutdown |
| `peritus-cli` | C | Scriptable command client, JSON output, exit codes, and shell completion |
| `peritus-tui` | C | Interactive run, approval, trace, diff, review, and evolution views |
| `peritus-test-support` | C | Deterministic clocks, fake providers, fake tools, temporary repositories, event builders, fault points, and fixture utilities |
| `peritus-conformance` | C | Provider, tool, plugin, sandbox, journal, protocol, and replay conformance suites |
| `peritus-benchmarks` | C | Criterion/load/soak harnesses and stable performance datasets |
| `xtask` | C | Schema generation, architecture checks, proof/TCB checks, fixtures, licenses, release assembly, and reproducibility |

### Dependency and source-layout rules

The physical group directories are proposed as `crates/foundation`, `crates/state`, `crates/runtime`, `crates/tools`, `crates/model`, `crates/orchestration`, `crates/observe`, and `crates/app`. Cargo package boundaries, not directories, enforce dependencies.

- Foundation crates never depend on storage, Tokio, providers, tools, UI, or platform crates.
- Platform/effect crates depend inward on plans and types; verified crates never depend outward on effect implementations.
- Client crates depend only on public/app protocols and presentation libraries.
- Provider crates cannot depend on orchestration, workspace, or storage.
- Plugins never link into the daemon address space as arbitrary native dynamic libraries.
- Every crate has a named owner, README with invariants and dependency policy, public API docs, unit tests, and a `tests/` directory for boundary behavior where warranted.
- `src/lib.rs` and binary `main.rs` declare modules, re-export intentional API, and compose dependencies. Business logic in roots is rejected by architecture checks.
- A source file has one named responsibility. A 400-line soft budget triggers review; a 700-line hard budget requires a checked-in exception naming why further decomposition would harm cohesion. Generated files and proof-generated expansions are measured separately.
- Generic `utils.rs`, `helpers.rs`, `common.rs`, `misc.rs`, or `manager.rs` modules are prohibited unless the name is domain-qualified and responsibility is documented.
- Libraries use typed `thiserror`-style errors with stable categories and source chaining. Dynamic context errors are allowed only at application composition/reporting boundaries.
- Public fields are private unless they are intentionally stable wire data. Constructors enforce validity; typestate or enums prevent invalid lifecycle combinations.
- Feature flags are additive capability/build choices, never hidden semantic modes that change correctness guarantees.
- The workspace denies warnings, undocumented unsafe, unexpected cfgs other than the pinned Verus cfg, and accidental public dependency leakage.
- `xtask architecture-check` reads Cargo metadata and an allowlisted dependency-layer file, rejecting upward dependencies, forbidden crate pairs, cycles, root-module logic, unowned schema changes, and size exceptions without rationale.

### Verus-first implementation strategy

#### Functional-core/effect-shell protocol

All consequential operations follow the same five-step shape:

1. **Plan:** verified code converts current state and a command into a typed effect plan or a typed rejection.
2. **Authorize:** verified policy proves the actor has the exact capability for the plan and revision.
3. **Execute:** a narrow H/T adapter performs the requested I/O without making domain decisions.
4. **Attest:** the adapter returns a bounded observation containing request ID, normalized result, artifact digests, and platform evidence.
5. **Reduce:** verified code checks that the observation matches the outstanding plan and either emits authoritative events or records a failure.

An adapter may report “file written” or “process exited”; it cannot report “run accepted.” An SQLite transaction may durably append an already authorized event batch; it cannot invent an event. A provider may return a tool call; it cannot make the call authorized.

#### Verified state machines

`peritus-kernel`, `peritus-policy`, `peritus-leases`, `peritus-review`, and `peritus-evolution` use Verus `state_machine!` or `tokenized_state_machine!` where the ownership model benefits from linear ghost state. Executable transition functions mirror the specification relations. Each transition has:

- explicit preconditions and postconditions;
- an inductiveness proof for every global invariant;
- negative tests for rejected preconditions;
- property tests comparing executable transitions to a simple reference model;
- serialization/replay tests ensuring wire conversion cannot create a state unavailable to the verified constructor.

The initial proof inventory includes:

| Invariant | Required claim |
|---|---|
| `INV-001 LegalTransition` | Every authoritative state is reachable from an initializer through declared transitions only. |
| `INV-002 EventSequence` | Aggregate sequence numbers start at one, advance exactly once per event, never repeat, and never decrease. |
| `INV-003 RevisionFreshness` | Gate, review, approval, and waiver evidence is valid only for its exact revision tuple. |
| `INV-004 AcceptanceCompleteness` | `Accepted` implies the current contract is valid, all required gates pass, reviewer policy is satisfied, blockers are resolved/authorized, and no required evidence is stale. |
| `INV-005 NoImplicitSuccess` | Failure, cancellation, exhaustion, shutdown, and recovery transitions cannot produce `Accepted`. |
| `INV-006 ExclusiveWriter` | At most one live mutation lease exists for a workspace generation. |
| `INV-007 RoleSeparation` | Reviewer/evaluator capabilities exclude workspace mutation; writer/fixer capabilities exclude acceptance, waiver, and promotion authority. |
| `INV-008 CapabilityScope` | A capability authorizes only its actor, resource set, operation set, revision, environment, and validity interval. |
| `INV-009 ApprovalReplaySafety` | An approval resolves one matching pending request and cannot be replayed for another action or revision. |
| `INV-010 FindingConservation` | A finding remains open until a current-revision resolution, reviewer-confirmed invalidation, superseding finding, or authorized waiver is recorded. |
| `INV-011 GateDAG` | A gate runs only after its dependencies and cannot be counted twice toward a quorum. |
| `INV-012 BudgetMonotonicity` | Consumption never decreases; reservations cannot exceed available resources; exhaustion cannot be bypassed by retry. |
| `INV-013 ProcessOwnership` | Every live process is owned by one run/action and has one eventual observed terminal disposition. |
| `INV-014 SnapshotReachability` | Rollback targets only recorded snapshots of the same workspace lineage and creates a new revision. |
| `INV-015 EvidenceClosure` | Every authoritative claim references existing digest-verified evidence or an explicit typed absence permitted by policy. |
| `INV-016 SpecImmutability` | A run never changes its governing spec digest; amendment creates a new revision lineage. |
| `INV-017 HarnessImmutability` | An ordinary run never changes its governing harness digest. |
| `INV-018 EvaluatorIsolation` | Candidate/evolution capabilities cannot address sealed evaluator resources or promotion policy. |
| `INV-019 PromotionSafety` | Production promotion implies immutable evaluation inputs, satisfied statistical/safety constraints, compatible schema, and required authority. |
| `INV-020 CausalParentage` | Every non-root command/event belongs to exactly one run and has a valid causal parent/root chain. |
| `INV-021 MemoryNonAuthority` | Retrieved memory can contribute context but cannot change policy, provenance, acceptance, or capabilities. |
| `INV-022 PolicyMonotonicity` | Lower-authority configuration layers can tighten but cannot silently loosen protected policy. |

Liveness properties that cannot be proven solely as safety invariants are documented separately and tested under fair-scheduler assumptions: authorized queued work eventually starts or receives a terminal scheduling failure; cancellation eventually reaches every owned child; durable outbox entries eventually deliver or expose a terminal delivery fault.

#### Trust-boundary discipline

Verus documentation warns that assumed specifications and calls from unverified Rust can subvert verification guarantees. Peritus therefore uses these rules:

- `peritus-tcb` is the only crate allowed to declare external specifications or trusted bodies.
- `verification/trust.toml` records symbol, upstream version, assumed contract, threat if false, refinement/conformance tests, reviewer, and expiration/review date.
- Trusted specifications are as weak as possible. They specify returned bytes or OS observations, not desired domain outcomes.
- Verified public entry points expose safe ordinary-Rust wrappers that validate every `requires` condition at runtime before calling verified executable code.
- Types that represent authorization or accepted state have private fields and cannot be deserialized directly. Wire data is converted through checked constructors.
- CI diffs executable code, specifications, and proof code independently. A proof change that weakens an `ensures`, strengthens a `requires`, adds an assumption, or changes executable semantics is release-blocking until explicitly reviewed.
- `xtask verify-trust` rejects `assume`, `admit`, `axiom`, `external_body`, `external`, and external-specification markers outside the allowlist. It also rejects allowlist entries without a live issue and owner.
- Full verification is run without `focus` for pre-commit, protected-branch, and release gates. Solver resource limits are pinned and proof timeouts are failures, not skips.
- Proof code is formatted, documented, reviewed for quantifier triggers and brittleness, and tested against expected counterexamples when a specification is intentionally tightened.

### Domain model

The authoritative domain uses stable opaque IDs and immutable revisions. Timestamps aid diagnostics but never determine event order.

#### Core aggregates

- **Project:** repository identity, roots, configuration revision, trust policy, and known harnesses.
- **HarnessRevision:** immutable component graph and compatibility metadata.
- **AcceptanceSpec:** immutable contract revision with goals, exclusions, gates, review policy, resource policy, and required evidence.
- **Session:** user interaction lifetime and durable command queue.
- **Run:** one attempt to satisfy one spec with one harness/policy/provider profile tuple.
- **Attempt:** bounded writer/fixer execution period within a run.
- **Turn:** one model-response lifecycle, possibly containing several tool/action steps.
- **Action:** proposed effect plus authorization, execution, artifacts, and result.
- **Workspace:** isolated repository lineage, worktree, lease generation, snapshots, and current revision.
- **GateExecution:** one gate definition evaluated against one exact revision tuple.
- **ReviewCycle:** reviewer assignments, findings, resolutions, quorum, and decision.
- **Finding:** durable issue with stable identity and revision-aware lifecycle.
- **ApprovalRequest:** consequential operation awaiting an authorized decision.
- **MemoryRecord:** scoped, provenance-bearing derived knowledge that is never authoritative.
- **EvaluationCampaign:** frozen tasks/profile and candidate rollout outcomes.
- **EvolutionCampaign:** evidence, proposed changes, variants, attribution, and promotion lifecycle.

#### Run lifecycle

```text
Created
  → PreparingWorkspace
  → Ready
  → Writing ↔ WaitingForActionApproval
  → Validating
      ├─ deterministic failure → Fixing
      └─ gates pass → Reviewing
  → Reviewing
      ├─ blockers → Fixing → Validating
      ├─ authority needed → WaitingForAuthority
      └─ policy satisfied → Accepting → Accepted

From any nonterminal state:
  → Pausing → Paused → Resuming → prior resumable phase
  → Cancelling → Cancelled
  → Failing → Failed
  → Recovering → reconciled phase | Failed

Budget/cycle exhaustion:
  → NeedsHuman | Failed (never Accepted)
```

The previous resumable phase is stored as typed state, not inferred from logs. `Accepted`, `Cancelled`, and `Failed` are terminal for that run revision. Additional work creates a successor run linked by `supersedes`.

#### Review lifecycle

```text
Planned → Assigned → Running → Submitted → Reconciled
  → ChangesRequired → FixInProgress → AwaitingRevalidation → Planned
  → AuthorityRequired
  → Satisfied
  → Failed
```

Findings have `Open`, `FixProposed`, `Resolved`, `Disputed`, `WaiverRequested`, `Waived`, `Superseded`, and `Invalidated` states. Only verified transitions can close them. A workspace mutation after review invalidates review satisfaction unless the contract explicitly declares a review evidence class unaffected by that mutation; production defaults invalidate all code-quality review evidence.

#### Evolution lifecycle

```text
Draft → Frozen → BaselineRunning → Diagnosing → Proposing
  → VariantsRunning → Attributing → PromotionReview
  → Promoted | Rejected | Failed | Cancelled
```

Freezing commits dataset manifests, evaluator digests, model/provider profile, budgets, concurrency, harness baseline, random seeds, and metric definitions. Any change creates a successor campaign, never an in-place edit.

### Command and event protocol

Public commands express intent and include expected aggregate sequence/revision where races matter. The daemon authenticates the submitting actor, calls verified authorization, and appends the resulting event batch in one transaction.

Representative command families are:

- project/configuration: `RegisterProject`, `ApplyConfigRevision`, `RegisterHarness`;
- run: `CreateRun`, `PrepareWorkspace`, `StartAttempt`, `PauseRun`, `ResumeRun`, `CancelRun`, `RecoverRun`;
- action: `ProposeAction`, `AuthorizeAction`, `RecordActionStarted`, `RecordActionObservation`, `CancelAction`;
- candidate/evidence: `SubmitCandidate`, `RecordGateObservation`, `RecordEvidence`, `InvalidateEvidence`;
- review: `AssignReviewer`, `SubmitReview`, `ReconcileFindings`, `ProposeResolution`, `ConfirmResolution`, `RequestWaiver`;
- authority: `RequestApproval`, `ResolveApproval`;
- acceptance: `EvaluateAcceptance`, `AcceptRun`, `FailRun`;
- evolution: `FreezeCampaign`, `RecordRollout`, `SubmitDiagnosis`, `ProposeHarnessChange`, `SelectVariant`, `RequestPromotion`, `ResolvePromotion`, `RollbackHarness`.

Event families are split into domain modules rather than one giant enum source file. A small top-level envelope contains schema version, event ID, aggregate ID/type, sequence, causal IDs, actor, timestamp, payload type, payload bytes, previous hash, and event hash. Payload types are closed per major protocol version but unknown future payloads remain storable and exportable by older readers.

Public API rules:

- Commands are not deserialized directly into privileged domain objects.
- Events are immutable after append.
- Consumers acknowledge an explicit event cursor; subscriptions are at-least-once and clients deduplicate by event ID.
- Idempotency keys prevent duplicated client/provider submissions from duplicating effects.
- Stable error codes are separate from prose and include retryability and responsible subsystem.
- Schemas are generated from canonical Rust definitions and checked into a generated directory owned by `xtask`; generated diffs must accompany source changes.
- Compatibility tests read every prior released fixture. Fields are additive within a major version; removal or semantic reinterpretation requires a new major protocol and migration bridge.

### Persistence and data layout

#### Authoritative store

SQLite in WAL mode is the local authoritative database. One daemon process owns write access. Event append uses `synchronous=FULL` for authoritative transitions. Performance-sensitive derived telemetry can use a separately documented weaker durability class, never shared with acceptance state.

Core tables are conceptually:

```text
events(
  aggregate_id, sequence, event_id, aggregate_kind,
  schema_major, schema_minor, payload_kind, payload_bytes,
  actor_id, correlation_id, causation_id, root_id,
  observed_at, previous_hash, event_hash
)
commands(idempotency_key, actor_id, request_digest, status, result_event_range)
outbox(event_id, destination, attempt, not_before, status)
projections(name, version, checkpoint_sequence, payload)
artifacts(digest, size, media_type, encryption, created_by_event, state)
leases(resource_id, generation, holder, expires_at, event_sequence)
schema_migrations(version, digest, applied_at, release)
```

The database stores the exact payload bytes that were hashed. Hash verification never depends on reserialization. Per-aggregate chains detect deletion/reordering; signed evidence exports add a bundle-level manifest and Merkle root. A hash chain is tamper-evidence, not a substitute for host security.

#### Artifact store

Large payloads stream into `.peritus/objects/sha256/<prefix>/<digest>` through temporary files. Finalization fsyncs content and parent directories, verifies size/digest, atomically renames, then records availability. Partial blobs are never referenced by authoritative evidence. Garbage collection is mark-and-sweep from journal references with quarantine before deletion.

Default persisted artifacts include complete command stdout/stderr streams, terminal metadata, patch inputs/results, snapshots, normalized model items, gate reports, reviews, trace reports, and evaluation outputs. User-visible truncation affects rendering only; full bounded output remains in the artifact store subject to quota/retention policy.

Sensitive raw model/tool material uses envelope encryption with a project data key protected by the OS credential store. Redacted normalized records remain queryable. Export requires an explicit profile declaring whether sensitive blobs are omitted, redacted, or encrypted for named recipients.

#### Projections and replay

Each projection has a pure fold function and version. Rebuild creates a shadow projection, verifies invariants/checksums, then atomically swaps it into use. Startup compares projection checkpoints with journal heads and repairs or rebuilds as needed.

Replay modes are:

- **State replay:** deterministic event fold with no external effects.
- **Decision replay:** rerun verified planners/reducers against recorded inputs and compare emitted decisions.
- **Simulation replay:** replace effects/providers with recorded observations.
- **Live reproduction:** explicitly authorized new run using recorded configuration; never confused with deterministic replay.

#### Migration and rollback

Migrations are forward-only in place. Risky migrations create a verified backup and validate free disk space first. A previous binary is not run against a newer schema; operational rollback restores the pre-migration database copy and immutable artifact set. Every released schema fixture remains in `compat/` and is tested by all newer releases.

### Workspace and Git semantics

Each run receives a dedicated Git worktree anchored to an immutable baseline object ID. The agent sandbox sees only the worktree and explicitly allowed read-only roots. `.git` indirection, parent repository metadata, `.peritus`, harness security policy, evaluator assets, and secret storage are protected.

Workspace operations follow these rules:

- A verified lease transition grants one actor a generation-bound mutation token.
- Read-only snapshots can be mounted concurrently. A reviewer never shares a live writable directory with a writer/fixer.
- File targets are resolved relative to an opened workspace root. Platform implementations use handle-relative operations where possible and revalidate immediately before mutation.
- Path comparison accounts for separators, symlinks/junctions, case folding, Unicode normalization, alternate data streams/device names on Windows, and nested repositories/worktrees.
- Ordinary file mutation is expressed as a patch or explicit artifact replacement. Arbitrary shell writes still occur inside the OS sandbox but are detected by before/after Git and filesystem reconciliation before a candidate revision is accepted.
- A patch includes preimage digest, normalized target, mode intent, and expected workspace generation. Partial application is rejected unless the patch explicitly declares independent hunks and policy permits it.
- Candidate creation waits for owned foreground mutations to finish, reconciles untracked/ignored changes according to policy, hashes the resulting tree, and records a snapshot/event.
- Rollback creates a new worktree/tree revision from a known snapshot. It never deletes the abandoned candidate or its evidence.
- Merge into a user branch is a separately authorized operation after acceptance. Peritus never rewrites user branches or silently commits unrelated dirty state.

### Process and sandbox execution

Structured execution uses argv, cwd handle, environment references, stdin mode, timeout, resource class, network request, filesystem request, and output policy. Shell parsing is a distinct higher-risk action type rather than the default representation.

The process supervisor:

- owns every task and child process through RAII plus durable intent records;
- uses process groups/session IDs on Unix and job objects on Windows;
- streams ordered stdout/stderr chunks with monotonic offsets;
- spools output to artifacts while applying separate bounded UI windows;
- supports PTY and non-PTY modes, resize, stdin, signals, graceful cancellation, and forced termination;
- records spawn failure, exit code, signal/exception, timeout, cancellation, sandbox denial, and output completeness distinctly;
- reconciles process state after daemon restart and marks unverifiable orphan outcomes explicitly;
- enforces bounded process count, CPU, memory, file descriptors/handles, disk, output, and wall time.

Platform sandbox implementations compile the same abstract policy and pass the same conformance suite. Missing platform enforcement is reported as unsupported; it cannot silently fall back to unrestricted execution. Container isolation is an additional backend, not an excuse to skip host policy.

Network access is denied by default for agent actions. Authorized requests specify host patterns, port/protocol, DNS behavior, redirect policy, and duration. A managed proxy records normalized destinations and can inject narrowly scoped credentials without exposing them to the model or child environment.

### Capability and approval model

Capabilities are opaque domain values minted only by verified policy transitions. A capability contains actor, role, environment, resource selector, operations, revision/generation, issuance event, expiry, and optional use count. Policy intersection always chooses the most restrictive applicable rule; deny wins ties.

Risk classes distinguish read, scoped write, execution, network, dependency/environment management, repository history mutation, secret use, external side effect, policy waiver, and harness promotion. Prompts can explain policy but cannot modify it.

Approval requests contain the canonical action, resolved targets, risk analysis inputs, requested delta from current authority, and revision tuple. Human responses are actor-authenticated, expire, and bind to the request digest. “Approve similar” creates an explicit policy amendment with its own scope and audit event rather than a hidden string-prefix exception.

### Tool and plugin architecture

Tools register immutable descriptors containing name/version, JSON Schema, schema digest, capabilities, side-effect class, idempotency semantics, timeout/output limits, and implementation identity. The verified router computes the tool set exposed to each role and validates every call before dispatch.

Tool outputs use a common envelope with structured result, human rendering, model rendering, artifact references, error category, retryability, timing, and truncation metadata. A tool cannot hide failure behind successful prose.

Built-in filesystem, shell, Git, and quality tools call shared workspace/process primitives; they do not reimplement policy. MCP servers and plugins are untrusted external actors. They run out of process or in a Wasm component sandbox, declare capabilities, receive mediated paths/secrets, and cannot access daemon memory. Plugin protocol version and signature/trust state are visible to users. A plugin crash cannot crash the daemon or corrupt its journal.

### Model-provider and streaming architecture

The provider-neutral protocol represents role messages, provenance, content blocks, tool schemas/calls/results, reasoning summaries where available, output schemas, usage, rate-limit snapshots, cache metadata, and finish/error reasons. It does not expose provider SDK types publicly.

Each adapter publishes a capability profile: streaming, parallel tool calls, structured output, prompt caching, image/audio input, reasoning controls, context limits, resumable response IDs, and cancellation. The agent loop chooses behavior from the profile; it never assumes “compatible” means identical.

Provider requests have deterministic idempotency keys where supported. Retry policy distinguishes pre-send failure, ambiguous acceptance, stream interruption, rate limit, transient server error, invalid request, authentication, safety refusal, and malformed content. Repeated or out-of-order streaming items are normalized and deduplicated before verified turn reduction.

Credentials are secret references resolved only inside the provider worker. Request/response tracing is redacted and bounded. Cost/token accounting is observation data checked against reservations; providers cannot increase budgets by reporting refunds.

### Context and memory

Context is a typed graph rather than concatenated strings. Nodes carry provenance (`system`, `application`, `user`, `repository`, `external`, `memory`, `tool`, `agent`, `review`), authority, trust, content digest, recency, token estimate, and dependencies. The verified selector respects precedence, required inclusions, role visibility, and token budgets.

Compaction produces a derived node linked to every source range and a compaction policy/version. Required policy/spec content is never summarized away. A compaction result that omits mandated facts, violates provenance separation, or exceeds budget is rejected.

Memory records contain scope, source events, claim type, confidence, supporting/contradicting evidence, creation/review time, expiry, retrieval features, and quarantine state. Memory is derived context, never authority. It cannot grant capabilities, amend specifications, waive findings, or rewrite harness components. User deletion creates a tombstone and rebuilds affected indexes. Poisoning tests ensure external instructions stored in memory remain untrusted quoted material.

### Agent inner loop

`peritus-agent` implements a durable turn as a state machine:

```text
PreparingContext → RequestingModel → StreamingResponse
  → ProposedToolCalls → AwaitingAuthorization → ExecutingTools
  → RecordingResults → PreparingContext
  → ProposedCompletion → Completed
```

Every state can transition through retry, cancellation, pause, provider failure, malformed response, or recovery paths. The model never calls an effect implementation directly. Parallel tool calls require independent capabilities and bounded fan-out; result ordering is stable and explicit. Long-running commands yield control while remaining owned by the run.

A completion is a proposal containing summary, evidence references, unresolved uncertainties, and requested next phase. The orchestrator—not the model—decides whether to validate, review, fix, request authority, or fail.

### VDD acceptance contracts and quality gates

An acceptance contract includes:

- human-authored objective and user-visible behavior;
- immutable requirements with stable IDs;
- explicit exclusions and assumptions;
- repository roots and permitted change surface;
- required deterministic gates and dependency DAG;
- required review categories, quorum, independence, and severity thresholds;
- resource/budget ceilings and retry/cycle policy;
- security/approval policy references;
- evidence requirements and export classification;
- completion and failure conditions.

Gate definitions include command/action plan, environment, inputs, dependency gates, timeout/resources, parser, success predicate, required artifacts, and freshness scope. Auto-discovery may propose gates but cannot silently add/remove required gates from a frozen contract.

Gate execution occurs in a clean candidate snapshot. Results are tied to the full revision tuple and environment digest. Cached results are usable only when the verified freshness relation proves the new tuple does not affect them. Production defaults favor rerunning over broad cache assumptions.

### Writer, reviewer, and fixer orchestration

Role prompts are versioned harness components, but separation is enforced structurally:

- **Writer:** writable lease; may inspect, patch, execute, and request approvals; cannot review, waive, or accept.
- **Reviewer:** fresh context and read-only snapshot; may inspect source/diff/evidence and submit findings; cannot patch, run mutating tools, waive, or accept.
- **Fixer:** writable lease plus current findings; may patch and validate; cannot edit the specification, delete findings, waive, or accept.
- **Verifier/gate runner:** executes frozen gates; cannot edit the candidate or interpret policy into success.
- **Orchestrator:** verified transition authority; has no raw shell/filesystem capability.
- **Human authority:** resolves configured approvals/waivers/promotions through authenticated protocol actions.

Reviewer inputs exclude the writer's hidden reasoning and include the immutable spec, exact diff/tree, gate evidence, relevant source, prior findings/resolutions, and declared limitations. Contracts may require distinct model families/providers for review quorum. Independence attestations record provider, model family, prompt revision, context digest, and shared ancestry; the policy decides whether two reviews are sufficiently independent.

A finding schema contains ID, review/cycle ID, candidate revision, category, severity, blocking status, confidence, requirement IDs, locations, evidence artifacts/events, description, reproduction, expected behavior, suggested remediation, and reviewer identity. Free-form output is parsed into this schema and rejected/retried if required fields are absent.

The fixer must disposition every current blocker as fixed, disputed with evidence, superseded, or waiver-requested. A reviewer confirms resolution against the new revision. Duplicate findings are reconciled without losing provenance. Oscillation, flat severity, max cycles, or budget exhaustion terminates as `NeedsHuman`/`Failed`; none is an acceptance shortcut.

Default production acceptance requires deterministic gates passing, zero unresolved blockers, all required categories reviewed, reviewer quorum met, all evidence current, and human approval where configured. The exact policy is contract data evaluated by verified code.

### Observability and trace architecture

Peritus emits one normalized causal record for every meaningful lifecycle boundary. OpenTelemetry export is a projection; the local event/evidence model remains usable offline and is not coupled to a vendor backend.

Required correlation fields include project, session, run, attempt, turn, action/tool call, process, gate, review cycle, finding, actor, environment, workspace revision, harness revision, spec revision, provider profile, and root/parent causality. Metrics and spans carry stable low-cardinality dimensions; high-cardinality content stays in artifact-linked events.

Trace storage has four layers:

1. **Raw vault:** optional encrypted provider bytes, terminal streams, and tool payloads under strict access/retention policy.
2. **Normalized event stream:** typed redacted items with stable schemas and source byte/artifact references.
3. **Attempt analysis:** evidence-linked timelines, failure classifications, resource use, state transitions, and anomalous behavior.
4. **Cross-run analysis:** clustered failure patterns, success patterns, regressions, component correlations, and campaign summaries.

Derived reports must cite event IDs and artifact ranges. A report parser validates citations exist and belong to the claimed run/revision. Unsupported inference is labeled; generated diagnosis never overwrites raw evidence.

The failure taxonomy is extensible but starts with:

- specification ambiguity/conflict/unachievable requirement;
- context selection/compaction/provenance failure;
- model reasoning, malformed output, refusal, or completion error;
- provider authentication, quota, rate limit, transport, protocol, or accounting error;
- tool schema, routing, authorization, execution, or result-normalization error;
- workspace/patch/Git/path conflict;
- sandbox/process/network/resource failure;
- deterministic gate failure versus gate infrastructure failure;
- review disagreement, invalid finding, unresolved blocker, or oscillation;
- journal, artifact, projection, migration, or recovery failure;
- approval/authority timeout or denial;
- scheduler starvation/cancellation/dependency failure;
- evolution contamination, attribution uncertainty, statistical rejection, or promotion denial.

Redaction runs before default trace persistence and again before export. It combines registered secret fingerprints, structured field classification, environment-key policy, entropy/token detectors, and user-configured patterns. Redaction events record that material was removed without retaining the secret. Seeded-canary tests ensure secrets do not appear in logs, model-visible errors, TUI rendering, crash reports, telemetry, or evidence exports.

### Agent debugger and failure analysis

`peritus-debugger` reproduces AHE's layered drill-down behavior as typed jobs:

1. Select traces by frozen query and record the selection manifest.
2. Normalize and classify infrastructure versus task outcomes.
3. Produce per-attempt timelines and candidate root causes.
4. Cluster recurring failure/success patterns across tasks and revisions.
5. Link every claim to source evidence and estimate confidence/alternative causes.
6. Map each pattern to likely harness component classes and constraint strength.
7. Submit the report as evidence; never mutate the harness directly.

The debugger may use models, deterministic analyzers, or both. Model-generated diagnoses are untrusted proposals until schema/citation validation succeeds. Reports retain contrary evidence and distinguish observation, inference, and recommendation.

### Harness component model

The committed `.peritus-harness/manifest.toml` defines an acyclic content graph. Component classes include:

- base/system instruction fragments;
- role definitions and role-specific prompts;
- tool descriptors, schemas, implementations, and exposure policies;
- middleware/context transforms;
- skills and reference bundles;
- sub-agent/collaboration definitions;
- memory schemas, selectors, ranking, retention, and injection policy;
- gate definitions and parsers;
- orchestration and termination policy;
- provider capability/profile settings;
- observability/redaction/analysis policy;
- evolution strategy and metric definitions.

Security root policy, human authority definitions, sealed evaluator content, trust-boundary specifications, and production promotion rules are controlled assets but are not evolvable by an ordinary campaign.

Each component has kind, stable ID, schema version, content digest, dependencies, compatibility range, owner, provenance, and optional executable artifact digest. Materialization validates dependency cycles, missing references, duplicate IDs, incompatible versions, undeclared files, and forbidden component-to-component authority.

### Evaluation and harness evolution

Evolution is an offline, separately authorized activity. It operates on immutable run evidence and isolated candidate revisions.

#### Evaluation corpus

Datasets are divided into declared development/calibration, regression, sealed holdout, and optional canary partitions. Sealed task contents, hidden tests, and expected outputs are unavailable to candidate agents, debuggers that produce edits, and evolvers. Evaluator code and dataset manifests are read-only and digest-pinned.

An evaluation profile freezes:

- task/dataset revision and partition visibility;
- baseline and candidate harness digests;
- provider/model, reasoning controls, sampling, context and output limits;
- sandbox image/environment, resource limits, timeout, and concurrency;
- rollout count/seeds and retry rules;
- verifier/evaluator digests;
- metric definitions and infrastructure-failure treatment;
- cost, latency, safety, and reliability constraints.

Infrastructure failures are counted and reported separately and according to the frozen metric policy; they are never silently dropped to improve a score.

#### Change manifest

Every proposed change has a stable ID and records:

- source failure/success pattern IDs and cited evidence;
- root-cause hypothesis and alternatives;
- target component and why that constraint level is correct;
- exact files/components and semantic diff;
- predicted fixed tasks/classes;
- predicted regression tasks/classes;
- expected cost, latency, token, reliability, and security effects;
- falsification criteria;
- compatibility/migration impact;
- rollback plan.

Changes are committed separately when independently attributable. Interacting changes declare a group so evaluation does not falsely claim independent causality.

#### Variant execution and attribution

Best-of-N or factorial variants use separate Git branches/worktrees and fixed profiles. Scheduling randomizes or blocks execution to reduce temporal/provider bias. Results retain all rollouts, including exceptions/timeouts. Attribution compares task-level transitions, stability history, resource deltas, and confidence intervals. Because AHE evidence shows regression prediction is weaker than fix prediction, Peritus never treats the evolver's self-prediction as promotion evidence by itself.

Selection is multi-objective. Primary correctness may use paired pass/fail outcomes with bootstrap or exact paired confidence intervals as appropriate. Secondary constraints include critical-regression count, safety policy, deterministic gate integrity, latency distribution, token/cost distribution, infrastructure reliability, review burden, and trace completeness. Statistical methods and thresholds are versioned campaign inputs.

#### Promotion

Promotion is a verified state transition requiring:

- immutable candidate, baseline, profile, dataset, evaluator, and results;
- satisfied correctness and non-regression predicates;
- zero prohibited safety/evaluator/policy changes;
- component/schema compatibility;
- complete change attribution and evidence bundle;
- independent review of executable/security-sensitive components;
- configured human or organizational authority.

Promotion atomically changes the project production-harness pointer to an immutable digest and records the previous digest for rollback. Existing runs retain their original harness. Rollback is a new promotion event to a known compatible revision, not history rewriting.

### Configuration and policy layering

Configuration layers, from highest authority to lowest, are:

1. compiled security invariants and release profile;
2. system/organization policy;
3. authenticated user policy;
4. committed project configuration;
5. immutable run acceptance contract;
6. session overrides permitted by higher layers;
7. role/harness preferences;
8. model proposals, which have no configuration authority.

Each effective value retains source-layer provenance. Protected security values combine monotonically: a lower layer may tighten but not loosen. Non-security preferences follow explicit precedence. `peritus config explain <key>` renders the final value, every contributing layer, and rejected overrides.

`peritus.toml` and harness manifests have schema versions and strict unknown-field handling for security-sensitive sections. Secret values are references, never plaintext. Environment variables are accepted only through declared mappings and do not silently override project/security policy.

### Local application protocol and daemon lifecycle

The daemon listens on a user-scoped Unix domain socket or Windows named pipe with peer-identity checks and filesystem/ACL protection. Remote TCP is disabled in the initial production architecture unless a separately authenticated transport profile is explicitly configured.

The app protocol supports:

- request/response commands with idempotency and expected revision;
- resumable event subscriptions with cursor and at-least-once delivery;
- binary/artifact transfer by bounded chunk and digest;
- approval and user-input requests;
- terminal attach/input/resize;
- capability and version negotiation;
- graceful incompatibility errors.

Startup obtains an exclusive daemon lock, verifies/migrates storage, scans journal integrity, rebuilds stale projections, reconciles leases/processes/worktrees, resumes outbox delivery, and only then accepts mutation commands. Read-only diagnostic mode remains available when mutation startup fails.

Shutdown stops intake, checkpoints queues, requests cancellation or persistence according to run policy, flushes authoritative state, and reports work that remains externally active. The daemon never claims clean shutdown while owned effects are unaccounted for.

### Security considerations

#### Trust boundaries and adversaries

Peritus assumes the following can be malicious, compromised, malformed, or simply wrong:

- repository content, including `AGENTS.md`, build scripts, dependencies, Git metadata, filenames, and tests;
- user-provided external content and fetched web material;
- model output and model-generated tool arguments;
- provider responses and network intermediaries outside TLS guarantees;
- MCP servers, plugins, language servers, and project executables;
- terminal output containing control sequences or spoofed prompts;
- memories and prior run artifacts;
- evolution candidates attempting evaluator/policy manipulation;
- concurrent local clients attempting stale or replayed commands.

The host OS, Peritus release binary, pinned trust specifications, configured human authority, and correctly protected credential store form the base TCB. Platform sandbox limitations are documented and tested, not papered over by prompts.

#### Required controls

- Parse and render untrusted content as data with provenance; never concatenate it into higher-authority instruction sections without explicit quoting/typing.
- Canonicalize command/action displays from the same structured plan that policy authorizes; do not authorize one string and execute another.
- Recheck path and capability immediately before effect; use generation/preimage checks to defeat time-of-check/time-of-use races.
- Protect environment variables, credential files, IPC sockets, Git metadata, Peritus state, harness trust policy, and evaluator assets from agent reads/writes unless explicitly required.
- Use least-privilege child environments and strip inherited credentials by default.
- Enforce network destination policy after DNS resolution and on redirects/connect tunnels; defend against loopback, link-local, metadata-service, DNS rebinding, and alternate IP representations.
- Escape terminal output and disable dangerous control sequences in CLI/TUI renderers.
- Sign trusted plugins/releases, verify artifact digests, produce SBOM/provenance, and audit licenses.
- Rate-limit approvals and bind them to canonical action digests to prevent UI confusion/replay.
- Keep review independence metadata visible; do not represent same-model self-review as equivalent to independent review.
- Treat evolution as hostile optimization: immutable evaluators, sealed holdouts, capability denial, multi-objective gates, and human promotion authority.
- Encrypt sensitive raw traces and support verifiable deletion/retention workflows.

#### Unsafe and FFI policy

Unsafe code is permitted only where the OS/terminal/sandbox/crypto dependency boundary requires it. Every unsafe module contains a `SAFETY.md` or module-level safety contract covering ownership, lifetimes, pointer validity, alignment, thread behavior, signal/handle rules, and teardown. Unsafe blocks are minimal, locally commented, inventoried by `xtask`, reviewed by a second security-capable engineer, and exercised under Miri/sanitizers where applicable.

### Failure handling

Failures are typed, persisted, and recoverable where recovery is honest:

| Failure | Authoritative behavior | Recovery/evidence |
|---|---|---|
| Provider rejects or times out | attempt remains non-success; retry only if policy and idempotency allow | record request digest, provider category, usage ambiguity, retry decision |
| Stream disconnects after partial tool call | incomplete item is never executable | persist chunks and normalization error; retry/recover with provider-specific semantics |
| Tool schema/result malformed | reject observation; do not mutate domain success | retain raw/redacted artifact and validation details |
| Approval denied/expires | action becomes denied/expired; agent receives bounded reason | new proposal requires new action/request digest |
| Sandbox policy unavailable | action fails closed | platform probe and unsupported-control report |
| Process timeout/cancel | terminate owned process tree; no inferred success | terminal result plus output completeness and kill escalation |
| Daemon crashes during effect | recovery reconciles outstanding plan with OS/workspace observation | mark completed, failed, or indeterminate; indeterminate cannot support acceptance |
| Patch conflict/preimage mismatch | no authoritative candidate revision | conflict evidence; writer/fixer must rebase/replan |
| Disk full during blob write | temporary blob remains unreferenced; event not committed | quota/disk diagnostic and cleanup route |
| Disk/database failure during append | transaction rolls back; command stays uncommitted/retryable by idempotency key | integrity check before retry |
| Projection corrupt | daemon rebuilds from journal | projection audit report |
| Journal hash/sequence mismatch | mutation mode stops | read-only diagnostics, backup/forensic recovery; never auto-ignore |
| Lease owner disappears | lease expires/reconciles through verified transition | workspace scan and new generation; old token unusable |
| Gate infrastructure fails | classify separately from gate assertion failure | retry per contract or end non-success |
| Reviewer output invalid | retry with schema error within budget, then fail/require human | raw output and parser evidence |
| Review oscillates/stagnates | terminate `NeedsHuman`/`Failed` | trend evidence; never accept by exhaustion |
| Spec changes | successor run/revision; all prior acceptance evidence stale | explicit lineage and change diff |
| Evolution contaminates evaluator | candidate/campaign rejected and quarantined | security finding and immutable audit trail |
| Migration fails | original store remains/restores intact | migration log, backup validation, read-only diagnostics |

No library panics for recoverable input/environment errors. Invariant violations in authoritative code trigger fail-stop diagnostics and preserve evidence; they are never caught and converted into success.

### Performance and resource model

The scheduler uses bounded queues and explicit reservations for provider concurrency, process slots, CPU/memory/disk classes, and token/cost budgets. Priority does not bypass policy or starvation bounds. Child tasks inherit a subset of the parent's remaining budget; they cannot mint resources.

Streaming paths use bounded channels and backpressure to disk/provider/client. Slow UI or telemetry exporters cannot block authoritative event append indefinitely. Large artifacts stream without whole-buffer copies. Digests are incremental. Database write transactions remain short and never span model/tool I/O.

Initial production SLOs are established by benchmark evidence before release, not guessed into the API. The release must publish at least:

- daemon startup and recovery time by journal size;
- p50/p95/p99 command-to-first-event and event-append latency;
- terminal throughput and cancellation latency;
- maximum steady-state memory per active run and per streamed process;
- projection rebuild throughput;
- supported concurrent run/process/provider counts per reference machine;
- artifact quota behavior and garbage-collection pause bounds;
- Verus full-workspace verification duration and proof hot spots.

Performance optimization cannot weaken durability, authorization, evidence freshness, or proof obligations without a reviewed design amendment.

## Data and compatibility

### Versioned surfaces

The following are independently versioned:

- CLI behavior and exit-code contract;
- local app protocol;
- domain command/event schemas;
- SQLite schema and projection versions;
- acceptance spec and gate schema;
- harness manifest and component schemas;
- tool/MCP/plugin protocols;
- provider capability profiles;
- trace/evidence bundle schema;
- trust/exclusion manifests.

The first public production release is `1.0.0`; internal implementation checkpoints are not published as supported releases. From 1.0 onward, public libraries and protocols follow semantic compatibility rules. Events remain readable indefinitely or have a deterministic, tested migration path. Unknown event payloads are preserved during export/migration even when an older component cannot interpret them.

### Canonical fixtures and generated assets

`compat/` contains one minimal and one realistic fixture for every released schema version plus corrupt/adversarial cases. `schemas/` and generated client bindings are derived from canonical Rust definitions by `xtask generate`; CI regenerates and rejects drift. Generated files have explicit owners and are never hand-edited.

### Licensing and source reuse

Project-Peritus is MIT licensed. NexAU-AHE and LemonHarness are MIT references; Codex is Apache-2.0. Architectural ideas can be reimplemented cleanly. Any copied or adapted source must retain required notices, provenance, and compatible licensing. `cargo deny`/license inventory and an attribution file are release gates. The design does not assume that all code visible in the references can be copied without file-level provenance review.

## Verification

### Verification pyramid

1. **Formal verification:** Verus specifications, inductive invariants, refinement lemmas, tokenized ownership, arithmetic/budget proofs, and full clean release verification.
2. **Compile-time constraints:** private constructors, typestate, exhaustive enums, trait bounds, Send/Sync review, denied warnings, and API visibility.
3. **Unit and property tests:** public behavior, boundary values, transition model comparison, serialization round trips, path/policy generators, and regression cases.
4. **Concurrency tests:** Loom models for queues/leases/shutdown where supported, deterministic scheduler tests, cancellation races, and deadlock checks.
5. **Adapter refinement/conformance:** every H/T boundary is tested against the contract assumed by verified code, including injected lies and partial failures.
6. **Integration tests:** journal/artifact/workspace/process/provider/tool/gate/review interactions in temporary repositories and sandboxes.
7. **End-to-end tests:** real daemon clients and representative language repositories on Linux, macOS, and Windows.
8. **Adversarial security tests:** hostile repository, prompt injection, sandbox escape, path race, secret canary, approval spoof, plugin/MCP, and evolution gaming.
9. **Resilience and chaos:** deterministic failpoints, kill/restart, power-loss simulation, disk/quota faults, provider faults, worker death, and corrupted projections.
10. **Evaluation campaigns:** reproducible task suites measuring correctness, regression, cost, latency, reliability, review effectiveness, and trace completeness.
11. **Performance/soak:** bounded-resource load, long-running sessions, output-heavy commands, many artifacts, and repeated recovery.

### Required developer checks

The root `justfile`/`xtask` provides stable commands, ultimately including:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
cargo test --doc --workspace --all-features
cargo verus verify --workspace
cargo verus build --workspace --release
cargo miri test <eligible crates>
cargo deny check
cargo vet
cargo audit
cargo llvm-cov nextest --workspace
cargo fuzz ...
cargo xtask architecture-check
cargo xtask verify-trust
cargo xtask verify-generated
cargo xtask compatibility
cargo xtask conformance --all
```

Exact tools may evolve before interface freeze, but equivalent evidence is mandatory. Focused checks accelerate local work; protected branches run the full matrix.

### Test design rules

- Tests assert contracts and externally observable state, not private implementation trivia.
- Time, randomness, IDs, provider streams, filesystems, and worker scheduling are injectable/deterministic in tests.
- No network-dependent unit tests. Network integration tests use controlled servers and explicit labels.
- Every bug fix begins with a failing regression test at the narrowest meaningful boundary.
- Every proof invariant has at least one negative executable/property test that would fail if the guard disappeared.
- Snapshot/golden tests normalize nondeterministic values and require reviewed diffs.
- Ignored/flaky tests are forbidden on release branches. Quarantine is visible non-success, time-limited, and excludes release.
- Coverage is diagnostic, not a substitute for proof or meaningful assertions. Critical state/policy code requires branch and mutation evidence.

## Rollout and rollback

“Rollout” here means internal implementation and integration order, not releasing an incomplete product.

- Work lands behind internal compile-time or configuration gates only when needed to keep the main branch integrated; gates must fail closed and may not advertise unsupported behavior.
- Internal dogfooding uses disposable test repositories and explicitly non-production state. It does not create compatibility promises.
- No public alpha/beta package, installer, release tag, or production documentation is produced before the complete production acceptance checklist passes.
- Database and protocol fixtures begin immediately so internal changes remain deliberate rather than chaotic.
- The first public artifact is a production-reviewed 1.0 release candidate built from the release process; it becomes 1.0 only after security, compatibility, platform, proof, soak, and recovery evidence is signed off.
- Operational rollback restores a signed previous release plus its compatible pre-migration database backup or performs a tested forward repair. Harness rollback is an independent immutable pointer transition and does not require binary rollback.

## Parallel implementation plan

### Parallel-work rules

The slices below are designed for isolated worktrees and explicit crate ownership.

1. One slice owns each crate/path while active. Cross-slice edits require coordination recorded on the owning issue.
2. Shared protocols/specifications land before their consumers. Consumers use conformance fixtures and fake implementations rather than editing the protocol ad hoc.
3. Root `Cargo.toml`, toolchain, lint, CI, `xtask`, shared dependency versions, schemas, and architecture policy have a foundation owner and merge queue.
4. A change to `peritus-types`, `peritus-spec`, `peritus-kernel`, `peritus-policy`, public protocols, or TCB requires an architecture note, compatibility/proof impact, and downstream workspace verification.
5. Generated assets are changed only through their canonical generator in the same commit.
6. Each slice supplies compile-clean library code, docs, tests, failure behavior, and completion evidence; placeholder success paths and `todo!()` in reachable production code are prohibited.
7. Integration slices depend on released internal contracts, not unmerged branches. Temporary adapters live in the consumer slice and are deleted at integration.
8. Stage barriers freeze interfaces needed by the next wave; they do not declare product completeness.

### Dependency graph

```text
A0 repository/toolchain
 ├─ A1 formal foundations ─┬─ B0 kernel/state ─┬─ D0 agent loop ─┐
 │                         ├─ B1 policy/budget ┤                 │
 │                         ├─ B2 spec/gates ───┤                 │
 │                         └─ B3 protocols ────┼─ C0 persistence│
 ├─ A2 test/conformance ───────────────────────┼─ C1 workspace  │
 └─ A3 app protocol ───────────────────────────┼─ C2 execution  │
                                               ├─ C3 providers  │
                                               ├─ C4 tools      │
                                               ├─ C5 context    │
                                               └─ C6 trace      │
                                                                ▼
                           E0 gates + E1 review + E2 scheduler → E3 orchestrator
                                                                │
                          F0 debugger + F1 eval + F2 harness ────┤
                                                                ▼
                                                          F3 evolution
                                                                │
                             G0 daemon → G1 CLI / G2 TUI / G3 plugins
                                                                │
                                  H0 security + H1 resilience + H2 platform
                                  + H3 performance + H4 release qualification
```

### Slice catalog

| Slice | Owns | Depends on | Deliverable and completion evidence |
|---|---|---|---|
| **A0 Workspace/toolchain** | root manifests, pinned Rust/Verus toolchains, lint profiles, `justfile`, base CI | none | Empty workspace builds in ordinary Rust and Verus; strict lint/doc/license/reproducibility skeleton passes. |
| **A1 Formal foundation** | `peritus-types`, `peritus-tcb`, verification manifests | A0 | Validated opaque types, trust rules, first proofs, runtime wrappers, and trust-cheat CI. |
| **A2 Test/conformance foundation** | `peritus-test-support`, `peritus-conformance`, fixture conventions | A0/A1 | Deterministic clock/ID/fault/provider/tool fixtures and runnable empty conformance suites. |
| **A3 Application protocol foundation** | `peritus-app-protocol`, schema generation | A0/A1 | Version negotiation, request/event envelopes, cursor/idempotency types, compatibility fixtures. |
| **B0 Lifecycle kernel** | `peritus-kernel` | A1 | Session/run/attempt/turn/action state machines and proofs for legal transitions, causality, and no implicit success. |
| **B1 Policy, leases, and budgets** | `peritus-policy`, `peritus-budget`, verified part of `peritus-leases` | A1 | Capability/authority/lease/resource models with `INV-006`–`INV-009`, `INV-012`, and policy property tests. |
| **B2 Acceptance specification** | `peritus-spec`, verified quality-policy definitions | A1 | Contract/gate/review schema, validation, revision/freshness model, and adversarial contract tests. |
| **B3 Domain protocol and codec** | `peritus-protocol`, `peritus-codec` | A1, B0–B2 contracts | Versioned commands/events/errors, bounded decoding, schema/codegen, compatibility corpus. |
| **C0 Journal/projections/artifacts** | `peritus-journal`, `peritus-projection`, `peritus-artifact-store`, `peritus-migrations`, `peritus-evidence` | A2, B3 | Transactional event store, replay/rebuild, blob finalization, migrations, failpoint crash evidence. |
| **C1 Git/workspace/patch** | `peritus-git`, `peritus-patch`, `peritus-workspace` | B1–B3, A2 | Worktree lifecycle, lease enforcement, hardened paths, atomic patches, snapshots/rollback, malicious-path suite. |
| **C2 Process/sandbox backplane** | `peritus-process`, `peritus-sandbox` | B1/B3, A2 | Owned process/PTY lifecycle and abstract sandbox plan with fake backend and cancellation/resource tests. |
| **C3 Platform security backends** | Linux/macOS/Windows sandbox crates, `peritus-network`, `peritus-secrets` | C2, B1 | Native enforcement, probes, common conformance, secret canaries, and fail-closed unsupported behavior. Work divides further by OS without shared-path edits. |
| **C4 Tool system** | tool protocol/router and built-in tool crates | B1/B3, C1/C2 | Verified exposure/routing, common envelopes, fs/shell/Git/quality tools, schema and authorization tests. |
| **C5 Model providers** | model protocol, provider core and provider adapters | B3, A2 | Capability negotiation, normalized streaming, retries/idempotency, fake server and provider conformance. Provider adapters are independent sub-slices. |
| **C6 Context and memory** | `peritus-context`, `peritus-memory`, `peritus-role` | A1, B1/B2/B3 | Provenance graph, selection/compaction plans, memory lifecycle/retrieval, poisoning tests, role capability definitions. |
| **C7 Trace and telemetry** | `peritus-trace`, `peritus-telemetry` | B3, C0 | Causal normalized traces, redaction/raw vault references, OTel projection, backpressure and secret-leak tests. |
| **D0 Agent loop** | `peritus-agent` | B0/B1/B3, C4–C6 | Durable model/tool loop with pause/cancel/recovery, structured completion proposals, fake-provider/tool E2E tests. |
| **D1 Gate engine** | `peritus-gates`, `peritus-tools-quality` integration | B2/B3, C1/C2/C4 | Gate DAG planner/executor/parser/freshness, clean-snapshot runs, infrastructure classification, proof/property tests. |
| **D2 Review engine** | `peritus-review` | B0–B3, C6 | Finding lifecycle, quorum/independence, reconciliation, resolution/waiver, invalidation, oscillation and malformed-review tests. |
| **D3 Scheduler/collaboration** | `peritus-scheduler`, `peritus-collaboration` | B0/B1/B3, A2 | Bounded fair queues, reservations, cancellation trees, causal messages, bounded fan-out, concurrency models. |
| **E0 Delivery orchestrator** | `peritus-orchestrator` | C0/C1, D0–D3 | Complete writer→gate→review→fix loop; only acceptance path; scenario matrix for every terminal state and crash phase. |
| **E1 Harness materialization** | `peritus-harness` | B2/B3, C0/C1 | Component graph/schema/compatibility/materialization, protected classes, content-addressed revisions. |
| **E2 Debugger** | `peritus-debugger` | C7, C5/C6, A2 | Evidence selection, per-attempt/cross-run analysis jobs, citation validation, failure taxonomy reports. |
| **E3 Evaluation** | `peritus-eval` | C0/C2/C5, D3, E1 | Frozen profiles, dataset isolation, rollouts, pass@k/statistics/stability, infrastructure accounting, reproducibility tests. |
| **F0 Evolution** | `peritus-evolution` | E1–E3, B0/B1/B3 | Campaign state machine, manifests, variants, attribution, selection, promotion/rollback proofs and gaming tests. |
| **G0 Daemon** | `peritus-daemon` | A3, C0, C2, E0, F0 | Composition, IPC, supervision, startup reconciliation, durable queues/outbox, shutdown/recovery kill tests. |
| **G1 CLI** | `peritus-cli` | A3/G0 | Complete scriptable command surface, JSON, stable exits, completions, black-box tests. |
| **G2 TUI** | `peritus-tui` | A3/G0 | Live runs, terminal, approvals, diffs, traces, reviews, evolution; snapshot and interaction tests. |
| **G3 Extension integration** | `peritus-mcp`, plugin SDK/host | B1/B3, C2/C4, G0 | Out-of-process/Wasm plugins and MCP with trust/capability/quotas, malicious extension suite. |
| **H0 Security qualification** | cross-cutting tests/docs only; no silent redesign | all functional slices | Threat model closure, unsafe/TCB audit, sandbox/path/network/plugin/evolution red team, external review findings resolved. |
| **H1 Resilience qualification** | failpoint/chaos/recovery suites | C0 onward | Crash matrix, corruption, disk full, provider/tool/worker death, reboot/reconcile evidence. |
| **H2 Platform qualification** | cross-platform conformance/packaging | G0–G3 | Linux/macOS/Windows full matrix, installers, path/sandbox/process equivalence and documented platform deltas. |
| **H3 Performance qualification** | benchmarks/load/soak and tuning | G0/F0 | Published SLO evidence, bounded resources/backpressure, long-horizon soak, no correctness weakening. |
| **H4 Release qualification** | docs, migration, SBOM, signatures, reproducibility, final audit | H0–H3 | Every acceptance criterion mapped to signed evidence; production 1.0 artifact only after final `ready` verdict. |

### Crate-to-slice ownership registry

This registry is canonical for parallel planning. A slice may split internally only along the listed crate boundaries; changing ownership requires a plan amendment.

| Slice | Exact crate ownership |
|---|---|
| A0 | `xtask` plus root workspace/toolchain/CI files |
| A1 | `peritus-types`, `peritus-tcb` |
| A2 | `peritus-test-support`, `peritus-conformance` |
| A3 | `peritus-app-protocol` |
| B0 | `peritus-kernel` |
| B1 | `peritus-policy`, `peritus-budget`, `peritus-leases`, `peritus-approval` |
| B2 | `peritus-spec`, `peritus-quality-policy` |
| B3 | `peritus-protocol`, `peritus-codec` |
| C0 | `peritus-journal`, `peritus-projection`, `peritus-artifact-store`, `peritus-migrations`, `peritus-evidence` |
| C1 | `peritus-git`, `peritus-patch`, `peritus-workspace` |
| C2 | `peritus-process`, `peritus-sandbox` |
| C3 | `peritus-sandbox-linux`, `peritus-sandbox-macos`, `peritus-sandbox-windows`, `peritus-network`, `peritus-secrets` |
| C4 | `peritus-tool-protocol`, `peritus-tool-router`, `peritus-tools-fs`, `peritus-tools-shell`, `peritus-tools-git`, `peritus-tools-quality` |
| C5 | `peritus-model-protocol`, `peritus-provider-core`, `peritus-provider-openai`, `peritus-provider-anthropic`, `peritus-provider-google`, `peritus-provider-compatible` |
| C6 | `peritus-context`, `peritus-memory`, `peritus-role` |
| C7 | `peritus-trace`, `peritus-telemetry` |
| D0 | `peritus-agent` |
| D1 | `peritus-gates` |
| D2 | `peritus-review` |
| D3 | `peritus-scheduler`, `peritus-collaboration` |
| E0 | `peritus-orchestrator` |
| E1 | `peritus-harness` |
| E2 | `peritus-debugger` |
| E3 | `peritus-eval` |
| F0 | `peritus-evolution` |
| G0 | `peritus-daemon` |
| G1 | `peritus-cli` |
| G2 | `peritus-tui` |
| G3 | `peritus-mcp`, `peritus-plugin-sdk`, `peritus-plugin-host` |
| H3 | `peritus-benchmarks` |

Slices in the same dependency wave may run in parallel. C3 splits by platform; C5 splits by provider; G1/G2 split by client. B0/B1/B2 proceed in parallel only after A1 agrees on shared primitive types. D0/D1/D2/D3 proceed in parallel against frozen B/C contracts. H0–H3 begin test design early but issue their qualification verdicts only against the integrated release candidate.

### Merge gates between waves

- **Gate A:** toolchains reproducible; trust policy active; primitive/API naming and fixture rules approved.
- **Gate B:** verified domain/spec/policy/protocol invariants pass; protocol v1 candidate frozen for effect teams.
- **Gate C:** persistence, workspace, process, sandbox abstraction, providers, tools, context, and trace pass their conformance suites independently.
- **Gate D:** agent, gates, review, and scheduler pass fake-boundary integration and all illegal-transition tests.
- **Gate E:** the complete delivery loop passes crash/replay/revision-freshness/adversarial role scenarios.
- **Gate F:** debugger/evaluation/evolution pass contamination, statistical, attribution, and promotion safety suites.
- **Gate G:** daemon/clients/extensions pass lifecycle, compatibility, and end-to-end scenarios on all platforms.
- **Gate H:** security, resilience, performance, documentation, reproducibility, and final independent review are all `ready`.

## Open questions

There are no blocking product questions required to draft or begin the foundation slices. The following defaults are intentionally explicit and may be changed only through a reviewed design amendment before their relevant interface freeze:

1. Product and binary name: `Peritus` / `peritus`.
2. License: repository MIT license, with file-level notices for adapted third-party code.
3. Deployment: local-first daemon with no hosted service dependency.
4. Tier-one hosts: current supported Linux distributions, macOS, and Windows; exact minimum versions are pinned during C3/H2 based on enforceable sandbox APIs.
5. Storage: SQLite plus content-addressed filesystem artifacts.
6. Extension safety: out-of-process or Wasm components; no arbitrary in-process native plugins.
7. Production review default: deterministic gates plus at least two sufficiently independent blocker-free reviews for high-risk changes; contracts may require stricter policy, while lowering below the production floor requires authorized policy change.
8. Promotion authority: human approval required for production harness promotion by default.
9. Verification policy: every supported deterministic path is verified; exclusions are exceptional, enumerated, expiring, and release-visible.

## Out of scope

These exclusions do not remove any capability requested for the coding harness:

- Training or serving a foundation model is not implemented by Peritus; model services integrate through provider adapters.
- Implementing a new operating-system kernel, container runtime, Git implementation, SQL engine, TLS stack, SMT solver, or Verus compiler is not part of Peritus. Their narrow adapters remain explicit TCB/effect boundaries and are independently tested.
- A multi-tenant hosted SaaS control plane is not required for the local production product. The versioned app protocol intentionally permits a separately designed remote service later without coupling it to the core.
- General-purpose CI hosting and arbitrary fleet orchestration are integrations, not substitutes for Peritus's own run scheduler and evidence model.

No requested observability, workspace/state semantics, Codex-style loop, writer/reviewer/fixer orchestration, harness evolution, cross-platform sandboxing, formal verification, recovery, security, or production hardening is deferred outside the production design.

## Alternatives considered

### Fork Codex CLI and graft evolution/review onto it

This offers the shortest path to an impressive demo but couples Peritus to a large upstream internal protocol and recurring merges. It also makes it harder to establish verified authority boundaries retroactively. Rejected in favor of a clean domain/control plane with independently implemented adapters and borrowed architectural lessons.

### One Rust crate or a small handful of crates

This minimizes initial Cargo configuration but encourages protocol, state, I/O, provider, UI, and policy coupling; creates broad rebuild/review ownership; and makes parallel work collide. Rejected. Peritus uses many cohesive crates with enforced dependency layers, while avoiding meaningless one-type utility crates.

### Distributed microservices from the start

This creates network consistency, deployment, authentication, and operational complexity before it buys user value. Rejected for the local product. The daemon has internal message boundaries and versioned protocols so specific workers can move out of process without changing the domain model.

### Ordinary Rust with a small verified kernel

This would be easier but violates the project's formal-verification ambition. Rejected. The selected architecture pushes all supported deterministic logic into Verus and treats every exclusion as a tracked defect in verification coverage rather than a permanent comfort boundary.

### Mark broad async/I/O modules trusted to claim high Verus coverage

This produces a nominally Verus-heavy codebase while moving correctness into unchecked assumptions. Rejected as false assurance. Peritus maximizes meaningful verified executable logic and minimizes the actual TCB.

### Raw JSONL as the only store

Append-only text is transparent but weak for atomic multi-record transitions, indexes, leases, migration, and crash reconciliation. Rejected as the sole authority. Portable JSONL/evidence export remains supported over a transactional SQLite journal.

### In-process native plugin ABI

This offers low overhead but expands the daemon's memory-safety and authority TCB and creates Rust ABI/version problems. Rejected. Plugins use process or Wasm isolation and a stable protocol.

## Source basis

The design was grounded in the local reference repositories and current official Verus material:

- `reference-repos/NexAU-AHE/README.md` and `agentic_harness_engineering.pdf`: component/experience/decision observability, trace-first analysis, change manifests, falsification, attribution, variants, and rollback.
- `reference-repos/NexAU-AHE/evolve.py` and `agents/evolve_agent/evolve_prompt.md`: concrete evaluation, task stability, debugger, history, manifest, Git, best-of-N, and promotion inputs.
- `reference-repos/LemonHarness/README.md`, `lemonharness-guidance.md`, and `.pi/extensions/lemonharness/`: workspace boundaries, snapshots, execution records, time/context budgets, memory, privileges, quality gates, and review loops.
- `reference-repos/codex/codex-rs/`: Cargo decomposition, protocol commands/events, session/turn state, rollout persistence, app protocol, terminal execution, patching, approvals, permissions, and platform sandbox boundaries.
- Verus official project and tutorial/reference: multi-crate Cargo verification, spec/proof/exec modes, transition systems, trusted components, external specifications, and calling verified code from ordinary Rust.

Material observations from references remain evidence, not imported authority. File-level reuse requires separate provenance and license review.

Primary external references:

- [Agentic Harness Engineering paper](https://arxiv.org/abs/2604.25850)
- [LemonHarness paper](https://arxiv.org/abs/2606.24311)
- [OpenAI Codex repository](https://github.com/openai/codex)
- [VDD/IAR definitions](https://github.com/Navigators-Guild/apprentice-onboarding)
- [Verus repository and status](https://github.com/verus-lang/verus)
- [Using Verus via Cargo](https://verus-lang.github.io/verus/guide/cargo_verus.html)
- [Verus transition systems](https://verus-lang.github.io/verus/state_machines/)
- [Verus assumptions and trusted components](https://verus-lang.github.io/verus/guide/tcb.html)
- [Calling verified code from unverified Rust](https://verus-lang.github.io/verus/guide/call-from-unverified-code.html)
- [Verus guidance for LLM-assisted proofs and cheat checking](https://verus-lang.github.io/verus/guide/llmforverusproof.html)

## Architecture verdict

**ready**

Evidence for readiness of the design:

- the user-visible outcome and non-negotiable Verus/production/maintainability constraints are explicit;
- public interfaces, domain aggregates, state transitions, persistence, security, failure behavior, and compatibility are defined;
- authority decisions and unavoidable effect boundaries are separated with a concrete maximum-verification policy;
- crate ownership and dependency direction prevent god files/crates and support parallel isolated work;
- each implementation slice names dependencies, owned paths, deliverables, tests, and completion evidence;
- no intermediate stage is represented as a releasable MVP;
- all requested subsystems culminate in one production acceptance and release qualification gate.

Residual risks are implementation risks, not unresolved architectural blockers: Verus feature/toolchain limits may require narrowly documented exclusions; three native sandbox backends require specialized review; formal proof and cross-platform recovery will be expensive; provider behavior is externally unstable; and self-evolution remains vulnerable to evaluator overfitting without disciplined sealed datasets and promotion authority. The design makes each risk visible, owned, testable, and incapable of silently becoming success.
