# Project Peritus

Peritus is a local-first coding-agent harness built primarily in Rust and Verus. It combines:

- explicit workspace and durable-state semantics;
- a tight inspect, edit, run, test, and diagnose loop;
- writer, independent reviewer, and fixer orchestration;
- evidence-based acceptance, recovery, observability, and harness evolution; and
- a terminal product that handles provider setup, workspaces, long-running tasks, and handoff.

The aim is a production coding harness, not a demonstration or an MVP. All planned architecture
slices have implementations, tests, policy surfaces, and operator documentation. The repository is
still under final production qualification, so it deliberately reports `NotReadyForProduction`
until the exact release candidate passes every required native-host and release gate.

## Current development state

| Area | Implemented capability |
| --- | --- |
| A0-A3 foundation | Pinned Rust and Verus toolchains, architecture policy, conformance support, and the versioned application protocol |
| B0-B3 authority | Lifecycle kernel, policy, capabilities, budgets, leases, approvals, quality policy, and domain protocol |
| C0-C7 substrate | Durable state, worktrees, processes, sandboxes, tools, model providers, context and memory, traces and telemetry |
| D0-D3 engines | Agent loop, deterministic gate DAG, independent review, resource-aware scheduling, and collaboration |
| E0-E3 orchestration | Writer-reviewer-fixer delivery, harness materialization, failure analysis, and isolated evaluation |
| F0 evolution | Evidence-bound harness changes with human-controlled promotion and rollback |
| G0-G4 product | Daemon, CLI, TUI, plugins and MCP, provider onboarding, managed workspaces, conversational coding runs, packaging, and self-update |
| H0-H4 qualification | Security, resilience, platform, performance, artifact, provenance, and release-readiness machinery |

The product supports direct OpenAI, Anthropic, Gemini, and compatible API routes. It also supports
ChatGPT and Claude subscription accounts through the official `codex` and `claude` executables.
Those executables own login and model transport; Peritus retains conversation, tool, workspace,
review, and policy authority. When two or more providers are selected, provider settings offer
explicit automatic-failover consent. A role switches only after its selected provider exhausts
ordinary recovery, never for safety, refusal, cancellation, or ambiguous-acceptance outcomes, and
records the exact switch in durable trace and live progress evidence. Once a route fails over, a
run-scoped circuit keeps later roles from repeatedly calling that same unavailable provider when a
healthy consented fallback exists.

Long-running coding tasks persist their conversation, design, candidate changes, findings, trace,
and handoff state. Productive work can continue across bounded segments and daemon restarts.
Malformed or stalled provider turns use bounded retries with traced backoff and jitter. Completion
requires repository-grounded inspection, deterministic project checks, independent review, and an
exact accepted revision. A long inspection sequence that produces no workspace mutation or
declared external effect receives a finite in-session correction toward a concrete delivery step.
When a caller-authorized operational request asks for a live result, supporting scripts and
documentation are accepted only alongside a successful effect and a later fresh verification.

The product runner now checkpoints the exact workspace and conversation after mutations,
verification commands, gates, review, and fixer work. Provider failure, cancellation, or a deadline
after useful work returns the strongest candidate with its current, stale, or missing acceptance
evidence and concrete remaining work. A continuation validates the workspace and conversation
identity and resumes at the first incomplete phase, so a reviewer retry does not repeat design,
writing, or already-current gates. Every ordinary exit has protected finalization time; only an
invalid initial request or an impossible internal invariant escapes the settlement boundary.

The daemon now persists that checkpoint, its typed settlement, the continuation state, remaining
work, and the interruption cause as one durable product-run record. On restart it validates
terminal candidates against the current workspace, marks superseded qualification evidence stale,
and automatically resumes interrupted work from the first reusable phase. The TUI and scriptable
CLI show the exact candidate paths, checks, review, run command, and remaining work. Users can
inspect, run, continue, export, accept, commit, or discard the retained candidate; accepting or
committing an unqualified candidate requires an explicit evidence-naming confirmation.

The generic capability matrix now covers all eleven remediable failure families: completion,
selective resume, measured performance, real lifecycle ingress, directional schemas, browser
semantics, provider recovery, repository drift, ordinary prerequisites, terminal control, and
external-adapter settlement. Every family has passing, honestly incomplete, and failing fixtures
bound to typed product evidence or directly observed process behavior. None uses benchmark task
identities, hidden verifier answers, paid provider calls, or an external benchmark run.

## What remains before release

The implementation is not the release decision. Production readiness still requires:

1. completing the accepted broadly useful repairs found by the finished Terminal-Bench 2.0
   diagnostic campaign;
2. freezing the remediation candidate after every retained failure has an honest systemic,
   evaluator-integrity, or candidate-quality disposition;
3. only then, under an explicit qualification start, running both complete benchmark suites once
   with one exact, revision-bound final binary;
4. running H0-H4 against the exact final commit on Linux, macOS, and Windows;
5. retaining the eight-hour soak, signature, provenance, reproducibility, migration, and recovery evidence;
6. receiving an independent final audit and an H4 `Ready` result; and
7. publishing the first signed GitHub release only after every required hosted check is green.

The H4 adapter cannot sign, tag, publish, deploy, or manufacture missing evidence. Human release
authority remains outside the harness.

H0, H1, and H2 now have standard native process boundaries instead of requiring each release
operator to reinvent one. H0 runs one reviewed probe per case. H1 keeps one reviewed controller
alive across prepare, fault injection, recovery, and cleanup. H2 stages a fresh exact package copy
for each of its 18 scenarios and publishes a complete no-overwrite report through `peritus-h2`.
All three bind responses to the exact candidate or package, verify retained raw artifacts, and own
the complete process tree. H2 now includes the Rust native controller that drives the packaged
installers, daemon, CLI, TUI, process, sandbox, upgrade, rollback, and uninstall checks. Its first
Linux development run passed all 18 scenarios with complete cleanup. The TUI scenario now launches
the installed interface in a native PTY, negotiates with the packaged daemon, observes a rendered
frame, sends Ctrl-Q, and verifies successful terminal restoration. Final candidate-bound Linux,
macOS, and Windows reports are still required; a development report is not release evidence.
The native package workflow now runs the same complete controller on all three hosted operating
systems and retains each report plus raw evidence. Its macOS and Windows sandbox scenarios call the
actual Seatbelt and AppContainer/Job Object host probes; hosted results remain evidence only after
the corresponding revision finishes successfully.

H1 now has all 43 checked-in production controller routes. Focused native runs against freshly
built binaries passed both sides of the journal, blob, retained Git snapshot, exclusive-lease,
patch, D1 gate, and F0 campaign/pointer promotion commits. A projection-corruption route also
proves that startup rejects corrupt active bytes and atomically installs a fresh verified
generation without changing the authoritative journal. A journal-corruption route proves that
fresh startup detects a changed committed frame and stops before authority mutation. A
blob-corruption route now changes a referenced content-addressed object and proves that restart
quarantines the divergent bytes, retains their audit root, and denies further use. The staged
snapshot-corruption route redirects a retained snapshot ref to the wrong commit, then proves that
fresh recovery atomically removes it from active use and retains the divergent value under the
quarantine namespace. An acceptance-evidence corruption route changes the portable record bytes
without changing their indexed identity, then proves that fresh evidence-store startup preserves
the exact corrupt row in a digest-bound quarantine and denies it to every subsequent reader. The
harness-promotion corruption route now publishes both real F0 activation directives, corrupts only
the harness-activation evidence, and proves fresh startup quarantines it without changing the
already-committed 16-event, four-head activation or production pointer. Six
dependency routes now exercise real executable-backed provider failure,
the ordinary grounded and receipt-backed product command tool, and daemon-owned worker tasks. A
fresh scheduler replay either requeues the exact owned work after one dependency death or preserves
explicit exhausted non-success after consuming the configured retry ceiling. The artifact-finalize
disk route also drives two real writers through one durable logical quota, proves the losing
finalization rolls its already-published bytes back, and verifies the admitted artifact after a
fresh process opens the store. All eleven E0 lifecycle routes now commit the shortest legal
writer/gate/reviewer/fixer/acceptance reducer prefix through C0, kill the staged `peritusd` at the
named durable phase, and require a fresh process to reproduce the exact state, event count,
aggregate head, child ownership, handoff, proposal, or B2 certificate. The staged daemon was also
killed with checked work still unpublished, and again after each corresponding durable commit.
Recovery proved rollback for the before cases and exact replay for the after cases, including lease
projection identity, exact patch postimage bytes, complete gate successor state, and all-or-nothing
campaign, production-pointer, and approve-once state. This is useful evidence, not an H1 readiness
claim: production quota routes now prove that artifact finalization and snapshot-manifest
publication leave no rejected bytes, metadata, or retained Git reference, while journal exhaustion
leaves no partial command, event, or aggregate head. The final three routes boot a digest-bound
Alpine guest under QEMU/KVM, stage the exact static `peritusd`, and prove outstanding-effect,
durable-before-ack, and startup-reconciliation recovery across changed guest kernel boot IDs. All
three focused reboot diagnostics passed with exact single-effect and cleanup evidence. A complete
43-case run against the eventual exact release revision is still required before H1 can be Ready.

H0 also has a Rust-owned exact-candidate preparer and a three-host native workflow. The preparer
derives the common candidate identity from committed source, binds the controller and native host
facts, and refuses dirty or cross-platform inputs. Each host runs four isolated worker partitions,
reassembles them in catalog order, and retains the same canonical platform report and raw evidence.
Cross-host aggregation and a separately supplied independent review remain mandatory; the workflow
cannot manufacture either one.

All hosted jobs are capped at ten minutes. Rust and Verus checks are partitioned by the reviewed
architecture layers, H2 assigns one of its 18 scenarios to each shard, and tagged releases build
each native binary separately before package assembly and attestation. Required Gate A check names
remain stable and aggregate the complete shard set.

The tagged release workflow now stages each native archive, generates a candidate-bound inventory,
SPDX SBOM, and SLSA provenance document in Rust, and retains GitHub keyless Sigstore attestations.
The Rust `peritus-h4` operator now prepares and verifies candidate-bound Ed25519 evidence envelopes,
replays signed fresh-subject cleanup records, compares independent builds, assembles all 25
acceptance criteria, reconstructs the independent final audit and manifest, and emits one
no-overwrite Ready/NotReady bundle through verified release policy. This supplies the release
mechanism; the unfinished exact-candidate campaigns and independent audit remain the release
blockers.

## Benchmark qualification

The completed pinned HarnessBench diagnostic campaign contains all 106 tasks with no missing native
adapter run. Its retained means are:

| Measure | Result |
| --- | ---: |
| Outcome | 0.8969 |
| Process | 0.9286 |
| Security | 1.0000 |
| Combined | 0.8331 |

Forty tasks have perfect outcome and 64 score at least 0.9. The retained runs total 8.529 execution
hours and 31,286,948 model tokens. Failures produced general improvements to recovery, grounding,
verification, evidence handoff, multi-turn state, tool use, and artifact consistency. Peritus did
not add task-specific answers or weaken the upstream oracles. The
[benchmark integrity appendix](docs/benchmark-integrity-appendix.md) records cases where a
score-only workaround would require hidden-answer leakage, a task-specific hack, or breaking the
published contract.

A Rust-owned publisher now reconstructs that baseline from the retained result tree, checks exact
coverage against all 106 pinned tasks, and records the selected report and SHA-256 for each task.
The diagnostic run predates embedded build identity, so that limitation is explicit; the final
single-binary campaign will reject any task without native source and executable identity.

Because general fixes were intentionally made as failures were diagnosed, those 106 reports bind
successive development checkpoints. They are the retained diagnostic baseline. The final report
will compare them with a second complete run made by one frozen, revision-bound release candidate.

The frozen Terminal-Bench 2.0 diagnostic campaign is complete: all 445 serialized trials finished,
with 239 rewards of 1, 151 rewards of 0, and 55 unscored trials. Accuracy across the 390 scored
trials is 0.6128; success across all completed trials is 0.5371. This retained baseline spans
successive development checkpoints and therefore is diagnostic evidence, not the final-candidate
score. Its immutable normalized report has SHA-256
`d7feff820c7d38d204744f75ef9214cb7b91949cac2c8c3b5625f10c39321bc0`. Setup, exact evidence,
results, and reproduced failures are documented under [external benchmarks](benchmarks/README.md).

The remediation branch now gives both external suites one versioned admission and settlement
boundary. It performs real provider canaries before expensive work, retains exact candidate and
qualification evidence, and atomically publishes one native report or a separate recovery report
for every admitted attempt. External verifier rewards remain independent of the product's strict
accepted/candidate/failed disposition.

## Install and run

No public release exists yet. From a source checkout, build and install the current native package
for your user account:

```sh
cargo xtask product-install
peritus
```

After the first release is published, Linux and macOS users will be able to install it with:

```sh
curl -fsSL https://raw.githubusercontent.com/Corvidae-Coding-Projects/Project-Peritus/main/install.sh | sh
```

Windows PowerShell users will be able to run:

```powershell
irm https://raw.githubusercontent.com/Corvidae-Coding-Projects/Project-Peritus/main/install.ps1 | iex
```

The bootstrap selects the correct release asset, verifies its SHA-256 digest, and invokes the same
transactional installer used by source builds. Peritus checks for updates at startup at most once
every six hours and never blocks offline use. Use `peritus update` for a manual check,
`peritus update --disable-checks` to disable automatic checks, and `--enable-checks` to restore
them.

Run `peritus` inside a Git repository to open it automatically. The first launch guides provider
login and workspace trust without requiring environment exports or hand-written configuration.
Useful direct commands are:

```text
peritus open [PATH]   Open a specific workspace
peritus providers    Configure or switch providers
peritus workspaces   Inspect or repair managed workspaces
peritus update       Check for a product update now
```

In the TUI, press `n` to create a task. Select a run and press Enter or `m` to add context, answer a
question, redirect work, or continue a failed or completed run in the same managed worktree. After
acceptance, inspect the diff and choose whether to commit, export, or discard it. The
[product-experience guide](docs/g4-product-experience.md) contains the complete interaction and
recovery reference.

## Develop and verify

Peritus pins Rust `1.97.1`, Verus `0.2026.08.09.92f466f`, and vstd revision
`92f466f247f45128c630d1c843fd6e27d2115587`. With those tools installed, use the checked command
surface:

```text
just check          # format, build, tests, Clippy, docs, and workspace policy
just licenses       # dependency, source, and license policy
just toolchain      # confirm the Rust, Verus, vstd, and Z3 pins
just ordinary-api   # check the safe-Rust boundary around formal code
just test           # run unit and qualification-contract suites
just verus-verify   # verify all declared Verus roots and trust policy
just verus-build    # build the verified release configuration
just gate-a         # run the complete merge gate
```

Keep local builds resource-aware:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --workspace
```

`architecture.toml` is the reviewed registry for crate ownership, dependency layers, verification
classes, trusted roots, and source-size policy. Every crate README names a package-specific check.
`cargo xtask docs-check` validates maintained Markdown structure and local links; `cargo xtask all`
includes it with the other repository policy checks.

## Repository guide

- [Documentation index](docs/README.md) explains the architecture and points to each operator guide.
- [Benchmarks](benchmarks/README.md) covers HarnessBench, Terminal-Bench, and the failure journal.
- [Packaging](packaging/README.md) describes native packages and installation layout.
- [Release](release/README.md) describes release evidence and publication.
- [Security](security/README.md) lists security qualification assets and boundaries.
- [Verification](verification/README.md) describes Verus roots, trust accounting, and proofs.
- [xtask](xtask/README.md) documents the checked repository-policy commands.

Generated workspaces, benchmark results, account state, credentials, and large traces belong outside
Git. Only durable code, schemas, policies, documentation, and reproducible summaries are committed.
