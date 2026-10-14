# peritus-tools-quality

## Folder guide

Explicit quality check discovery, execution, and evidence adapters for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-quality`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Explicit quality-check discovery, invocation, and candidate B2 evidence. |

`peritus-tools-quality` supplies the built-in `quality.discover` and `quality.run` tools. Discovery
combines explicitly supplied typed definitions with deterministic Cargo and Just surfaces from an
immutable C1 workspace. Discovered checks never become acceptance policy implicitly.

Runs use the same restricted C2/C3 path as shell execution and produce structured execution facts
plus candidate B2 gate-observation inputs. The crate never asserts snapshot freshness or final gate
acceptance, and infrastructure, parser, timeout, cancellation, or artifact failures cannot pass.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
