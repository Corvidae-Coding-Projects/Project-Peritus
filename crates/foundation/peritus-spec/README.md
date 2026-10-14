# peritus-spec

## Folder guide

immutable acceptance contracts for Project Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-spec`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Immutable acceptance contracts for Peritus. |

`peritus-spec` is the verified, effect-free acceptance-contract model for Peritus. It freezes
stable requirements, exclusions, assumptions, deterministic gates, review policy, required
evidence, bounded completion policy, and explicit human approval and waiver declarations.

## Invariants

- Contract, requirement, gate, category, and evidence identities are immutable.
- All externally supplied sets are non-ambiguous and retained in strict canonical order.
- Every gate dependency names a declared gate, and the checked graph is acyclic.
- Gate-specific evidence names a contract-wide evidence declaration.
- Review categories, quorum, blocker threshold, and independence facts are explicit contract data.
- Approval and waiver requirements are explicit and their required evidence is validated.
- A contract binding exists only when its `AcceptanceSpecId` exactly matches the governing
  `RevisionTuple`.

## Boundary

This crate does not parse files, hash bytes, execute gates, inspect findings, evaluate evidence,
persist contracts, or grant authority. Callers supply already-computed identifiers and digests.
`peritus-quality-policy` evaluates observations; later gate and review crates produce them.

The production dependency set is limited to `peritus-types` and the workspace-pinned `vstd`.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
