# Budget ancestor propagation checkpoint

The production `BudgetLedger::transition` contract now establishes equal consumption deltas
at every account on any finite stored parent path rooted at a changed account. The theorem
covers all five budget dimensions. It strengthens the previous immediate-parent relation;
it does not change runtime admission, accounting, or error behavior.

`ancestor_path` describes nonempty sequences of in-bounds account indices whose consecutive
entries follow the pre-ledger's actual parent identities. `all_ancestor_deltas_follow`
inducts on the path position. The induction retains that each predecessor changed, applies
the existing checked immediate-parent relation, and uses well-formed account identity
uniqueness to identify that relation's parent witness with the next path entry. Equality
of the five consumption deltas is then transitive. No assumed propagation result is added.

The public transition still has no executable precondition. Its actual `transition::apply`
implementation checks input and successor validity and complete refinement, then invokes
`accepted_result_is_exact`. That proof now derives the path theorem and the strengthened
public result specification. An error retains its existing semantics.

The theorem is conditional on a changed descendant: an unchanged sibling may legitimately
have a changed parent. Paths use accounts present before the transition; newly allocated
children have no pre-step index. This proves deltas of accounted consumption, not that an
external overrun report is fully charged. It does not construct a root path for every
account or prove journal commits, adapter behavior, external usage, or durable recovery.

Implementation was isolated from the changing integration worktree in a source copy of
`1aa9282ff2cfcbf8fac325f11f157f408d33ee30`. Strict pinned Verus passed 449 checks with zero
errors, all 27 budget tests passed, and strict all-target Clippy and formatting passed.
Independent Sol reviewer `/root/sol_runtime_phase` checked the three source hashes, actual
production composition, induction, five-dimension relation, nonvacuity, and these limits.
After integration, strict Verus again passed 449 checks with zero errors.

The review accepts this bounded theorem. It does not discharge an entire registered
obligation or authenticate a protected CI approval. The manifests retain source identities
and raw output; they are not a clean-commit or complete compiler-input attestation.
