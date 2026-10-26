# crates/foundation/peritus-policy/src/proofs — navigation

## Responsibility

Proof obligations for compiled separation, exact scope, restriction monotonicity, and reducers.

## Files in this folder

| File | Role |
| --- | --- |
| [mod.rs](mod.rs) | Proof obligations for compiled separation, exact scope, restriction monotonicity, and reducers. |
| [monotonicity.rs](monotonicity.rs) | Proofs that ordinary composition only shrinks pointwise allowed query sets (`INV-022`). |
| [refinement.rs](refinement.rs) | Refinement lemmas for executable time and limited-use reducers. |
| [role_separation.rs](role_separation.rs) | Proofs for non-configurable role separation (`INV-007`). |
| [scope.rs](scope.rs) | Proofs for exact permission pairs and preserved complete scope (`INV-008`). |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
