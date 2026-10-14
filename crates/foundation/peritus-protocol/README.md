# peritus-protocol

## Folder guide

Stable canonical domain protocol for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-protocol`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Stable canonical domain protocol for Peritus. |

`peritus-protocol` defines Peritus's stable, versioned domain wire messages. It maps canonical
frames to checked domain values without treating decoded bytes as authorization, evidence, or a
durable commit receipt.

Schema version 1 covers lifecycle commands, command envelopes, immutable event records, reducer
errors, lifecycle phases, B1 policy and budget data, and B2 acceptance contracts.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
