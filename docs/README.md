# Peritus documentation

## Folder guide

Human architecture, contracts, operations, and recorded evidence. Current interface guides and historical proof/qualification archives have different roles.

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |
| [a3-app-protocol.md](a3-app-protocol.md) | Document: A3 application protocol guide. |
| [b3-protocol-refinements.md](b3-protocol-refinements.md) | Document: B3 protocol refinement discharge. |
| [benchmark-integrity-appendix.md](benchmark-integrity-appendix.md) | Document: Benchmark integrity appendix. |
| [c0-durable-state.md](c0-durable-state.md) | Document: C0 durable state. |
| [c1-workspaces.md](c1-workspaces.md) | Document: C1 Git, workspace, and patch boundary. |
| [c2-process-sandbox.md](c2-process-sandbox.md) | Document: C2 process and sandbox backplane. |
| [c3-platform-security.md](c3-platform-security.md) | Document: C3 platform security backends. |
| [c4-tool-system.md](c4-tool-system.md) | Document: C4 tool system. |
| [c5-model-providers.md](c5-model-providers.md) | Document: C5 model providers. |
| [c6-context-memory.md](c6-context-memory.md) | Document: C6 context and memory. |
| [c7-trace-telemetry.md](c7-trace-telemetry.md) | Document: C7 trace and telemetry operations. |
| [computer-automation.md](computer-automation.md) | Document: Computer automation. |
| [d0-agent-loop.md](d0-agent-loop.md) | Document: D0 agent loop. |
| [d1-gate-engine.md](d1-gate-engine.md) | Document: D1 gate engine. |
| [d2-review-engine.md](d2-review-engine.md) | Document: D2 review engine. |
| [d3-scheduler-collaboration.md](d3-scheduler-collaboration.md) | Document: D3 scheduler and collaboration. |
| [daily-driving-limiters-audit.md](daily-driving-limiters-audit.md) | Document: Daily-driving limiter findings. |
| [e0-actor-orchestrator.md](e0-actor-orchestrator.md) | Document: E0 AcTor delivery orchestrator. |
| [e1-harness-materialization.md](e1-harness-materialization.md) | Document: E1 harness materialization. |
| [e2-debugger.md](e2-debugger.md) | Document: E2 debugger. |
| [e3-evaluation.md](e3-evaluation.md) | Document: E3 evaluation. |
| [f0-evolution.md](f0-evolution.md) | Document: F0 production harness evolution. |
| [file-size-limit-repairs.md](file-size-limit-repairs.md) | Document: File, attachment, artifact, and workspace size limits. |
| [formal-coverage-audit.md](formal-coverage-audit.md) | Document: Production formal verification work record. |
| [formal-coverage-checkpoint.md](formal-coverage-checkpoint.md) | Document: Formal verification stabilization checkpoint. |
| [formal-coverage-inventory.json](formal-coverage-inventory.json) | Structured formal coverage inventory data with fields `current_source_sha256`, `limits`, `packages`, `registers`, `schema`. |
| [formal-foundation.md](formal-foundation.md) | Document: Formal foundation. |
| [formal-governance-ruleset.template.json](formal-governance-ruleset.template.json) | Structured formal governance ruleset.template data with fields `name`, `target`, `enforcement`, `bypass_actors`, `conditions`. |
| [formal-method-mapping-fix.md](formal-method-mapping-fix.md) | Document: Proof-scope method ownership repair. |
| [formal-proof-impact-reconciliation.md](formal-proof-impact-reconciliation.md) | Document: Proof-impact reconciliation checkpoint and qualification procedure. |
| [formal-scheduler-gap3.md](formal-scheduler-gap3.md) | Document: GAP-03 scheduler production proofs. |
| [foundation-toolchain.md](foundation-toolchain.md) | Document: Foundation toolchain policy. |
| [g0-daemon.md](g0-daemon.md) | Document: G0 daemon and application composition. |
| [g0-recovery-runbook.md](g0-recovery-runbook.md) | Document: G0 recovery runbook. |
| [g0-shutdown-runbook.md](g0-shutdown-runbook.md) | Document: G0 shutdown runbook. |
| [g1-cli.md](g1-cli.md) | Document: G1 Scriptable CLI. |
| [g2-tui.md](g2-tui.md) | Document: G2 Interactive TUI. |
| [g3-extensions.md](g3-extensions.md) | Document: G3 Extensions and MCP. |
| [g4-product-experience.md](g4-product-experience.md) | Document: G4 product experience. |
| [github-governance.md](github-governance.md) | Document: GitHub Gate A governance. |
| [h0-security-qualification.md](h0-security-qualification.md) | Document: H0 security qualification. |
| [h1-resilience-qualification.md](h1-resilience-qualification.md) | Document: H1 resilience qualification. |
| [h2-platform-qualification.md](h2-platform-qualification.md) | Document: H2 platform qualification and packaging. |
| [h3-performance-qualification.md](h3-performance-qualification.md) | Document: H3 performance qualification. |
| [h4-release-policy.md](h4-release-policy.md) | Document: H4 verified release policy. |
| [h4-release-qualification.md](h4-release-qualification.md) | Document: H4 release qualification. |
| [harness-improvements.md](harness-improvements.md) | Document: Harness improvement inbox. |
| [local-working-memory.md](local-working-memory.md) | Document: Local working memory. |
| [minimum-repair.md](minimum-repair.md) | Document: Minimum repair. |
| [navigation-map.md](navigation-map.md) | Navigation documentation scope, directory coverage, maintenance guidance, and complete waypoint index. |
| [openai-codex-runtime-bridge-repair.md](openai-codex-runtime-bridge-repair.md) | Document: OpenAI Codex runtime bridge repair. |
| [professional-harness-capability-audit.md](professional-harness-capability-audit.md) | Document: Professional coding-harness capability audit. |
| [provider-contracts.md](provider-contracts.md) | Document: Hosted provider contracts. |
| [release-migration-recovery.md](release-migration-recovery.md) | Document: Release migration, backup, restore, and rollback. |
| [rsi-native-operations.md](rsi-native-operations.md) | Document: Native candidate evaluation and evolution. |
| [slash-command-qa-2026-09-08.md](slash-command-qa-2026-09-08.md) | Document: Interactive slash-command audit — 2026-09-08. |
| [test-conformance-foundation.md](test-conformance-foundation.md) | Document: Test and conformance foundation. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [formal-coverage-evidence/](formal-coverage-evidence/AGENTS.md) | Historical formal/qualification evidence archive. Files record earlier runs and reviews; they are not a current testing mandate or proof of this checkout. |
| [formal-scheduler-gap3/](formal-scheduler-gap3/AGENTS.md) | Historical scheduler design/analysis and retained GAP-03 evidence; use current scheduler source for executable behavior. |
| [releases/](releases/AGENTS.md) | Release-specific human notes; each version records changes and delivery context, rather than current runtime state. |
| [testing/](testing/AGENTS.md) | Real-interface campaign reports and reproducible defects. These document observed behavior and its limits; they are not runtime modules. |

## Find the right guide

| Work area | Human guide | Current implementation map |
| --- | --- | --- |
| Startup, provider setup, workspaces, and coding runs | [Product experience](g4-product-experience.md), [provider contracts](provider-contracts.md) | [Launcher](../crates/app/peritus-launcher/AGENTS.md), [onboarding](../crates/app/peritus-provider-onboarding/AGENTS.md), [product runner](../crates/app/peritus-product-runner/AGENTS.md) |
| Terminal/browser controls and reconnection | [CLI](g1-cli.md), [TUI](g2-tui.md), [WebUI](../webui/README.md) | [CLI](../crates/app/peritus-cli/AGENTS.md), [TUI](../crates/app/peritus-tui/AGENTS.md), [gateway](../crates/app/peritus-web/AGENTS.md) |
| Daemon lifecycle, shutdown, and recovery | [Daemon](g0-daemon.md), [recovery](g0-recovery-runbook.md), [shutdown](g0-shutdown-runbook.md) | [Daemon](../crates/app/peritus-daemon/AGENTS.md), [durable product runs](../crates/app/peritus-daemon/src/product_run/AGENTS.md) |
| Request/response protocols | [Application protocol](a3-app-protocol.md), [domain protocol](b3-protocol-refinements.md) | [Application wire](../crates/app/peritus-app-protocol/AGENTS.md), [domain wire](../crates/foundation/peritus-protocol/AGENTS.md), [compatibility inputs](../compat/AGENTS.md) |
| Provider turns, prompts, skills, and long-run context | [Model providers](c5-model-providers.md), [agent loop](d0-agent-loop.md), [local working memory](local-working-memory.md), [context/memory](c6-context-memory.md) | [Providers](../crates/model/AGENTS.md), [developer loop](../crates/orchestration/peritus-agent/src/developer/AGENTS.md), [local context](../crates/app/peritus-product-runner/src/local_context/AGENTS.md), [role guidance](../crates/app/peritus-product-runner/src/engineering_workflow/AGENTS.md) |
| Owned effects and authority | [Workspaces](c1-workspaces.md), [processes](c2-process-sandbox.md), [platform security](c3-platform-security.md), [tools](c4-tool-system.md) | [Runtime](../crates/runtime/AGENTS.md), [tools](../crates/tools/AGENTS.md), [foundation](../crates/foundation/AGENTS.md) |
| Durable state and observations | [Durable state](c0-durable-state.md), [trace/telemetry](c7-trace-telemetry.md) | [State](../crates/state/AGENTS.md), [observability](../crates/observe/AGENTS.md) |
| Candidate review and coordinated runs | [Review](d2-review-engine.md), [scheduling/collaboration](d3-scheduler-collaboration.md), [orchestrator](e0-actor-orchestrator.md) | [Orchestration](../crates/orchestration/AGENTS.md), [interactive product coordinator](../crates/app/peritus-product-runner/AGENTS.md) |
| Harness construction, evaluation, and improvement | [Materialization](e1-harness-materialization.md), [failure analysis](e2-debugger.md), [evaluation](e3-evaluation.md), [evolution](f0-evolution.md) | [Harness](../crates/orchestration/peritus-harness/AGENTS.md), [analysis](../crates/analysis/AGENTS.md), [improvement inbox](../crates/app/peritus-daemon/src/product_run/improvements/AGENTS.md) |
| Plugins and MCP | [Extensions](g3-extensions.md) | [Extensions layer](../crates/extensions/AGENTS.md) |
| Native installation and releases | [Packaging](../packaging/README.md), [native compilation](../packaging/native-build.md), [release/migration background](release-migration-recovery.md) | [Packaging](../packaging/AGENTS.md), [xtask](../xtask/README.md), [release assets](../release/AGENTS.md) |
| Real behavioral evidence and known defects | [Campaign/evidence area](testing/AGENTS.md), [retained replay resources](testing/reproducers/README.md) | [Project real-testing policy](../README.md#real-testing) |

The letter-and-number documents preserve architectural intent and earlier implementation notes.
The linked source maps identify current owners. Follow the relevant caller and manifest when a
historical guide differs from this checkout; a document title does not establish current behavior.

## Historical design and evidence

[Formal foundation](formal-foundation.md), [formal coverage evidence](formal-coverage-evidence/AGENTS.md),
[toolchain foundation](foundation-toolchain.md), [conformance foundation](test-conformance-foundation.md),
and the `h0`–`h4` qualification/release documents preserve earlier design and campaign context.
[GitHub governance](github-governance.md) describes earlier enforcement arrangements; workflow files
and live GitHub settings establish current hosted behavior. These records are retained for context,
not as instructions to restore retired external unit-test or formal-verification tooling.

The application still owns its candidate checks, independent review, recovery, and evidence handling.
Those runtime features are described under the product and orchestration routes above.

## Navigating and maintaining documentation

Start with [AGENTS.md](AGENTS.md) for local file roles, or [navigation-map.md](navigation-map.md)
for the complete recursive index. Each source directory links to its parent and owning package.
When files move, update their local waypoint, parent child-area table, and any README links.
The former `cargo xtask docs-check` command is retired; it is not a current documentation command.
