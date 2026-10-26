# crates/foundation/peritus-budget/src/reachability — navigation

## Responsibility

Implementation pieces for `reachability.rs`: Closed reachability and total command/outcome relation for the budget reducer.

## Files in this folder

| File | Role |
| --- | --- |
| [account_updates.rs](account_updates.rs) | Exact operational account-vector updates for lineage accounting. |
| [accounts.rs](accounts.rs) | Exact account-vector effects used by the closed budget step relation. |
| [allocation.rs](allocation.rs) | Exact child-allocation and closure account effects. |
| [cancellation_proofs.rs](cancellation_proofs.rs) | Exact held-reservation cancellation refinement. |
| [commands.rs](commands.rs) | Closed exact accepted-step relations for every budget command. |
| [equivalence.rs](equivalence.rs) | Extensional transport across executable ledger duplication. |
| [executable_bridge.rs](executable_bridge.rs) | Specification projections for the executable accounting helpers. |
| [functionality.rs](functionality.rs) | Functionality of the exact command-candidate relation. |
| [guards.rs](guards.rs) | Exact input-state guards for every accepted command outcome. |
| [initial_proof.rs](initial_proof.rs) | Exact proof that the public root constructor creates the unique initial shape. |
| [lifecycle_proofs.rs](lifecycle_proofs.rs) | Exact activation and account lifecycle refinement lemmas. |
| [lifecycle_steps.rs](lifecycle_steps.rs) | Exact finalization and account-lifecycle command steps. |
| [public_model.rs](public_model.rs) | Stable crate-level projections over private command guards and rejection relations. |
| [reducer_proofs.rs](reducer_proofs.rs) | Command-local exact-effect predicates and reducer refinement lemmas. |
| [rejections.rs](rejections.rs) | Branch-ordered typed rejection relation for total reducer outcomes. |
| [reservations.rs](reservations.rs) | Exact reservation-vector effects used by accepted command steps. |
| [total.rs](total.rs) | Total accepted/rejected outcomes and finite accepted-state reachability. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [commands/](commands/AGENTS.md) | Implementation pieces for `commands.rs`: Closed exact accepted-step relations for every budget command. |
| [functionality/](functionality/AGENTS.md) | Implementation pieces for `functionality.rs`: Functionality of the exact command-candidate relation. |
| [guards/](guards/AGENTS.md) | Implementation pieces for `guards.rs`: Exact input-state guards for every accepted command outcome. |
| [reducer_proofs/](reducer_proofs/AGENTS.md) | Implementation pieces for `reducer_proofs.rs`: Command-local exact-effect predicates and reducer refinement lemmas. |
| [rejections/](rejections/AGENTS.md) | Implementation pieces for `rejections.rs`: Branch-ordered typed rejection relation for total reducer outcomes. |

Owning module: [reachability.rs](../reachability.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
