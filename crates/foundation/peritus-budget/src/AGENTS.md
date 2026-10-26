# crates/foundation/peritus-budget/src — navigation

## Responsibility

Hierarchical resource-budget accounting for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [accounting_model.rs](accounting_model.rs) | Exact specification folds for derived child and reservation accounting. |
| [accounting_prefix_model.rs](accounting_prefix_model.rs) | Prefix bounds for executable reconstruction of derived accounting totals. |
| [amounts.rs](amounts.rs) | Fixed-dimensional checked budget values. |
| [command.rs](command.rs) | Closed command vocabulary for pure budget transitions. |
| [failure.rs](failure.rs) | Stable typed failures for amount arithmetic and ledger transitions. |
| [identity_model.rs](identity_model.rs) | Extensional equality for opaque authority identities. |
| [invariant.rs](invariant.rs) | Concrete structural and accounting predicates for the ledger reducer. |
| [lib.rs](lib.rs) | Hierarchical resource-budget accounting for Peritus. |
| [limits.rs](limits.rs) | Budget dimensions, exact dimension sets, and immutable account ceilings. |
| [model.rs](model.rs) | Executable conservation helpers and their mathematical counterparts. |
| [reachability.rs](reachability.rs) | Closed reachability and total command/outcome relation for the budget reducer. |
| [refinement_model.rs](refinement_model.rs) | Concrete immutable-prefix and ancestor-propagation refinement predicates. |
| [snapshot.rs](snapshot.rs) | Checked read-only projections of private ledger records. |
| [state.rs](state.rs) | Privately represented ledger state, checked snapshots, and transition receipts. |
| [transition.rs](transition.rs) | Total value-in/value-out command reducer. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [accounting_model/](accounting_model/AGENTS.md) | Implementation pieces for `accounting_model.rs`: Exact specification folds for derived child and reservation accounting. |
| [amounts/](amounts/AGENTS.md) | Implementation pieces for `amounts.rs`: Fixed-dimensional checked budget values. |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Closed command vocabulary for pure budget transitions. |
| [reachability/](reachability/AGENTS.md) | Implementation pieces for `reachability.rs`: Closed reachability and total command/outcome relation for the budget reducer. |
| [refinement_model/](refinement_model/AGENTS.md) | Implementation pieces for `refinement_model.rs`: Concrete immutable-prefix and ancestor-propagation refinement predicates. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Privately represented ledger state, checked snapshots, and transition receipts. |
| [transition/](transition/AGENTS.md) | Implementation pieces for `transition.rs`: Total value-in/value-out command reducer. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
