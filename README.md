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
records the exact switch in durable trace and live progress evidence.

Long-running coding tasks persist their conversation, design, candidate changes, findings, trace,
and handoff state. Productive work can continue across bounded segments and daemon restarts.
Malformed or stalled provider turns use bounded retries with traced backoff and jitter. Completion
requires repository-grounded inspection, deterministic project checks, independent review, and an
exact accepted revision. A long inspection sequence that produces no workspace mutation or
declared external effect receives a finite in-session correction toward a concrete delivery step.
When a caller-authorized operational request asks for a live result, supporting scripts and
documentation are accepted only alongside a successful effect and a later fresh verification.

## What remains before release

The implementation is not the release decision. Production readiness still requires:

1. completing the serialized Terminal-Bench 2.0 diagnostic campaign;
2. applying only broadly useful fixes found by that campaign and rerunning affected unchanged tasks;
3. rerunning both complete benchmark suites with one exact, revision-bound final binary;
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

H1 now has eight checked-in production controller routes. Focused native runs against freshly built
binaries passed both sides of the journal, blob, retained Git snapshot, and exclusive-lease
commits. The staged daemon was killed with an append plan, artifact, candidate tree, or move-only
lease transition still unpublished, and again after each corresponding durable commit. Recovery
proved rollback for the before cases and exact replay for the after cases, including the lease
event, aggregate head, projection revision, digest, and producing position. This is useful evidence,
not an H1 readiness claim: the remaining 35 catalog routes still need genuine component, quota,
process, and disposable-VM controls.

H0 also has a Rust-owned exact-candidate preparer and a three-host native workflow. The preparer
derives the common candidate identity from committed source, binds the controller and native host
facts, and refuses dirty or cross-platform inputs. Each hosted shard retains its canonical report
and raw evidence. Cross-host aggregation and a separately supplied independent review remain
mandatory; the workflow cannot manufacture either one.

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

The official 89-task Terminal-Bench 2.0 campaign runs five attempts per task at concurrency one to
protect system memory. It is still in progress against a frozen adapter binary. Setup, commands,
results, and reproduced failures are documented under [external benchmarks](benchmarks/README.md).

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
