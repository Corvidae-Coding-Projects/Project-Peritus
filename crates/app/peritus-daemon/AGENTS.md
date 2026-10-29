# crates/app/peritus-daemon — navigation

## Responsibility

Production daemon and application composition owner for Peritus.

Package `peritus-daemon` is a member of the root Cargo workspace; architectural owner `G0` in `app`.

Local dependency waypoints: [peritus-agent](../../orchestration/peritus-agent/AGENTS.md), [peritus-app-protocol](../peritus-app-protocol/AGENTS.md), [peritus-approval](../../state/peritus-approval/AGENTS.md), [peritus-artifact-store](../../state/peritus-artifact-store/AGENTS.md), [peritus-codec](../../foundation/peritus-codec/AGENTS.md), [peritus-evidence](../../state/peritus-evidence/AGENTS.md), [peritus-debugger](../../analysis/peritus-debugger/AGENTS.md), [peritus-eval](../../analysis/peritus-eval/AGENTS.md), [peritus-evolution](../../analysis/peritus-evolution/AGENTS.md), [peritus-harness](../../orchestration/peritus-harness/AGENTS.md), [peritus-gates](../../orchestration/peritus-gates/AGENTS.md), [peritus-git](../../runtime/peritus-git/AGENTS.md), [peritus-journal](../../state/peritus-journal/AGENTS.md), [peritus-kernel](../../foundation/peritus-kernel/AGENTS.md), [peritus-leases](../../state/peritus-leases/AGENTS.md), [peritus-local-socket](../../runtime/peritus-local-socket/AGENTS.md), [peritus-migrations](../../state/peritus-migrations/AGENTS.md), [peritus-model-protocol](../../model/peritus-model-protocol/AGENTS.md), [peritus-policy](../../foundation/peritus-policy/AGENTS.md), [peritus-collaboration](../../orchestration/peritus-collaboration/AGENTS.md), [peritus-process](../../runtime/peritus-process/AGENTS.md), [peritus-product-runner](../peritus-product-runner/AGENTS.md), [peritus-projection](../../state/peritus-projection/AGENTS.md), [peritus-protocol](../../foundation/peritus-protocol/AGENTS.md), [peritus-provider-anthropic](../../model/peritus-provider-anthropic/AGENTS.md), [peritus-provider-compatible](../../model/peritus-provider-compatible/AGENTS.md), [peritus-provider-opencode](../../model/peritus-provider-opencode/AGENTS.md), [peritus-provider-core](../../model/peritus-provider-core/AGENTS.md), [peritus-provider-google](../../model/peritus-provider-google/AGENTS.md), [peritus-provider-openai](../../model/peritus-provider-openai/AGENTS.md), [peritus-orchestrator](../../orchestration/peritus-orchestrator/AGENTS.md), [peritus-patch](../../runtime/peritus-patch/AGENTS.md), [peritus-review](../../orchestration/peritus-review/AGENTS.md), [peritus-role](../../orchestration/peritus-role/AGENTS.md), [peritus-run-settlement](../../orchestration/peritus-run-settlement/AGENTS.md), [peritus-scheduler](../../orchestration/peritus-scheduler/AGENTS.md), [peritus-sandbox](../../runtime/peritus-sandbox/AGENTS.md), [peritus-secrets](../../runtime/peritus-secrets/AGENTS.md), [peritus-spec](../../foundation/peritus-spec/AGENTS.md), [peritus-telemetry](../../observe/peritus-telemetry/AGENTS.md), [peritus-tool-protocol](../../tools/peritus-tool-protocol/AGENTS.md), [peritus-tool-router](../../tools/peritus-tool-router/AGENTS.md), [peritus-tools-fs](../../tools/peritus-tools-fs/AGENTS.md), [peritus-tools-git](../../tools/peritus-tools-git/AGENTS.md), [peritus-tools-quality](../../tools/peritus-tools-quality/AGENTS.md), [peritus-tools-shell](../../tools/peritus-tools-shell/AGENTS.md), [peritus-trace](../../observe/peritus-trace/AGENTS.md), [peritus-types](../../foundation/peritus-types/AGENTS.md), [peritus-workspace](../../runtime/peritus-workspace/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-daemon`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Daemon entry/composition, local transport, authenticated command handlers, domain-service wiring, workspace admission, and product-run registry. |

## Nearby owners

- [Client/daemon request and event contracts](../peritus-app-protocol/AGENTS.md).
- [Coding-run role execution](../peritus-product-runner/AGENTS.md).
- [Bootstrap and packaged-daemon startup](../peritus-launcher/AGENTS.md).

## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
