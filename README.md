# Project Peritus

Peritus is a local-first, Verus-first coding-agent harness under active production construction.
It combines explicit durable workspace/state semantics, a tight inspect/edit/run/test loop,
writer-reviewer-fixer orchestration, and evidence-driven harness observability and evolution.

The repository is not yet a releasable product. Implementation is staged for safe parallel work,
but no stage is an MVP and no intermediate stage carries a production-readiness claim.

## Current development state

The implemented foundation and runtime spine now covers:

- A0–A2: pinned Rust/Verus workspace governance, verified foundational types and trust accounting,
  deterministic test support, and reusable conformance execution;
- B0–B3: the lifecycle kernel, capabilities/policy/budgets/leases/approvals, acceptance contracts,
  quality policy, and the versioned domain protocol/codec;
- C0: the durable journal, rebuildable projections, artifact store, migrations, evidence admission,
  committed B0/B1 receipts, and restart-safe authority observations;
- C1: structured Git/worktree operations, checked atomic patches, target-owned workspace
  authorization, candidates, snapshots, rollback, and restart reconciliation; and
- C2: structured process and PTY execution, the target-owned execution gateway, complete
  platform-neutral sandbox contracts, bounded supervision/output/cancellation and resource
  accounting, durable process recovery and retryable output publication, holder quiescence,
  executable reference semantics, and reusable A2 qualification; and
- C3: target-owned native backend preparation and lifecycle hooks, protected helper channels,
  native Linux, macOS, and Windows enforcement backends and probes, managed HTTP/CONNECT egress,
  exact secret leases and delivery, redaction, native recovery, and complete backend teardown; and
- C4: bounded canonical tool descriptors and JSON schemas, capability/role exposure, target-owned
  one-use authorization and routing, replay/control/result envelopes, and C1/C2/C3-backed
  filesystem, Git, shell, and explicit quality tools with fresh-subject conformance; and
- C5: a versioned provider-neutral model protocol, exact capability negotiation, bounded normalized
  streaming and reduction, deterministic retry/idempotency and cancellation semantics, a hardened
  HTTP/process transport boundary, production OpenAI Responses, Anthropic Messages, stable-v1
  Google Interactions/Generate Content, explicitly profiled compatible endpoints, and separate
  account-backed Codex/Claude routes through their credential-owning official executables, with
  immutable wire fixtures and fresh-subject A2 conformance; and
- C6: canonical role-specific context views, provenance and authority-aware context DAGs,
  deterministic dependency-complete selection and token accounting, validated compaction lineage,
  typed provider-neutral render plans, scoped evidence-backed memory lifecycle and tombstones,
  deterministic explainable retrieval, and rebuildable canonical indexes;
- C7: durable causal trace observations with canonical persistence and replay, default-surface
  redaction, bounded rebuildable projections, non-authoritative metrics, bounded telemetry
  buffering, exporter failure isolation, acknowledgement, shutdown, and restart recovery;
- D0: a durable verified inner-agent state machine and cooperative runtime composition for
  role-scoped context/memory preparation, normalized provider streaming, independently authorized
  C4 tool execution and long-running control, stable result ordering, bounded accounting,
  completion proposals, pause/cancel/retry/recovery, and crash-safe C0 replay; and
- D1: a durable gate DAG engine with exact specification/workspace/snapshot bindings, deterministic
  dependency planning and aggregation, C4-only quality execution, strict structured result parsing,
  clean read-only snapshots, explicit assertion-versus-infrastructure outcomes, bounded retry and
  cancellation, crash recovery, fresh evidence admission, and fail-closed acceptance; and
- D2: a durable deterministic independent-review engine with immutable contract/revision/context
  bindings, bounded structured submissions, separately reported quorum dimensions, stable finding
  provenance and conservation, duplicate reconciliation, fixer/reviewer disposition handshakes,
  externally authorized waiver observation, exact revision invalidation, oscillation/escalation,
  B2 quality projections, and crash-safe C0 replay; and
- D3: durable bounded resource-aware scheduling with dependency readiness, explicit worker
  reservations, deterministic fairness and recovery, plus causal collaboration task trees,
  delegation, message and artifact handoff, truthful joins, cancellation propagation, and
  crash-safe C0 replay; and
- E0: a durable deterministic AcTor delivery orchestrator that composes writer D0 turns, fresh D1
  gates, independent D2 review, bounded fixer revision cycles, D3 work/task ownership, B2
  evaluation, and durable B0 acceptance truth with commit-before-effect directives, pause,
  cancellation, and exact restart reconciliation; and
- E1: strict C1-backed harness manifests and complete typed component catalogs, deterministic
  compatibility/authority graphs, protected controlled assets, immutable content-addressed
  revision DAGs, exact owned-path C1 materialization and ancestor rollback, plus C0 durability,
  replay, projections, protocol fixtures, and independent A2 conformance; and
- E2: immutable subject-bound C7/C0 evidence selection, deterministic causal timelines and closed
  failure taxonomy, citation-complete root-cause analysis, cross-run pattern clustering, E1
  component correlation and harness-health summaries, optional strictly validated C5/C6 model
  assistance, plus crash-safe jobs, replay, report artifact/evidence publication, protocol
  fixtures, migration, Verus obligations, and independent A2 conformance.

These are library and verification layers. There is not yet a user-facing `peritus` CLI, daemon,
TUI, harness evaluation/promotion runtime, or native packaged-host qualification. A3, E3, F0,
G0–G3, and H0–H4 remain before production release and qualification.

Gate A is the current merge authority: ordinary Rust checks, architecture and API policy,
supply-chain policy, pinned toolchains, full Verus verification, and verified release builds must
all pass together. Required GitHub-hosted checks now execute on Ubuntu, macOS, and Windows, with a
separate locked Foundation matrix covering the same platform, dependency, and Verus boundaries.

## Foundation checks

Rust `1.97.1`, Verus `0.2026.08.09.92f466f`, and vstd revision
`92f466f247f45128c630d1c843fd6e27d2115587` are pinned. Install those tools, then use the
checked-in command surface:

```text
just check          # format, build, tests, Clippy, docs, and workspace policy
just licenses       # dependency, source, and license policy
just toolchain      # probe the installed Rust/Verus/vstd/Z3 pins
just ordinary-api   # audit formal APIs callable from ordinary safe Rust
just verus-verify   # full TCB-aware verification plus no-cheating V/H roots
just verus-build    # full verified release plus no-cheating V/H builds
just gate-a         # the complete formal-foundation gate
```

All dependency-resolving commands use `--locked`. `architecture.toml` is the reviewed registry
for crate ownership, dependency layers, verification classes, trusted source roots, and source
size exceptions. New crates must inherit the workspace package metadata and lints, declare their
owner/layer/class in Cargo metadata, and be registered in that policy file.

The checked `cargo xtask` interface also works from a workspace member directory. Root CI rejects
nested or legacy Cargo configuration before that convenience is considered trustworthy, so a
repository that has not passed the root gate must not treat a member-local Cargo alias as evidence.

The [foundation toolchain policy](docs/foundation-toolchain.md) documents the exact pins, accepted
Verus cfg names, locked-input rules, and the known cargo-verus/bundled-Z3 metadata discrepancy.
The [formal foundation](docs/formal-foundation.md) documents the verified value types, zero-cheat
TCB baseline, semantic manifests, and the claims that A1 deliberately does and does not establish.
The [test and conformance foundation](docs/test-conformance-foundation.md) defines deterministic
clock, identifier, event, fault, script, provider, tool, repository and content-addressed fixture
semantics, plus the runtime-neutral conformance runner and its fail-closed suite verdicts.
Focused A2 checks are `cargo test --package peritus-test-support --all-targets --all-features
--locked` and `cargo test --package peritus-conformance --all-targets --all-features --locked`.
The [C0 durable-state guide](docs/c0-durable-state.md) documents the journal, projections,
artifacts, migrations, and evidence boundary. The [C1 workspace guide](docs/c1-workspaces.md)
documents structured Git worktrees, typed atomic patches, target-owned authorization, snapshots,
rollback, and restart reconciliation. The
[E1 harness-materialization guide](docs/e1-harness-materialization.md) documents strict manifest
inventory, checked component graphs, immutable revision history, C0/C1 materialization, restart,
and ancestor rollback. The
[E2 debugger guide](docs/e2-debugger.md) documents exact subject and evidence binding,
deterministic selection/timelines/causes/clustering, closed taxonomy, citations, optional validated
model analysis, durable replay/publication, and the non-mutation/non-authority boundary. The
[C2 process and sandbox guide](docs/c2-process-sandbox.md) documents structured process execution,
complete sandbox contracts, target-owned launch authorization, bounded supervision, terminal
accounting, restart reconciliation, and holder quiescence.
The [C3 platform security guide](docs/c3-platform-security.md) documents the native backend seam,
protected helper protocol, Linux/macOS/Windows enforcement and probes, managed egress, exact secret
delivery, teardown, recovery, and the distinction between implementation and packaged-host
qualification.
The [C4 tool system guide](docs/c4-tool-system.md) documents bounded schemas and envelopes,
capability/role exposure, two-phase authorization and one-use routing, C1/C2/C3-backed built-ins,
owned controls and replay, and the boundary between quality invocation and the future D1 gate DAG.
The [C5 model provider guide](docs/c5-model-providers.md) documents the provider-neutral protocol,
verified reduction and retry semantics, hardened HTTP/process ownership, official first-party API
and account-runtime contracts, explicit compatible profiles, immutable fixtures, and provider
conformance boundary.
The [C6 context and memory guide](docs/c6-context-memory.md) documents canonical role views,
provenance-aware context graphs, deterministic selection and token planning, validated compaction,
typed rendering, scoped derived-memory lifecycle, explainable retrieval, and rebuildable indexes.
The [C7 trace and telemetry guide](docs/c7-trace-telemetry.md) documents causal durable
observations, redaction, replay, projections, bounded buffering, export acknowledgement, failure
isolation, shutdown, and restart recovery.
The [D0 agent-loop guide](docs/d0-agent-loop.md) documents durable inner-turn transitions,
provider acknowledgement, independent tool authority, bounded parallel execution and control,
budget/limit handling, completion proposals, and honest restart recovery.
The [D1 gate-engine guide](docs/d1-gate-engine.md) documents deterministic gate planning and
aggregation, exact revision and clean-snapshot freshness, C4 quality execution, strict parsing,
evidence admission, bounded retry/cancellation, and crash-safe replay.
The [D2 review-engine guide](docs/d2-review-engine.md) documents immutable review bindings,
structured submissions, independent quorum, finding conservation and reconciliation,
fixer/reviewer dispositions, externally authorized waiver observations, revision invalidation,
truthful escalation, B2 projections, and crash-safe replay.
The [D3 scheduler and collaboration guide](docs/d3-scheduler-collaboration.md) documents bounded
resource scheduling, deterministic fairness, dependency readiness, worker ownership, causal task
trees, joins, handoffs, cancellation propagation, and restart recovery.
The [E0 AcTor orchestrator guide](docs/e0-actor-orchestrator.md) documents exact writer, gate,
reviewer, fixer, evaluation, and B0 handoffs; bounded revision loops; commit-before-effect
directives; pause and cancellation; replay; and terminal acceptance truth.
The [GitHub governance runbook](docs/github-governance.md) defines the GitHub Team-compatible
repository ruleset and required `Gate A` status that must be active after the A1 genesis push.
Immutable required-workflow authority remains an explicitly documented Enterprise Cloud deferral.
