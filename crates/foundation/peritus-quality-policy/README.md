# peritus-quality-policy

`peritus-quality-policy` is the pure, verified acceptance evaluator for Peritus. It consumes one
checked `peritus-spec` contract, an exact `RevisionTuple`, and a canonical evidence set. It returns
an acceptable decision only when every contract requirement is satisfied by current observations.

The crate owns observation and decision semantics, not workflows. Gate execution, review
orchestration, persistence, authorization, and lifecycle transitions live in later crates.

## Invariants

- `INV-003 RevisionFreshness`: a gate, review, evidence, approval, resolution, or waiver bound to a
  different revision tuple never contributes to acceptance.
- `INV-004 AcceptanceCompleteness`: the executable evaluator rejects missing or failed gates,
  exceeded completion limits, missing evidence, incomplete or non-independent review, unwaived
  blockers, invalid waivers, and missing or denied human approval. Adversarial tests cover those
  contract semantics. Verus connects an acceptable result to exact freshness, contract identity,
  an empty typed unmet-condition sequence, and typed gate-attempt/review-cycle predicates. The
  remaining phase-status conjunction is intentionally described as structural until
  `peritus-spec` exports specification views for its review, evidence, approval, waiver, and
  completion-policy fields.
- Evidence collections are checked, duplicate-free, and in canonical order. Review cycle
  identities and their one-based ordinals are both unique and ascend together, so each review
  observation consumes exactly one configured cycle.
- Human approvals and waivers are explicit observations. There are no permissive defaults.
- Evaluation is deterministic, total, effect-free, and returns every unmet condition in stable
  phase and identifier order.

## Dependency policy

Production dependencies are limited to `peritus-spec`, `peritus-types`, and `vstd`. This crate may
not depend on orchestration, persistence, process, workspace, provider, or application crates.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-quality-policy
```
