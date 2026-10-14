---
name: rust-reliability
description: Prevent and investigate Rust reliability failures in agent-written or human-written code through enforceable API and lint restrictions, ownership and cancellation analysis, and real execution. Use for Rust implementation, refactoring, debugging, or review involving async work, Tokio, queues, shared state, panic recovery, shutdown, unsafe abstractions, or suspicious compiler workarounds. Extend the real-testing skill with Rust-specific failure scenarios; do not generate unit-test suites or require Verus.
---

# Rust Reliability

Constrain the code where a rule is mechanically enforceable. Operate the actual program where correctness depends on behavior. Keep the acceptance contract independent of the implementation.

## 1. Establish scope and the observable contract

- Read project instructions, the requirement, the changed code, and its callers. Identify the affected crates, targets, features, toolchain, and runtime dependencies.
- Read and apply the installed `real-testing` skill. Use it for building, launching, operating real interfaces, systematic exploration, fixing failures, and preserving reproductions. This skill adds Rust-specific analysis and scenarios. If it is unavailable, follow the equivalent procedure in [behavioral-scenarios.md](references/behavioral-scenarios.md); report that the named dependency was unavailable without abandoning reachable work.
- State what acceptance, completion, cancellation, shutdown, and recovery mean for this feature. Distinguish submitting work from accepting it, and enqueuing work from durably recording or completing it.
- Define expected observations before execution from requirements, documented APIs, or an independent reference. Specify legitimate outcomes for races. Do not invent exactly-once delivery, durability, ordering, or rollback guarantees.
- Separate changes that fix a demonstrated bug from speculative hardening. Preserve existing supported behavior unless the task authorizes changing it.

## 2. Classify the affected failure paths

Read [rust-failure-modes.md](references/rust-failure-modes.md) for the relevant rows. Trace through callers and supervisors, not just the edited function.

| Changed behavior | Investigate |
| --- | --- |
| Futures, `select!`, timeouts, try-joins | Who owns pending data, which future gets dropped, progress lost on restart, caller cancellation, cancellation blast radius |
| Shared mutation and locks | Invariants at every suspension point; panic paths; lock order; synchronous blocking on runtime threads |
| Queues and streaming I/O | Backpressure, full/closed queues, partial I/O, loss, duplicate effects, bounded resource use |
| Workers and shutdown | Task ownership, observed join results, abort behavior, drain policy, deadlines, runtime destruction |
| Unsafe, FFI, generic collections | Actual safety obligations; arbitrary safe callbacks; allocation failure and panics; stable identities and aliasing |
| Compiler-error workarounds | New `unsafe`, leaked memory, pervasive shared ownership, clones, erased types, ignored errors, broad lint suppression |

Treat these as investigation triggers. `select!`, `Arc`, cloning, async mutexes, and `unwrap` are not automatically bugs. Do not forbid a useful construct merely because a text scan finds it. Identify the violated requirement or invariant before changing it.

## 3. Enforce applicable structural safeguards

- Prefer a small, typed public API around dangerous operations. Keep representation private; restrict constructors and transitions so callers cannot create the prohibited states. Validate deserialized/external data at the boundary.
- Keep pending message ownership outside a cancellable wait. For Tokio MPSC, consider reserving capacity first and synchronously transferring the message through the permit. Account for closure, fairness, and receiver failure; reservation does not establish durable delivery.
- Prepare state changes before committing them, and keep in-memory commits synchronous where practical. Ensure every suspension point exposes a valid state, or transfer ownership to a supervised operation that completes or explicitly recovers the transition.
- Keep client disconnection separate from cancellation of already accepted work when the contract requires completion. Provide safe cooperative stop points. A single-owner actor is not automatically cancellation-safe, and spawning does not survive runtime destruction or process failure.
- Aggregate shutdown errors after driving all required cleanup operations. Apply deadlines and a documented forced-stop policy when necessary. Do not replace every try-join indiscriminately.
- Preserve and observe task ownership and join outcomes. Retain handles in a supervisor; avoid accidental detachment and accidental `JoinSet` drop-aborts.
- For ordinary generated modules, use a project-appropriate unsafe-code restriction. Keep necessary unsafe code in an explicit reviewed boundary; do not extend restrictions across an existing codebase blindly. Check dependencies and FFI separately.
- Use approved Clippy restrictions for disallowed APIs/types/macros where the project supplies a safer wrapper. Check supported lint names against the pinned toolchain. Keep exceptions narrow and justified.
- Reject changes that silence required diagnostics, relax the acceptance contract, or substitute an unverified implementation merely to obtain a pass. Keep policy, check invocation, and acceptance rules outside the candidate's writable scope when the harness can enforce that.

Read [checks-and-harness.md](references/checks-and-harness.md) when selecting command scope, configuring restrictions, or integrating the runner.

## 4. Run the executable checks

Discover and prefer supported project commands. For a conventional Cargo project, run the bundled helper from its installed skill directory:

```bash
python3 scripts/rust_checks.py --manifest /absolute/project/Cargo.toml --output /absolute/evidence/rust-checks
```

Use `--package NAME` for selected workspace packages, `--features LIST`, `--no-default-features`, `--release`, `--target TRIPLE`, `--offline`, and `--forbid-unsafe` only when appropriate to the intended build. Use `--timeout SECONDS` for each command's budget.

The helper records Cargo metadata, tool versions, a source fingerprint, commands, exit statuses, logs, and a lexical risk inventory. It builds the current program and runs Clippy with mandatory discarded-future, immediately dropped-lock, and unused-must-use diagnostics. It never runs unit tests. It never claims behavioral verification.

- Treat exit `0` as **static checks passed**, exit `1` as **failed**, and exit `2` as **blocked**. Read `report.json` and the relevant logs. A missing toolchain, missing dependency, or unknown required lint is not a pass.
- Treat inventory findings as advisory locations to inspect. The inventory is a text search, includes comments and strings, and can miss aliases, macros, external crates, and custom wrappers. An empty inventory does not establish safety.
- Confirm the chosen command scope matches the production targets and supported feature configurations. Build configuration matrices only where a relevant failure mode justifies them; do not blindly enable mutually exclusive features.
- For a project needing custom build steps, run the actual project commands and retain equivalent evidence. Do not force the conventional helper onto an incompatible workflow or treat a Cargo library build as exercising the application.

## 5. Exercise the behavior using real testing

Select scenarios from [behavioral-scenarios.md](references/behavioral-scenarios.md) that match the changed behavior. For each selected scenario:

1. Write the actions, expected observations, and bounded exploration budget before execution.
2. Build and launch the current program with real local dependencies and isolated data. Record the executable/build, target/features, configuration, endpoint, and actual actions.
3. Drive its supported CLI, API, UI, worker input, or public library API. Check downstream effects through a fresh reader; restart where persistence is promised.
4. Use controlled barriers, full queues, slow peers, worker failures, and reachable fault points where available. Prefer causal coordination to arbitrary sleeps. Do not claim cancellation testing unless the targeted operation was actually pending or mid-transition.
5. Generate valid inputs and legal action sequences with distinct identities, values, sizes, ordering, retries, and lifecycle transitions. Apply contract-grounded oracles or metamorphic relations. Record concrete sequences and the seed.
6. Confirm and minimize each failure through the real interface. Fix it, rebuild, and rerun the failing scenario and directly affected checks. Retain the concrete reproduction and clean-state replay command.

Use existing meaningful checks if required by project instructions. Do not generate unit tests, mocks, snapshot suites, coverage quotas, or Verus proofs to satisfy this workflow. A small real consumer or driver is appropriate when it exercises the actual implementation and has an independent oracle.

## 6. Report evidence and acceptance boundaries

Report static-check results separately from behavioral results. Use **PASS**, **FAIL**, **BLOCKED**, or **NOT RUN** for each required scenario. Include:

- Actual build and launch commands, toolchain, build identity, selected targets/features, and relevant configuration.
- The input/actions, independent expected result, observed result, and downstream readback.
- Generated exploration budget and actual executed case count, dimensions, and justified invariants.
- Preserved failure locations and replay commands, plus the final result after rebuilding.
- Unresolved risks and exact boundaries of unexercised behavior. Classify unsupported claims accordingly.

For Peritus, require the harness to compare this evidence against its independently held required scenario list and candidate identity. Require unresolved mandatory failures or blockers to prevent acceptance. A report file is evidence metadata, not an attestation: a trusted runner must validate the execution provenance and artifact identity.

Claim completion only for the final behavior actually exercised. Do not convert a static pass, clean scan, successful startup, or an agent's explanation into evidence that runtime requirements held.
