# Project Peritus

Peritus is a local-first, Verus-first coding-agent harness under active production construction.
It combines explicit durable workspace/state semantics, a tight inspect/edit/run/test loop,
writer-reviewer-fixer orchestration, and evidence-driven harness observability and evolution.

The repository is not yet a releasable product. Implementation is staged for safe parallel work,
but no stage is an MVP and no intermediate stage carries a production-readiness claim.

## Current development state

The implemented foundation and runtime spine now covers:

- A0–A3: pinned Rust/Verus workspace governance, verified foundational types and trust accounting,
  deterministic test support, reusable conformance execution, and the transport-neutral
  application protocol with negotiated versions/features, canonical envelopes, resumable events,
  artifact/prompt/terminal flows, generated schemas, compatibility fixtures, and Verus refinements;
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
  fixtures, migration, Verus obligations, and independent A2 conformance; and
- E3: immutable dataset and evaluator isolation, exact E1/C5/C2/C3 profile binding, deterministic
  paired D3 rollout plans, complete attempt/outcome/resource accounting, frozen integer/fixed-point
  statistical analysis, crash-safe schedule/execution/publication effects, canonical reports,
  C0 evidence admission, protocol fixtures, migration, Verus refinements, and independent A2
  conformance without harness mutation or promotion authority; and
- F0: immutable evidence-citing evolution campaigns, isolated E1 candidate variants and interaction
  groups, deterministic E3-backed attribution and deny-wins multi-objective selection, exact D2
  review and B0/B1 human-authority binding, atomic durable production-pointer activation,
  append-only rollback, crash-safe replay/publication, protocol fixtures, migration, Verus
  promotion/evaluator-isolation refinements, and independent A2 conformance; and
- G0: the production `peritusd` application root with strict protected state configuration,
  singleton local IPC ownership, authenticated durable A3 sessions, one bounded C0 authority owner,
  exact application idempotency and event subscriptions, streaming artifacts, fresh signed-approval
  prompts, C2 terminal bridges, configured C3/C4/C5 provider and tool inventories, bounded worker
  supervision, fenced destination-native outbox delivery, F0 pointer loading, C7 local telemetry,
  ordered startup/recovery/shutdown, explicit read-only diagnostics, Verus lifecycle refinements,
  and an independent 28-case A2 daemon contract with 28/28 public-`peritusd` subprocess coverage,
  including real PTY execution and effect-before-ack kill/recovery qualification; and
- G1: the production `peritus` command-line client with strict dependency-free parsing, protected
  Unix-socket and Windows named-pipe A3 transport, negotiation and session resume, stable human and
  JSON output/exit categories, generic B3 command submission, resumable event streams, artifact
  transfer, prompt settlement, terminal control, heartbeat handling, and shell completions; and
- G2: the production `peritus-tui` client with a deterministic reducer/effect boundary, bounded
  runs/diff/review/trace/evolution/approval/terminal projections, reconnect and cursor resume,
  signed approval input, sanitized terminal rendering, PTY control, and reliable terminal-mode
  restoration; and
- G3: H-class canonical plugin contracts, strict filesystem discovery and trust binding, isolated
  process and Wasmtime-CLI plugin hosting, authority-mediated invocation, lifecycle quotas and
  cancellation, plus a bounded MCP 2025-06-18 JSON-RPC server for authority-filtered tools,
  resources, and prompts, backed by a seven-case runtime-neutral A2 plugin contract; and
- H1: a 43-scenario deterministic resilience qualification catalog covering every authoritative
  commit boundary, active daemon phase, corruption and disk-exhaustion class, provider/tool/worker
  death, reboot and reconciliation path, with fresh subjects, bounded cleanup/resource accounting,
  canonical evidence, false-success rejection, and a fail-closed production verdict; and
- H2: typed Linux, macOS, and Windows package/layout/service/transport/sandbox/process-equivalence
  contracts, fresh packaged-host qualification, and per-user install, upgrade, rollback, uninstall,
  systemd, launchd, and Task Scheduler assets that preserve protected configuration and state; and
- H3: deterministic workload, profile, SLO, measurement, accounting, baseline-comparison, evidence,
  load, and eight-hour soak machinery with a dedicated Criterion benchmark target, stable schemas,
  bounded-resource/backpressure evaluation, and no fabricated performance baseline.

The application, extension, resilience, packaging/platform, and performance-qualification surfaces
are now implemented. G3 deliberately cannot mint C4/B1 authority: packaged application embedding
must supply a current daemon-owned mediator for each exact run, workspace, and target. H1-H3 code
does not itself claim a qualification verdict; those verdicts require execution against the final
integrated release candidate and reviewed native-host/performance evidence. H0 security
qualification and H4 release qualification remain before production release.

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
just test           # includes deterministic H1-H3 unit and qualification-contract suites
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
The [A3 application protocol guide](docs/a3-app-protocol.md) documents version and feature
negotiation, exact B3 command/event bindings, bounded idempotency and resumable subscriptions,
artifact/prompt/terminal flows, daemon controls, stable errors, schemas, compatibility, and the
transport/non-authority boundary.
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
[E3 evaluation guide](docs/e3-evaluation.md) documents immutable datasets and profiles,
candidate/evaluator isolation, deterministic paired planning, complete outcome/resource
accounting, frozen statistical methods, durable execution/publication, replay, migration, and the
non-promotion boundary. The
[F0 production harness evolution guide](docs/f0-evolution.md) documents evidence-bound campaigns,
change manifests, interaction-aware attribution, deterministic selection, exact human promotion
authority, atomic production-pointer activation, recovery, and append-only rollback. The
[G0 daemon guide](docs/g0-daemon.md) documents strict configuration, local application transport,
single-writer authority, durable services, outbox/worker composition, startup and recovery,
readiness, and verification. The companion [recovery](docs/g0-recovery-runbook.md) and
[shutdown](docs/g0-shutdown-runbook.md) runbooks define operator handling for migration, journal,
approval-registry, outbox, process, artifact, timeout, and forced-kill cases. The
[G1 CLI guide](docs/g1-cli.md) documents the complete scriptable A3 surface, stable output and exit
contract, resumable streams, and local transport boundary. The
[G2 TUI guide](docs/g2-tui.md) documents deterministic presentation state, keyboard controls,
reconnection, approval handling, PTY sanitation, and terminal restoration. The
[G3 extensions guide](docs/g3-extensions.md) documents canonical plugin manifests, discovery and
trust, isolated process/Wasm lifecycle, authority mediation, quotas, MCP lifecycle and methods,
conformance, and the remaining daemon-embedding boundary. The
[H1 resilience guide](docs/h1-resilience-qualification.md) defines the 43-case disruption catalog,
fresh-subject execution, recovery invariants, evidence, and release verdict. The
[H2 platform guide](docs/h2-platform-qualification.md) defines package layouts, native supervisor
contracts, install/upgrade/rollback/uninstall behavior, platform equivalence, and host evidence. The
[H3 performance guide](docs/h3-performance-qualification.md) defines stable workload/profile data,
SLO evaluation, bounded accounting, baseline regression, load/soak execution, and evidence. The
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
