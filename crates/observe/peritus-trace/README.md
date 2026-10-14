# peritus-trace

## Folder guide

Durable causal tracing and redaction boundary for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-trace`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable causal tracing and redaction boundary for Peritus. |

`peritus-trace` is C7's durable, causal, redaction-safe observation boundary. It encodes inert
family-60 trace observations, commits them through C0, rebuilds pure projections from checked
journal history, and exposes only closed diagnostics and encrypted artifact-vault references.

It deliberately has no execution, policy, approval, budget, provider transport, tool dispatch, or
gate-decision capability. See `docs/c7-trace-telemetry.md` for the integration and recovery contract.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
