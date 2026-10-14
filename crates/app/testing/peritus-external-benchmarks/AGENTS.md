# crates/app/testing/peritus-external-benchmarks — navigation

## Responsibility

Native HarnessBench and Terminal-Bench qualification boundary for Peritus.

Package `peritus-external-benchmarks` is a member of the root Cargo workspace; architectural owner `H3` in `testing`.

Local dependency waypoints: [peritus-model-protocol](../../../model/peritus-model-protocol/AGENTS.md), [peritus-daemon](../../peritus-daemon/AGENTS.md), [peritus-launcher](../../peritus-launcher/AGENTS.md), [peritus-product-runner](../../peritus-product-runner/AGENTS.md), [peritus-process](../../../runtime/peritus-process/AGENTS.md), [peritus-provider-anthropic](../../../model/peritus-provider-anthropic/AGENTS.md), [peritus-provider-core](../../../model/peritus-provider-core/AGENTS.md), [peritus-provider-openai](../../../model/peritus-provider-openai/AGENTS.md), [peritus-run-settlement](../../../orchestration/peritus-run-settlement/AGENTS.md), [peritus-types](../../../foundation/peritus-types/AGENTS.md).

## Files in this folder

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-external-benchmarks`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Native, noninteractive external-benchmark entry point for Peritus. |
| [tests/](tests/AGENTS.md) | Historical fixture organization retained beneath the active benchmark package; this subtree currently contains documentation rather than runnable test targets. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
