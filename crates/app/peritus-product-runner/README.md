# peritus-product-runner

Daemon-owned production composition for interactive Peritus coding runs. Every run begins with a
read-only, repository-grounded design pass that writes a durable detailed Markdown document and
supplies it to all implementation turns. Grounding is enforced by observed workspace listings and
targeted reads, not accepted from model prose. The adapter then joins:

- embedded production-engineering, architect, developer, and reviewer workflow skills;
- the `peritus-agent` D0 provider/tool loop for real inspect, search, edit, command, test, and retry,
  with unread existing files protected from mutation and recoverable malformed, transport, and
  timeout terminals retried as fresh bounded provider attempts;
- `peritus-gates` changed-path planning, the deterministic 500-line source ceiling, exact
  per-project format/compile/build/test/lint evidence, and native structural checks for changed
  CSV, JSON, YAML, and conventional SQLite migration artifacts;
- `peritus-review` typed policy-derived findings conserved through fixer and fresh-review cycles;
- `peritus-orchestrator` fail-closed E0 accept/fix/exhaust decisions; and
- a durable task candidate, provider/tool trace, task-level summary, and explicit deliverable
  handoff consumed by the daemon.

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

Run lifetime is deliberately separate from model-turn lifetime. One provider attempt remains
wall-clock bounded and one developer segment remains bounded to 48 logical turns and 512 tool
calls. If that segment changed the exact Git candidate, its content checkpoint starts another
fresh, repository-grounded segment with a compact prompt. Therefore substantial work can continue
for as many segments as it needs without retaining an ever-growing model context. A segment that
exhausts its allowance without changing the candidate stops as no-progress, and three consecutive
malformed or ungrounded task-level terminals receive the exact rejection as corrective context;
three consecutive failures stop for user correction. Abrupt daemon restarts automatically
resume interrupted goals from their persisted conversation, finding ledger, trace, and unchanged
managed worktree.

Provider, filesystem, process, and Git effects remain ordinary Rust host adapters. In
`verus_only` builds the crate exposes the same daemon-facing boundary as a fail-closed total
implementation that cannot fabricate completion; the D0, D1, D2, and E0 decision cores remain in
their formally classified crates and the ordinary-API audit constrains the production adapter.

Focused qualification:

```text
CARGO_BUILD_JOBS=2 cargo test -p peritus-product-runner --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-product-runner --all-targets --all-features --locked -- -D warnings
```
