# Budget production correspondence checkpoint

This is a source audit of existing code at commit
`1aa9282ff2cfcbf8fac325f11f157f408d33ee30`, performed by `/root` in the active
formal-coverage worktree. The budget and types packages have no working-tree changes.
This checkpoint does not discharge obligations or approve an exclusion. Independent
review of this mapping and final proof-impact reconciliation remain pending.

## Executable connection

The public `BudgetLedger::transition` in `state.rs` calls `transition::apply`.
Both operate on the actual production `BudgetLedger` and `BudgetCommand` types
inside the same `verus!` module graph as ordinary Rust. The public function has
no execution precondition. Its result contract maps the actual returned
`BudgetTransition` or `BudgetError` to `reachability::budget_step`.

`transition::apply` validates the input, duplicates its exact account and reservation
sequences, dispatches the actual command, and validates the proposed successor and
refinement. The validators are verified executable functions. They establish the
properties checked at runtime; they do not assume that checking succeeded. Rejection
returns no successor and leaves the immutably borrowed input ledger unchanged.

`accepted_step_is_valid_and_complete`, named by several register entries, only unfolds
properties already present in its `accepted_step` premise. That lemma alone is not the
production proof. The relevant connection is the verified composition through
`transition::apply`, `transition::validation`, the command-local reducers, and their
exact result contracts. This distinction must be retained in the final inventory.

## Registered guarantees

| Register entry | Specification and actual implementation | Scope of this checkpoint |
| --- | --- | --- |
| INV-012 | `reachability::complete_refinement`, `model::ledger_well_formed`, and the actual `transition::apply` postcondition | Accepted results preserve conservation, monotonic account consumption and reservation high-water values, immutable identities, and parent propagation. Errors preserve the input. |
| OBL-0104 | `model::account_conserves`, `available_is_exact`, `ledger_well_formed`; executable `model::available` and `transition::validation` | All five resource dimensions are enumerated. Conservation is an inequality for consumed, reserved, and delegated amounts; the successful available-value computation establishes the corresponding exact sum with available capacity. |
| OBL-0105 | `reachability::commands::observation_step`, `reducer_proofs::observation::observation_refines`; actual `transition::reconciliation::observation::observe_validated` and its in-bounds/terminal helpers | In-bounds observations charge the cumulative difference; exact replay leaves the ledger unchanged and charges/releases zero. A lower cumulative observation is rejected. Overruns have the distinct semantics below. |
| OBL-0106 | `reachability::guards::capacity_fits`, `refinement_model::ancestor_consumption_propagates`; actual child-allocation, lineage charging, and refinement validation | A new child's limits must fit the parent's exact available capacity. The refinement predicate binds each changed account's five-dimensional consumption delta to its immediate parent's delta. The validated finite parent tree provides the transitive lineage interpretation. A separately named theorem quantifying over an entire ancestor path is not claimed here. |

An overrun is an accepted `OverrunFaulted` outcome, not an ordinary in-bounds
observation. It consumes the remaining reservation, records its accounted high-water
at the reservation ceiling, retains the larger supplied cumulative value separately
as `final_reported`, and faults the lineage. The proof does not say that an external
provider cannot exceed its reservation or that the conserved ledger accounts for
unbounded external consumption. The final OBL-0105 mapping must state this case
explicitly rather than imply that every accepted outcome charges the full reported
cumulative difference.

## Production consumers and remaining boundaries

The product command runtime calls the public reducer to allocate its child budget
(`peritus-product-runner/src/developer_tools/command_runtime/kernel.rs`) and begin
an effect reservation (`command_runtime/authority.rs`). The latter passes the actual
returned transition to `BudgetCommitRequest::new` and
`SqliteJournal::commit_budget_transition` in `peritus-journal/src/domain/budget.rs`.
These are ordinary production calls, not test-only imports.

The agent runtime's `runtime/budget.rs` emits the budget commands through a budget
port. A port's commitment to use this reducer, canonical protocol encoding, durable
CAS/idempotency installation, observation authenticity, and actual external resource
usage are not proved by the budget package's mathematical transition contract.
This checkpoint records these as separate work, not as technically justified
exclusions. The public snapshot/getter contracts and their downstream uses also
require review before any end-to-end consumer guarantee is discharged.

## Verification evidence

The retained strict run checked the fresh `target/formal-budget-review` target:
budget **447 verified, 0 errors**; its types dependency **210 verified, 0 errors**.
The command used the pinned toolchain check, all features, `--no-cheating`, and
`--rlimit 20`, with no item-selection flags. The ordinary budget suite passed
**27 tests**, including persisted-seed high-water and hierarchical lifecycle
reference models, exact replay, branch-ordered rejection frames, and commit boundaries.
No budget implementation or test was changed by this audit.

`source-sha256.txt` records the current budget/types Rust sources and manifests,
the budget tests, and shared build pins. It is a source checkpoint, not an atomic
before/after capture of every external compiler or dependency input. The surrounding
worktree was dirty. Raw logs and exact commands are retained beside this report.
