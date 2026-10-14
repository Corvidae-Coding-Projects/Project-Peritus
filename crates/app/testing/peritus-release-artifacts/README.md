# peritus-release-artifacts

## Folder guide

Deterministic H4 release artifact, SBOM, provenance, signature, and recovery evidence contracts.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-release-artifacts`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Deterministic contracts for H4 release artifacts and supply-chain evidence. |

`peritus-release-artifacts` is the H4 effect-boundary contract crate for release inputs. It
validates an exact candidate binding, inventories immutable artifacts, renders deterministic SPDX
2.3 and SLSA-style provenance documents, verifies detached Ed25519 signatures, compares outputs
from independent builders, and inventories migration/recovery/license documentation.

The crate is deliberately incapable of generating keys, signing, tagging, publishing, or turning
an observation into a release decision. Callers supply explicit bytes and identities; successful
construction means only that the evidence is internally consistent and content-addressed.

Canonical JSON is compact UTF-8 emitted from fixed-field structs and path/key-sorted vectors. No
ambient time, host, Git state, environment variable, or network lookup participates in generation.

## Behavioral work

Use the [project real-testing policy](../../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
