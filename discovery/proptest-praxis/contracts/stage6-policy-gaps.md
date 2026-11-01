# Stage 6 remaining policy coverage gaps

This ledger describes untested source paths, not a claim of product failure.

* `crates/foundation/peritus-policy/src/decision.rs` exposes
  `PolicyDefinition::evaluate`. The Stage 6 authority target covers public
  authority time, validity-window intersection, and use-limit primitives;
  it does not model whole-request registry identity, actor role, operation and
  permission ceilings, immutable/restriction denies, grant conjunction,
  approval clauses, exact denial precedence, or effective scope construction
  in `evaluation.rs`, `evaluation_constraints.rs`, and
  `evaluation_approval.rs`.
* `crates/foundation/peritus-quality-policy/src/evaluator.rs` exposes
  `evaluate_acceptance`. There is no Stage 6 independent oracle for its exact
  contract/revision binding, freshness, gate attempts, evidence requirements,
  review cycles, waivers, final approval, or ordered unmet conditions. The
  Stage 6 spec target checks gate graph behavior only; the Stage 6 kernel
  target uses a synthetic accepted contract as an input and does not test
  quality-policy settlement.
* `crates/foundation/peritus-security-policy/src/evaluator.rs` exposes
  `evaluate_security_readiness`. The Stage 6 security target covers one
  altered observation per family and a bounded review/finding state. It does
  not cover malformed `SecurityEvidence::new` input, multiple findings,
  nested finding candidate mismatch rejection, or every candidate tuple
  dimension. It tests H0 readiness evidence, which the evaluator itself
  documents as not conferring release authority.
* `crates/foundation/peritus-release-policy/src/evaluator.rs` exposes
  `evaluate_release`. No Stage 6 independent oracle covers its H4 evidence,
  qualifications, independent reviewer threshold, finding aggregation,
  exact candidate binding, diagnostic order, or decision digest. The H0
  security target does not substitute for H4 qualification.
* `crates/foundation/peritus-kernel/src/reducer.rs` and the reducer modules
  expose more lifecycle commands than the bounded Stage 6 kernel target.
  Action authorization/dispatch, review, waiver, acceptance settlement,
  multiple run and attempt identities, and durable replay are outside this
  target. The router target separately checks one committed tool execution
  and current-authority replay, while the existing Stage 3 agent-driver
  target handles selected crash/reopen boundaries.
