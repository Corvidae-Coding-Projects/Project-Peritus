# peritus-spec

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
