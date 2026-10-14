# peritus-codec

## Folder guide

Canonical versioned framing and bounded binary decoding for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-codec`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Canonical versioned framing and bounded binary decoding for Peritus. |

`peritus-codec` owns Peritus's canonical binary primitives, versioned frame header, bounded
reader/writer, and SHA-256 helpers. It is domain-neutral: lifecycle, policy, budget, and acceptance
messages are defined by `peritus-protocol`.

The codec treats every input byte as untrusted. It checks lengths before allocation or slicing,
rejects unknown primitive tags and trailing bytes, and never treats a digest as authenticity or
authority evidence.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
