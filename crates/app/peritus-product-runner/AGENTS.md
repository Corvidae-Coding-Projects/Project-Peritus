# crates/app/peritus-product-runner — navigation

## Responsibility

Daemon-owned writer-reviewer-fixer product run coordinator for Peritus.

Package `peritus-product-runner` is a member of the root Cargo workspace; architectural owner `G4` in `app`.

Local dependency waypoints: [peritus-harness](../../orchestration/peritus-harness/AGENTS.md), [peritus-tools-computer](../../tools/peritus-tools-computer/AGENTS.md), [peritus-agent](../../orchestration/peritus-agent/AGENTS.md), [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-budget](../../foundation/peritus-budget/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-context](../../orchestration/peritus-context/AGENTS.md), [peritus-gates](../../orchestration/peritus-gates/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-kernel](../../foundation/peritus-kernel/AGENTS.md), [peritus-leases](../../state/peritus-leases/AGENTS.md), [peritus-model-protocol](../../model/peritus-model-protocol/AGENTS.md), [peritus-orchestrator](../../orchestration/peritus-orchestrator/AGENTS.md), [peritus-obligations](../../orchestration/peritus-obligations/AGENTS.md), [peritus-patch](../../runtime/peritus-patch/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-process](../../runtime/peritus-process/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-provider-core](../../model/peritus-provider-core/AGENTS.md), [peritus-review](../../orchestration/peritus-review/AGENTS.md), [peritus-role](../../orchestration/peritus-role/AGENTS.md), [peritus-run-knowledge](../../orchestration/peritus-run-knowledge/AGENTS.md), [peritus-run-settlement](../../orchestration/peritus-run-settlement/AGENTS.md), [peritus-sandbox](../../runtime/peritus-sandbox/AGENTS.md), [peritus-spec](../../foundation/peritus-spec/AGENTS.md), [peritus-tool-protocol](../../tools/peritus-tool-protocol/AGENTS.md), [peritus-tool-router](../../tools/peritus-tool-router/AGENTS.md), [peritus-tools-shell](../../tools/peritus-tools-shell/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md), [peritus-workspace](../../runtime/peritus-workspace/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [build.rs](build.rs) | Discovers canonical root skill packages and generates their immutable resource registry. |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-product-runner`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [examples/](examples/AGENTS.md) | Direct native consumers of ProductRunner, including credentialed disposable-workspace examples; read each entrypoint before running it. |
| [src/](src/AGENTS.md) | The product execution coordinator: design, writer/reviewer/fixer turns, authorized tools, candidate/gate handling, local context, and final settlement. |

Canonical bundled guidance: [repository skills](../../../skills/AGENTS.md).

## Nearby owners

- [Daemon admission and durable run controls](../peritus-daemon/src/product_run/AGENTS.md).
- [Provider/tool turn loop](../../orchestration/peritus-agent/src/developer/AGENTS.md).
- [Typed findings and review lifecycle](../../orchestration/peritus-review/AGENTS.md).
- [Real operation reports and replays](../../../docs/testing/AGENTS.md).

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
