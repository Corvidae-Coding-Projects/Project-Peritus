# peritus-collaboration

## Folder guide

Durable deterministic causal delegation, messaging, joins, and cancellation.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-collaboration`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable deterministic causal collaboration for Peritus. |

`peritus-collaboration` is the durable, deterministic D3 causal-delegation aggregate. It owns
root/parent/depth task structure, offered-to-active ownership, bounded messages, joins, exact
artifact handoffs, pause, cancellation propagation, terminal truth, replay, and C0 persistence.

The crate consumes scheduler identities and exact reservation observations but never schedules or
executes work. Messages and assignments are inert data, not authority. Every accepted command
produces exactly one event and one complete successor checkpoint; restart is reconstructed from
canonical family 74 events and checked against the family 75 state checkpoint.

Canonical schema-v1 families are 73 (commands), 74 (events), and 75 (state). The C0 checkpoint
namespace is `0xD302` and the aggregate kind is `Collaboration`.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
