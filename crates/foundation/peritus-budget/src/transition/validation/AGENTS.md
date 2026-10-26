# crates/foundation/peritus-budget/src/transition/validation — navigation

## Responsibility

Implementation pieces for `validation.rs`: Executable tree, tombstone, conservation, and refinement validation.

## Files in this folder

| File | Role |
| --- | --- |
| [accounts.rs](accounts.rs) | Complete executable account-tree and conservation validation. |
| [arithmetic.rs](arithmetic.rs) | Contradictions that make derived-accounting arithmetic failures unreachable. |
| [records.rs](records.rs) | Reservation tombstone phase validation. |
| [refinement.rs](refinement.rs) | Executable monotonicity, immutable-prefix, and ancestor-propagation checks. |
| [reservations.rs](reservations.rs) | Complete executable reservation-tombstone and retry-history validation. |
| [structure.rs](structure.rs) | Executable uniqueness and parent-order checks with exact specification contracts. |
| [sums.rs](sums.rs) | Executable exact folds for delegated and operation-reserved capacity. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [refinement/](refinement/AGENTS.md) | Implementation pieces for `refinement.rs`: Executable monotonicity, immutable-prefix, and ancestor-propagation checks. |
| [sums/](sums/AGENTS.md) | Implementation pieces for `sums.rs`: Executable exact folds for delegated and operation-reserved capacity. |

Owning module: [validation.rs](../validation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
