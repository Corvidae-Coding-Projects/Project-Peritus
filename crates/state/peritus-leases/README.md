# peritus-leases

## Folder guide

generation-bound exclusive mutation leases for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-leases`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Generation-bound exclusive mutation leases for Peritus. |

`peritus-leases` is the pure state and observation-contract boundary for Peritus mutation leases.
It binds one workspace lineage to one resolved resource and environment, tracks exact actor/session
ownership, and fences every stale generation before another holder can acquire.

The crate returns logical plans and unprivileged claims only. Its compare-and-swap observations do
not prove persistence, and none of its values is an effect permit. C0 owns durable commit receipts;
the target C1/C2/C4 authorization gateway owns the private permit that can reach an effect.

Authority time is epoch-bound and monotonic. An epoch change or regression is accepted only by an
explicit fencing/reconciliation path. Expiry alone never establishes holder quiescence.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
