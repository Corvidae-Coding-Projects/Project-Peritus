# Peritus navigation

## Responsibility

Peritus is a local coding-agent application: clients request work, the daemon owns authority and execution, and role/provider/tool adapters perform the work.

## Start with the question

| Question | Entry points |
| --- | --- |
| Launch, install, provider setup, daemon availability | [Launcher](crates/app/peritus-launcher/AGENTS.md), [CLI](crates/app/peritus-cli/AGENTS.md), [onboarding](crates/app/peritus-provider-onboarding/AGENTS.md), [packaging](packaging/AGENTS.md) |
| Active coding run, steering, retries, candidates | [Daemon product runs](crates/app/peritus-daemon/src/product_run/AGENTS.md), [product runner](crates/app/peritus-product-runner/AGENTS.md), [developer loop](crates/orchestration/peritus-agent/src/developer/AGENTS.md) |
| Guided design interview, saved Markdown, implementation handoff | [Conversation tools](crates/app/peritus-product-runner/src/execution/conversation/AGENTS.md), [runner source](crates/app/peritus-product-runner/src/AGENTS.md), [feature design](.design/interactive-design-mode.md) |
| Bundled/installable workflows and module role stages | [Module domain](crates/orchestration/peritus-modules/AGENTS.md), [execution](crates/app/peritus-product-runner/src/execution/modules.rs) |
| Terminal/browser interaction | [TUI](crates/app/peritus-tui/AGENTS.md), [browser client](webui/AGENTS.md), [gateway](crates/app/peritus-web/AGENTS.md) |
| Model routing and provider failures | [Model layer](crates/model/AGENTS.md) |
| Commands, files, browser/desktop operations | [Tool layer](crates/tools/AGENTS.md), [native runtime](crates/runtime/AGENTS.md), [product tool executor](crates/app/peritus-product-runner/src/developer_tools/AGENTS.md) |
| Conversation size, working memory, prompt/skill delivery | [Local context](crates/app/peritus-product-runner/src/local_context/AGENTS.md), [role guidance](crates/app/peritus-product-runner/src/engineering_workflow/AGENTS.md), [context domain](crates/orchestration/peritus-context/AGENTS.md) |
| Persistence/recovery, authority, trace evidence | [State](crates/state/AGENTS.md), [foundation](crates/foundation/AGENTS.md), [observability](crates/observe/AGENTS.md) |
| Improvement inbox, candidate evaluation/promotion | [Daemon improvements](crates/app/peritus-daemon/src/product_run/improvements/AGENTS.md), [evolution](crates/analysis/peritus-evolution/AGENTS.md), [evaluation](crates/analysis/peritus-eval/AGENTS.md) |

The human coding path is CLI/launcher or UI → daemon `ProductRunService` → `ProductRunner` → `DeveloperLoop` → provider and host-owned tools. Journal-backed domain `AgentDriver`/`OrchestratorDriver` paths are related machinery with separate state ownership; do not conflate them with the interactive product coordinator.

These maps explain ownership and navigation. Read the linked implementation and its callers for exact behavior. Current Cargo membership identifies active packages; archived evidence and README-only retired components describe history. User instructions determine work scope; the maps introduce no testing or approval gates.

## Files in this folder

| File | Role |
| --- | --- |
| [.gitattributes](.gitattributes) | Git text/binary handling declarations for checked-in files. |
| [.gitignore](.gitignore) | Ignored build output, dependency caches, local integrations, and reference checkout paths. |
| [.mcp.json](.mcp.json) | Local MCP server/integration configuration. |
| [CHANGELOG.md](CHANGELOG.md) | Human release history. |
| [Cargo.lock](Cargo.lock) | Exact resolved Rust dependency versions; use with the root workspace manifest. |
| [Cargo.toml](Cargo.toml) | Root workspace membership, dependency/toolchain/lint declarations, and shared package metadata. |
| [DESIGN.md](DESIGN.md) | Product/UI design intent and visual-system decisions. |
| [LICENSE](LICENSE) | Project license text. |
| [PRODUCT.md](PRODUCT.md) | Product requirements and experience-level scope. |
| [README.md](README.md) | Human product overview, installation/use entrypoints, and the project real-testing policy. |
| [architecture.toml](architecture.toml) | Layer/package ownership and dependency direction declarations, with source-layout and controlled-root metadata. |
| [deny.toml](deny.toml) | Dependency license/advisory policy configuration. |
| [install.ps1](install.ps1) | Published PowerShell bootstrap installer for native Peritus packages. |
| [install.sh](install.sh) | Published Unix bootstrap installer for complete native Peritus packages. |
| [justfile](justfile) | Developer command recipes; inspect the referenced current implementations before selecting a recipe. |
| [rust-toolchain.toml](rust-toolchain.toml) | Pinned Rust compiler/toolchain selection for this checkout. |
| [rustfmt.toml](rustfmt.toml) | Rust source-format configuration. |
| [toolchains.toml](toolchains.toml) | Repository toolchain contract metadata. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [.cargo/](.cargo/AGENTS.md) | Cargo configuration for workspace builds and tool routing. |
| [.chainlink/](.chainlink/AGENTS.md) | Retained local issue/workflow configuration, rule documents, and tracking data. |
| [.claude/](.claude/AGENTS.md) | Local Claude integration configuration, hooks, and MCP utility code. |
| [.design/](.design/AGENTS.md) | Feature designs and investigation records. Designs explain intent; current callers/manifests establish what is implemented. |
| [.github/](.github/AGENTS.md) | GitHub workflow/action definitions. Configuration files describe workflows; live runner/enablement state is external to the repository. |
| [.impeccable/](.impeccable/AGENTS.md) | Retained visual/design-system guidance used by the browser UI work. |
| [app-protocol/](app-protocol/AGENTS.md) | Checked-in application-protocol schema/projection assets. The Rust contract owner is peritus-app-protocol. |
| [benchmarks/](benchmarks/AGENTS.md) | Benchmark profiles, baseline records, and external-suite integration notes. These are separate from routine real-testing workflows. |
| [compat/](compat/AGENTS.md) | Versioned protocol compatibility data. Surface/version/case directories pair manifests, input bytes, and expected decoder outcomes. |
| [crates/](crates/AGENTS.md) | Rust implementation grouped by architectural responsibility. Choose a layer by the behavior you need, then follow its package and module maps. |
| [docs/](docs/AGENTS.md) | Human architecture, contracts, operations, and recorded evidence. Current interface guides and historical proof/qualification archives have different roles. |
| [fixtures/](fixtures/AGENTS.md) | Checked-in runtime/configuration examples and recovery inputs. Keep fixture data distinct from executable owners and live user state. |
| [packaging/](packaging/AGENTS.md) | Install/distribution assets: native package recipes, launch/service assets, notices, manual pages, and platform entrypoints. |
| [protocol/](protocol/AGENTS.md) | Canonical protocol registry and migration/version assets shared by domain and application wire contracts. |
| [release/](release/AGENTS.md) | Release contract/configuration assets and candidate-specific publication records. |
| [resilience/](resilience/AGENTS.md) | Resilience contract/configuration assets retained alongside the runtime recovery implementations. |
| [security/](security/AGENTS.md) | Security policy/assessment assets and retained dependency findings; distinguish documents from runtime security enforcement. |
| [skills/](skills/AGENTS.md) | Bundled model guidance packages. These supply instructions/resources; execution and permissions remain in the host. |
| [webui/](webui/AGENTS.md) | Svelte browser client and its build/static assets. The Rust gateway is peritus-web; the daemon owns agent execution. |
| [xtask/](xtask/AGENTS.md) | Repository packaging and release command owner. Read the current command dispatcher rather than old qualification instructions. |

## Working in this area

Use the question-to-entry-point table first, then descend through the nearest child waypoint before reading implementation details. Confirm active packages in the root manifests and trace current callers; designs, fixtures, and retained evidence describe intent or history rather than automatically proving live behavior.

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
