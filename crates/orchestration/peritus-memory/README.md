# peritus-memory

## Folder guide

scoped derived-memory lifecycle and retrieval for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-memory`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Scoped derived-memory lifecycle and retrieval for Peritus. |

`peritus-memory` manages durable facts that agents may reuse in later turns. It records where each
fact came from, which workspace and role may see it, and whether the fact is still a candidate,
approved, quarantined, or deleted. Retrieval is deterministic, and its search indexes can be
rebuilt from the authoritative records.

The orchestration layer uses this crate when it assembles grounded context for an agent. Memory is
derived context, not project authority: it cannot replace repository contents, user instructions,
policy decisions, or fresh tool evidence. The crate also performs no ambient file or network I/O
and must not store secrets.

Lifecycle and retrieval logic live in separate modules so new ranking or retention policies can be
added without changing the durable record format.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
