# peritus-secrets

## Folder guide

Exact secret leases, zeroizing delivery, redaction, and credential-store adapters.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-secrets`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Exact secret leases, zeroizing material, scoped delivery, and redaction support. |

`peritus-secrets` resolves exact C2 `SecretReference` values through platform credential stores,
retains values only in zeroizing non-clone material, and delivers them through exact expiring
leases. Ordinary debug, error, canonical, observation, recovery, and artifact surfaces contain
references and keyed redaction fingerprints, never secret bytes.

The in-memory store exists only for deterministic tests. Production probes fail closed when the
platform credential service is unavailable.

The platform adapter also provides the product onboarding boundary for writing, replacing, and
removing credentials. Those operations accept zeroizing `SecretMaterial` and return only exact
content-bound `SecretReference` values.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
