# peritus-product-runner

Daemon-owned production composition for interactive Peritus coding runs. Every run begins with a
read-only, repository-grounded design pass that writes a durable detailed Markdown document and
supplies it to all implementation turns. Source repositories use an architect model whose
grounding is enforced by observed workspace listings and targeted reads, not accepted from model
prose. Explicit `kind = "artifact"` workspaces use a proportional deterministic Rust design built
from the exact durable conversation and a bounded sorted filesystem inventory, so a small
time-bound artifact task does not spend most of its deadline generating repetitive planning prose.
Both paths cover acceptance, findings, architecture, data flow, file plan, implementation slices,
verification, and explicit risks or non-goals. The adapter then joins:

- embedded production-engineering, architect, developer, and reviewer workflow skills;
- the `peritus-agent` D0 provider/tool loop for real inspect, search, edit, command, test, and retry,
  with unread existing files protected from mutation and recoverable malformed, transport, and
  timeout terminals retried as fresh bounded provider attempts;
- `peritus-gates` changed-path planning, the deterministic 500-line source ceiling, exact
  per-project format/compile/build/test/lint evidence, and native structural checks for changed
  CSV, JSON, YAML, and conventional SQLite migration artifacts;
- `peritus-review` typed policy-derived findings conserved through fixer and fresh-review cycles;
- `peritus-orchestrator` fail-closed E0 accept/fix/exhaust decisions; and
- a durable task candidate, provider/tool trace, synced effect-receipt ledger, task-level summary,
  and explicit deliverable handoff consumed by the daemon.

Developer commands use the same command path as the rest of Peritus. `run_command` starts a
structured command through C4 and the daemon-owned C2 process store, waits for it, and returns a
bounded terminal result. Programs that need interaction or background progress use
`command_start`; the model can then poll the stable run-owned handle, write bounded terminal input,
resize the terminal, send a supported signal, cancel the owned process tree, or reconcile an
interrupted observation. Output is retained as C2/C4 artifacts and projected with both its opening
context and final diagnostics when it is too large. A terminal active result enters the same
review, acceptance, handoff, and command-created-file ownership ledgers as a finite command. The
product runner does not contain a second PTY implementation or call `std::process::Command` for
developer tools.

An active handle belongs to the live product run. `command_recover` reconciles interrupted polling
or control with the existing C4/C2 owner; it does not promise to recreate an arbitrary terminal
after a daemon process restart. Startup recovery separately reconciles durable C2 process state and
the durable product run remains explicitly recoverable.

When the launcher supplied explicit automatic-failover consent, every designer, writer, reviewer,
and fixer invocation owns a deterministic provider cursor. The selected provider keeps its normal
bounded recovery first. Only then may the role advance to another configured tool-capable route;
media capability is checked against the current task. Safety, refusal, cancellation, raw ambiguous
transport, and normalized ambiguous acceptance never trigger a switch. Each accepted transition is
written to the append-only trace before its progress counter advances.

Writable tool receipts bind deterministic role/invocation/effect identity, provider call ID, and
canonical request digest to `Started`, `Completed`, or `Ambiguous` state. Exact completed calls
replay their bounded result. A command left in `Started` across restart is never launched again;
Peritus returns an explicit ambiguous observation so the agent or user can reconcile its effects.

The first workspace listing also grounds command execution in the current runtime rather than the
host's apparent CPU count. It reports logical and effective CPUs, the effective memory ceiling,
and a conservative recommended parallelism. Linux observations include cgroup-v2 limits. Command
execution applies that ceiling to common build ecosystems and returns a retryable error before
recognized Cargo, CMake, Make, or Ninja arguments can request a larger worker fan-out.

Independent review is a fresh D0 developer loop rather than a one-shot completion. It begins with
an observed workspace listing, reads the authoritative inputs and changed files through a
read-only tool executor, and only then admits the typed review. The executor rejects write, patch,
remove, and process calls even if a provider emits an undeclared tool name, so reviewer grounding
does not grant mutation authority. Malformed or ungrounded reviews receive their exact rejection
on a fresh bounded attempt.

The crate consumes already resolved provider and managed-workspace capabilities. It emits bounded
progress and deliverable evidence; it does not own UI, provider login, workspace trust, or Git
commit/export/discard authority. `Complete` is impossible for an empty or uncovered candidate, a
failed or missing exact-target command, or any unresolved policy blocker.

Run lifetime is deliberately separate from model-turn lifetime. The caller supplies the real
wall-clock horizon, up to the product's eight-hour hard ceiling. Every architect, writer, reviewer,
and fixer invocation receives the remaining shared time so it can do substantial work without
spending the complete window on open-ended exploration or optimization. At the horizon, Peritus
signals the shared provider cancellation token, gives the active operation a short settlement
period, and returns a typed budget failure that the caller can persist and report.

One provider attempt remains wall-clock bounded and one developer segment remains bounded to 48
logical turns and 512 tool calls. If that segment changed the exact Git candidate, its content
checkpoint starts another fresh, repository-grounded segment with a compact prompt. Therefore
substantial work can continue for as many segments as it needs without retaining an ever-growing
model context. A segment that exhausts its allowance without changing the candidate stops as
no-progress, and three consecutive malformed or ungrounded task-level terminals receive the exact
rejection as corrective context; three consecutive failures stop for user correction. Abrupt daemon
restarts automatically resume interrupted goals from their persisted conversation, finding ledger,
trace, and unchanged managed worktree.

At every completed effect boundary, the runner also measures regular-file bytes beneath the
managed workspace and the harness process's resident memory through the host's ordinary process
accounting interface. Git object storage is excluded because it is repository history rather than
task growth; generated build trees remain included. The daemon persists and displays current
workspace size, positive growth from the run baseline, and the highest observed resident memory.
Generous 50 GiB growth and 12 GiB observed-memory ceilings fail with the same distinct budget
category as token, request, tool, cost, and elapsed-time overruns.

Provider, filesystem, process, and Git effects remain ordinary Rust host adapters. In
`verus_only` builds the crate exposes the same daemon-facing boundary as a fail-closed total
implementation that cannot fabricate completion; the D0, D1, D2, and E0 decision cores remain in
their formally classified crates and the ordinary-API audit constrains the production adapter.

Focused qualification:

```text
CARGO_BUILD_JOBS=2 cargo test -p peritus-product-runner --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-product-runner --all-targets --all-features --locked -- -D warnings
```

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-product-runner
```
