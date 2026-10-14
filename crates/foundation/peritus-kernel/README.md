# peritus-kernel

## Folder guide

lifecycle state machines and authoritative event reduction for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-kernel`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Lifecycle state machines and authoritative event reduction for Peritus. |

`peritus-kernel` is Peritus's effect-free lifecycle authority core. It owns the session, run,
attempt, turn, action, review, waiver, and acceptance state machines and emits logical event plans
for accepted commands.

The crate consumes exact-revision policy, budget, and acceptance facts from the B1/B2 verified
crates. It performs no persistence, I/O, clock reads, process execution, or durable authorization.
Commands are requests; only checked witnesses can advance authority-bearing phases.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
