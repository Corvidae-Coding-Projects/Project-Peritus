# Project Peritus

Peritus is a local-first, Verus-first coding-agent harness under active production qualification.
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
  immutable wire fixtures and fresh-subject A2 conformance. The Claude account route keeps native
  Claude tools disabled and projects the same typed Peritus tool catalog into a structured inert
  call protocol, normalizes the observed schema-valid nested form through the same fail-closed
  declared-tool validator, and leaves Peritus as the only tool executor; and
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
- G4: the `peritus` no-argument product entry discovers platform-native
  per-user directories, creates protected roots and stable local identities, publishes a canonical
  public approval registry and strict daemon configuration, resumes immutable-generation setup
  state, resolves a version-matched packaged `peritusd`, and starts or reuses its local endpoint
  before entering the TUI. First launch now presents a provider catalog with visible readiness and
  useful defaults; delegates ChatGPT/Claude subscription login to the official `codex`/`claude`
  clients; configures direct OpenAI, Anthropic, Gemini, and compatible routes using hidden input and
  the operating-system credential store; persists switching/default/offline choices; and exposes
  focused repair plus `peritus providers` settings without environment exports or hand-written
  configuration. It now also discovers the current Git root, accepts `peritus open [PATH]`, keeps a
  most-recent workspace list, names the exact repository before trust, starts unknown repositories
  in restricted mode, creates a separate application-managed detached worktree when trusted,
  publishes and recovers canonical C1 registrations, reports clean/dirty/repair state, switches or
  forgets entries through `peritus workspaces`, and restarts a running daemon when its generated
  configuration or installed executable changed. Its daemon-owned coding-run service accepts a
  task and explicit provider choice for each role, then composes the production D0 developer loop,
  changed-target D1 gates, typed D2 finding conservation, and E0 writer/reviewer/fixer decisions in
  the managed worktree. A mandatory read-only design pass first writes a durable detailed Markdown
  plan covering acceptance, architecture, concrete files and modules, slices, and verification.
  Source repositories receive a model-authored design grounded by successful repository listings
  and targeted reads. Explicit generated-artifact workspaces receive a proportional Rust-rendered
  design grounded in the exact durable conversation and bounded filesystem inventory, preserving
  the implementation deadline for the actual artifact work. Embedded architect, developer, and
  reviewer skills require cohesive modules,
  thin composition roots, explicit interfaces, and collision-aware slices. Writers and fixers can
  inspect, search, edit, run, test, observe failures, and retry through bounded structured tools,
  but cannot mutate an existing file before reading it in the current turn. Provider-negotiated
  tool batches execute in proposal order, and unchanged full-file writes report that no bytes
  changed instead of rewriting the target. Exact identifiers and syntax remain literal across
  every generated artifact that records them. Independent review is
  also a fresh D0 loop: it must list and read the real workspace through a separately enforced
  read-only executor before its typed verdict can enter D2. Recoverable malformed, empty, timeout,
  and transport responses receive fresh bounded attempts; productive 48-turn work
  segments checkpoint the exact candidate and replenish the run in a compact newly grounded
  context, while a no-change exhausted segment stops. Rejected ungrounded or malformed task-level
  terminals receive their exact contract failure on the next bounded attempt instead of repeating
  the same prompt without useful feedback. Interrupted goals resume automatically on daemon restart
  from their persisted conversation, findings, trace, and managed worktree. Completion is refused
  unless every exact changed project satisfies the 500-line source ceiling and has explicit
  passing compile/build/test/lint evidence with no policy-derived blocker. Conventional
  manifestless Python and Node projects bind to their nearest tests, including root-level Python
  test conventions; changed YAML and JSON receive bounded structural parsing; conventional Python
  requirements receive an offline read-only satisfaction check so a missing production dependency
  cannot be hidden by a test substitute; standalone changed Python modules receive syntax evidence
  even without a manifest or supplied tests; performance claims require a same-workload unchanged
  baseline and candidate measurement; Python checks avoid cache side effects; and SQLite migration
  workspaces execute the schema, forward migration twice,
  postcheck, and rollback in a disposable Rust-owned database. Trusted-workspace repair now
  re-registers a validated advanced detached HEAD without discarding agent commits or unfinished
  files, and status accepts Git's ordinary trailing-slash directory records. The daemon persists
  every visible phase, durable finding state, and task-level summary, recovers interrupted records,
  and supports query, cancellation, retry, and conversational continuation through canonical A3
  messages. A completed run carries its managed path, exact changed files, successful commands,
  run instructions, design path, and inspect/accept/commit/export/discard handoff. The TUI provides
  an accessible task composer, textual progress timeline, diff and review/check views,
  role-provider switching, handoff controls, cancellation, retry, and a visible `R
  restart/reconnect` action when the daemon link drops. Host-native package assembly now
  installs the launcher,
  daemon, TUI, sandbox helper, lifecycle scripts, manifest, and checksums while preserving product
  state across repeat install, upgrade, rollback, and uninstall; and
- H0: a verified exact-candidate security-readiness policy plus a 42-case fresh-native-subject
  campaign covering R-SEC-001 through R-SEC-007, the nine security-relevant acceptance criteria,
  malicious repositories, native sandboxes, role isolation, evidence invalidation, evolution,
  observability/redaction, supply chain, unsafe/TCB reconciliation, independent review, bounded
  resources, cancellation, panic containment, cleanup, and canonical evidence; and
- H1: a 43-scenario deterministic resilience qualification catalog covering every authoritative
  commit boundary, active daemon phase, corruption and disk-exhaustion class, provider/tool/worker
  death, reboot and reconciliation path, with fresh subjects, bounded cleanup/resource accounting,
  canonical evidence, false-success rejection, and a fail-closed production verdict; and
- H2: typed Linux, macOS, and Windows package/layout/service/transport/sandbox/process-equivalence
  contracts, fresh packaged-host qualification, and per-user install, upgrade, rollback, uninstall,
  systemd, launchd, and Task Scheduler assets that preserve protected configuration and state; and
- H3: deterministic workload, profile, SLO, measurement, accounting, baseline-comparison, evidence,
  load, and eight-hour soak machinery with a dedicated Criterion benchmark target, stable schemas,
  bounded-resource/backpressure evaluation, and no fabricated performance baseline; and
- H4: a verified exact 25-criterion/44-requirement release policy, deterministic SPDX 2.3 SBOM,
  provenance, detached Ed25519 verification, independent-builder reproducibility, migration,
  recovery, licensing and artifact contracts, eleven fresh-subject final campaigns, signed H0-H3
  inputs, independent final audit, content-addressed evidence, and a fail-closed composition adapter
  that cannot sign, tag, publish, deploy, or manufacture evidence.

All original architecture slices A0 through H4 and the G4 product composition now have implemented
code, tests, formal-policy surfaces, schemas, and operating documentation. G3 deliberately cannot
mint C4/B1 authority:
packaged application embedding must supply a current daemon-owned mediator for each exact run,
workspace, and target. Qualification machinery does not fabricate a production verdict. A release
still requires running the H0-H4 campaigns against the exact final commit, retaining reviewed
native-host, eight-hour-soak, multi-language, signature, reproducibility, and independent-audit
evidence, and obtaining an H4 `Ready` decision. Until that evidence exists, Peritus remains
`NotReadyForProduction` by construction.

External production qualification is active against the pinned, unchanged 106-task HarnessBench
suite. Tasks 001 through 075 have run sequentially with full local reports and failure diagnosis;
task 051 passed all 21 checks cleanly, and the latest task 052 run passed all 17 external checks
after exercising ambiguity handling, category boundaries, provider-stall recovery, and finding
conservation. The source now treats advisory review as nonblocking and conserves stable finding
titles across updated location evidence. The final unchanged task 052 rerun completed natively
against finding identity version 2 with outcome 1.0, process 0.93, security 1.0, and combined 0.93.
Tasks 053 and 054 then completed natively on their first cycles with every transaction-analysis
and budget-variance oracle check passing. Task 055 exposed avoidable fresh-fixer grounding
rejections; after making that protocol explicit without weakening enforcement, its unchanged rerun
passed all 24 checks natively in one cycle with process 0.9867. Tasks 056 and 057 completed natively
with correct inventory calculations and preserved two-round resume state; their remaining oracle
deductions require contradictory or unpublished output conventions and are retained honestly.
Task 058 exposed a genuine mismatch between instructions that required batched writes and a
developer-loop request that disabled them. The provider-neutral loop now uses the negotiated batch
width, unchanged writes are explicit no-ops, and exact identifiers remain literal across artifacts.
The unchanged three-day state task now completes natively, writes all three final artifacts in one
batch, passes 10 of 11 oracle checks, and scores outcome/process/security/combined
0.9375/0.9233/1.0/0.8656. Its remaining hidden lexical check is retained rather than bench-tuned.
Task 059 then exercised a genuine two-round event replan. Peritus now keeps explicitly named staged
inputs isolated until their round introduces them and makes change reports account for changed,
added, removed, and already-satisfied constraints with literal values. The unchanged rerun passed
every oracle check with outcome 1.0, process 0.9533, security 1.0, and combined 0.9533.
Task 060 passed its cancellation oracle on both runs and exposed one real cleanup limitation. The
workspace tool can now remove an explicitly listed empty directory non-recursively after removing
its owned files, while still rejecting workspace-root, nonempty-directory, and external-evidence
deletion. The corrected run removed its complete temporary tree and scored
1.0/0.9367/1.0/0.9367.
Task 061 exposed a deadline imbalance: a small time-bound reporting workspace spent most of its
budget in a long generative design turn, leaving too little time for required polling and output.
Explicit artifact workspaces now receive the same mandatory design sections through a fast,
deterministic Rust renderer grounded in the exact conversation and sorted bounded inventory;
source repositories retain the full generative architecture pass. Per-role output ceilings keep
review turns proportional, while the workflow treats independent output categories as independent
predicates and distinguishes periodic polling from one long wait. The final unchanged run observed
the workspace repeatedly across 26 seconds, passed all seven checks, and scored
1.0/0.9867/1.0/0.9867.
Task 062 found all eight Kubernetes policy violations with perfect process and security scores; its
remaining outcome deduction is an unpublished severity taxonomy and lexical synonym check. Task 063
then grouped both topology-rooted incidents, retained downstream symptoms as evidence, and filtered
only true noise for a perfect outcome. Task 064 produced the correct cross-file root-cause chain,
change, impact, red-herring exclusions, mitigation, and verification with process 0.9967; its exact
incident-ID deduction is retained because no ID or format appears in the supplied evidence. Task
065 exposed a genuine permissive-default defect in constraint solving: options without region data
were treated as region-compatible. Hard eligibility and placement constraints now require
affirmative evidence unless an authoritative source declares a default. The unchanged rerun chose
only proven `us-east` plans and improved outcome/process/security/combined from
0.6617/0.95/1.0/0.6286 to 0.9873/1.0/1.0/0.9873.
Tasks 066 and 067 then produced correct rollback-readiness and canary decisions with complete
evidence and safe unexecuted next actions; their deductions come from unpublished severity,
keyword, and normal-status conventions. Task 068 satisfied every launch-plan requirement but an
oracle substring check treated explicit “do not promise” language as the forbidden promise itself.
Task 069 passed all 17 legal-compliance workflow checks with perfect process and security scores.
Task 070 applied the explicit three-skill shortlist rule and avoided protected attributes; its
retained 0.70 outcome reflects hidden ground truth that contradicts that threshold and a raw `age`
substring match inside the job-related word `managers`.
Tasks 071 and 072 then produced correct support-routing and logistics actions with safe customer
messages; their deductions require unpublished reply and compensation identifiers. Task 073
honestly rejected reproducibility from a deposit whose pinned upstream fixture omits the very script
the oracle expects to inspect. Task 074 applied the published evidence rubric literally, while hidden
ground truth assigned the one-detail score to a submission containing at least two accurate details.
Task 075 passed every moderation decision, policy, action, explanation, and redaction check; its only
deduction selects one side of an overlapping confidence-calibration rule.
Terminal-Bench 2.0, the complete professional-capability audit, documentation normalization,
release-installer qualification, and final hosted-runner closure remain required before production
readiness.

Gate A is the current merge authority: ordinary Rust checks, architecture and API policy,
supply-chain policy, pinned toolchains, full Verus verification, and verified release builds must
all pass together. Required GitHub-hosted checks now execute on Ubuntu, macOS, and Windows, with a
separate locked Foundation matrix covering the same platform, dependency, and Verus boundaries.

## Install and run the current product build

From a source checkout, build the checked host-native package and install it for the current user:

```text
cargo xtask product-install
```

After installation, ordinary use is one command. It requires no endpoint, provider export, or
hand-written configuration:

```text
peritus
```

Run it from inside a Git repository to use that repository automatically. Use `peritus open
[PATH]`, `peritus providers`, or `peritus workspaces` for an explicit workspace, provider settings,
or workspace settings. Press `n` in the Runs view to describe a coding task; `w`, `e`, and `f`
select the writer, reviewer, and fixer providers. Select any run and press Enter or `m` to talk to
it: add context while it works, answer a question, redirect it, or continue a failed or completed
run in the same managed worktree. After exact acceptance, use `i` to inspect the diff, `a` to mark
the deliverable accepted, `c` to commit its exact files, `p` to export a patch, or `D` to discard
it. The
[G4 product-experience guide](docs/g4-product-experience.md) explains onboarding, trust, coding
runs, native packaging, state locations, and recovery.

## Foundation checks

Rust `1.97.1`, Verus `0.2026.08.09.92f466f`, and vstd revision
`92f466f247f45128c630d1c843fd6e27d2115587` are pinned. Install those tools, then use the
checked-in command surface:

```text
just check          # format, build, tests, Clippy, docs, and workspace policy
just licenses       # dependency, source, and license policy
just toolchain      # probe the installed Rust/Verus/vstd/Z3 pins
just ordinary-api   # audit formal APIs callable from ordinary safe Rust
just test           # includes deterministic H0-H4 unit and qualification-contract suites
just verus-verify   # full TCB-aware verification plus no-cheating V/H roots
just verus-build    # full verified release plus no-cheating V/H builds
just gate-a         # the complete formal-foundation gate
```

Credentialed C5 qualification is explicit because hosted and ordinary local gates never receive
provider accounts. After authenticating the official executables, run the retained
`peritus-release-qualification` live-account examples documented in the owning provider crate
READMEs. Each probe exercises the production Peritus adapter and requires normalized
usage, exact canary text, no native-tool activity, and a completed terminal.

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
[H0 security guide](docs/h0-security-qualification.md) defines the literal R-SEC and acceptance
catalogs, 42-case fresh-subject campaign, threat/control/unsafe/TCB inventories, independent review,
evidence binding, cleanup, and fail-closed security verdict. The
[H1 resilience guide](docs/h1-resilience-qualification.md) defines the 43-case disruption catalog,
fresh-subject execution, recovery invariants, evidence, and release verdict. The
[H2 platform guide](docs/h2-platform-qualification.md) defines package layouts, native supervisor
contracts, install/upgrade/rollback/uninstall behavior, platform equivalence, and host evidence. The
[H3 performance guide](docs/h3-performance-qualification.md) defines stable workload/profile data,
SLO evaluation, bounded accounting, baseline regression, load/soak execution, and evidence. The
[H4 release-policy guide](docs/h4-release-policy.md) defines the verified 25-criterion and
44-requirement decision boundary. The
[H4 release-qualification guide](docs/h4-release-qualification.md) and
[migration/recovery runbook](docs/release-migration-recovery.md) define signed evidence collection,
release artifacts, independent audit, reproducibility, restoration, policy composition, and the
non-authorizing final verdict. The
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
