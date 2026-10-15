# Peritus release operator

## Folder guide

Native H4 release evidence generation and publication adapter.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-release-operator`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Native release evidence and publication effect boundary. Entry names: `main`. |

This binary is the narrow effect boundary for public release evidence. It reads one native package
record produced by `xtask`, builds deterministic inventory, SPDX 2.3, and SLSA provenance documents
with `peritus-release-artifacts`, and publishes those files only after the workflow supplies two
GitHub Sigstore attestation bundles.

The operator does not create signing keys or decide whether a candidate is ready. GitHub Actions
provides the short-lived signing identity, while the H4 qualification and release-policy crates
remain responsible for release admission.

## Behavioral work

Use the [project real-testing policy](../../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
