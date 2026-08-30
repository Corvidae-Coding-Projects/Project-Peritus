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
exact accepted revision.

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

The tagged release workflow now stages each native archive, generates a candidate-bound inventory,
SPDX SBOM, and SLSA provenance document in Rust, and retains GitHub keyless Sigstore attestations.
This supplies the release mechanism; the unfinished exact-candidate campaigns and independent audit
remain the release blockers.

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
