# peritus-approval

## Folder guide

digest-bound human approval transitions for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-approval`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Digest-bound human approval transitions for Peritus. |

`peritus-approval` owns Peritus's pure, digest-bound human approval state machine. It validates
canonical request and credential inputs, authenticates Ed25519 decisions through one audited
verification surface, proves terminality and one-time logical consumption, and produces only
unprivileged logical transitions.

The crate does not sign decisions, deserialize external wire values, establish that a supplied
credential-registry snapshot is durably current, commit state, activate policy, or issue an effect
permit. Those responsibilities remain at their named integration boundaries.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
