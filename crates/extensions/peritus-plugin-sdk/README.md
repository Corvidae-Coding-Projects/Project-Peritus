# peritus-plugin-sdk

## Folder guide

Versioned wire contracts and canonical manifests for isolated Peritus plugins.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-plugin-sdk`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Stable contracts for isolated Peritus plugins. |

Stable, serde-based contracts for Peritus plugins that execute outside the daemon address space.
The crate validates canonical manifests, bounded JSON payloads, protocol version negotiation, and
length-delimited request/result frames. Decoding a frame never grants authority; the plugin host
must bind every invocation to current daemon policy.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
