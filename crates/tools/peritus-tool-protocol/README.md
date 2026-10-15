# peritus-tool-protocol

## Folder guide

Bounded canonical model-facing tool protocol for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tool-protocol`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Bounded canonical model-facing tool protocol. |

Version-one bounded, canonical tool descriptors and invocation envelopes. The crate owns no
effects: it validates schemas, calls, progress, artifacts, results, controls, and replay identity.

Every public envelope emits deterministic `PTL1` bytes through a shared family/version framing
encoder. `CanonicalEnvelope::parse` provides bounded, lossless framing round trips; semantic
construction remains owned by each typed envelope so untrusted bytes cannot bypass validation.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
