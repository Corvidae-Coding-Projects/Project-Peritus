# peritus-tcb

## Folder guide

Audited trust-boundary policy anchor for Project Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tcb`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Compatibility exports naming historical verification-manifest paths; no manifests or proof runner are present in this package. |

`peritus-tcb` is a retained, dependency-free `no_std` compatibility package. It exports
repository-relative path names for historical verification manifests through
[src/lib.rs](src/lib.rs) and [src/manifests.rs](src/manifests.rs).

Those strings name the old actors, exclusions, obligations, proof-impact, and trust manifests.
The corresponding `verification/` infrastructure is no longer in this checkout. Returning a path
string does not load a manifest, establish authority, run a proof, or qualify a candidate.
The current [Cargo manifest](Cargo.toml) declares no dependencies or formal verification runner.

Read [the foundation map](../AGENTS.md) for active contract packages, and
[historical formal-foundation notes](../../../docs/formal-foundation.md) for the earlier design.
Use the [project real-testing policy](../../../README.md#real-testing) for behavioral changes.
