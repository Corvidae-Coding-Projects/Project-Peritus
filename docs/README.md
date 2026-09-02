# Peritus documentation

These guides explain how Peritus is built, how its parts fit together, and how to operate or qualify
the product. The letter-and-number names follow the architecture sequence in `DESIGN.md`.

## Foundations and authority

- [Toolchain policy](foundation-toolchain.md) records exact Rust, Verus, vstd, Z3, lockfile, and CI-input rules.
- [Formal foundation](formal-foundation.md) explains verified value types and the trusted-computing boundary.
- [Test and conformance foundation](test-conformance-foundation.md) defines deterministic fixtures and fresh-subject checks.
- [Application protocol](a3-app-protocol.md) defines versioned commands, events, sessions, artifacts, prompts, and terminals.
- [Protocol refinements](b3-protocol-refinements.md) records the formal refinements around the domain protocol.

## Runtime substrate

- [Durable state](c0-durable-state.md) covers the journal, projections, artifacts, migrations, and evidence.
- [Workspaces](c1-workspaces.md) covers Git worktrees, patches, snapshots, rollback, and restart repair.
- [Processes and sandboxes](c2-process-sandbox.md) covers structured execution, supervision, terminals, and recovery.
- [Platform security](c3-platform-security.md) covers native sandbox backends, network control, secrets, and teardown.
- [Tool system](c4-tool-system.md) covers schemas, authorization, routing, replay, and built-in tools.
- [Model providers](c5-model-providers.md) covers direct APIs, account-backed executable routes, streaming, and retry.
- [Context and memory](c6-context-memory.md) covers grounded context, compaction, token planning, and scoped memory.
- [Trace and telemetry](c7-trace-telemetry.md) covers durable observations, redaction, projections, and exporters.

## Agent and orchestration engines

- [Agent loop](d0-agent-loop.md) explains inner turns, tool calls, budgets, cancellation, and recovery.
- [Gate engine](d1-gate-engine.md) explains deterministic project checks and exact-revision evidence.
- [Review engine](d2-review-engine.md) explains independent review and finding conservation.
- [Scheduling and collaboration](d3-scheduler-collaboration.md) explains resource limits, task trees, and handoff.
- [AcTor orchestrator](e0-actor-orchestrator.md) composes writer, gates, reviewer, and fixer into delivery runs.
- [Harness materialization](e1-harness-materialization.md) covers component catalogs and immutable harness revisions.
- [Failure analysis](e2-debugger.md) covers evidence selection, timelines, causes, and pattern clustering.
- [Evaluation](e3-evaluation.md) covers isolated datasets, paired comparisons, statistics, and reports.
- [Harness evolution](f0-evolution.md) covers evidence-bound changes, human promotion, and rollback.

## Product surfaces

- [Daemon](g0-daemon.md), [recovery](g0-recovery-runbook.md), and [shutdown](g0-shutdown-runbook.md) cover the durable service.
- [CLI](g1-cli.md) documents the scriptable command surface and exit behavior.
- [TUI](g2-tui.md) documents interactive state, controls, reconnection, and terminal handling.
- [Extensions](g3-extensions.md) documents plugins, isolated hosts, authority mediation, and MCP.
- [Product experience](g4-product-experience.md) covers one-command startup, providers, workspaces, runs, and handoff.

## Production qualification

- [Security qualification](h0-security-qualification.md) defines the security campaign and verdict.
- [Resilience qualification](h1-resilience-qualification.md) defines disruption, recovery, and false-success checks.
- [Platform qualification](h2-platform-qualification.md) covers Linux, macOS, and Windows packages and services.
- [Performance qualification](h3-performance-qualification.md) covers load, SLOs, resource accounting, and soak tests.
- [Release policy](h4-release-policy.md) defines the exact readiness decision.
- [Release qualification](h4-release-qualification.md) covers artifacts, provenance, reproducibility, and audit.
- [Migration and recovery](release-migration-recovery.md) is the release operator runbook.
- [Professional harness capability audit](professional-harness-capability-audit.md) tracks expected production features and gaps.
- [Benchmark integrity appendix](benchmark-integrity-appendix.md) records retained benchmark gotchas and the score-only shortcuts Peritus refuses.
- [GitHub governance](github-governance.md) defines branch, ruleset, and required-check expectations.

## Documentation check

From the repository root:

```sh
cargo xtask docs-check
```

The check inventories maintained Markdown, validates basic structure and local links, and verifies
that each crate README includes its focused package command.
