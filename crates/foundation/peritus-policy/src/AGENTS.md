# crates/foundation/peritus-policy/src — navigation

## Responsibility

Verified, effect-free policy, capability, and authority-time decisions for Peritus. Public constructors validate ordinary-Rust inputs; outputs are only logical transitions.

## Files in this folder

| File | Role |
| --- | --- |
| [amendment.rs](amendment.rs) | Explicit single-tier policy-amendment preview without activation authority. |
| [api.rs](api.rs) | Stable public facade for the policy authority model. |
| [boundary.rs](boundary.rs) | Complete finite parent authority boundaries. |
| [capability.rs](capability.rs) | Move-only logical capability issuance and exact single-action use transitions. |
| [capability_failure.rs](capability_failure.rs) | Rejected capability uses that preserve the move-only authority value. |
| [capability_issuance.rs](capability_issuance.rs) | Move-only logical capability issuance transitions. |
| [capability_scope.rs](capability_scope.rs) | Complete actor, role, environment, permission, revision, time, and use scopes. |
| [capability_transition.rs](capability_transition.rs) | Successful exact action-bound capability-use transitions. |
| [capability_use.rs](capability_use.rs) | Exact action-bound capability-use requests. |
| [ceiling.rs](ceiling.rs) | Complete finite authority boundaries, default-deny grants, and immutable denials. |
| [decision.rs](decision.rs) | Public whole-request policy-evaluation entry point. |
| [decision_enum.rs](decision_enum.rs) | Exhaustive move-only public policy decision. |
| [decision_reason.rs](decision_reason.rs) | Stable whole-request policy denial reasons. |
| [decision_value.rs](decision_value.rs) | Move-only policy decision payloads and checked public accessors. |
| [definition.rs](definition.rs) | Complete immutable policy-definition validation. |
| [denial.rs](denial.rs) | Whole-request policy-denial value and exact scope views. |
| [digest_order.rs](digest_order.rs) | Shared exact ordering for fixed-size correlation digests. |
| [escalation.rs](escalation.rs) | Exact whole-request escalation challenges. |
| [evaluation.rs](evaluation.rs) | Composable whole-request policy-evaluation reducers. |
| [evaluation_approval.rs](evaluation_approval.rs) | Executable exact conjunction of all matching approval restrictions. |
| [evaluation_approval_result.rs](evaluation_approval_result.rs) | Final decision construction when policy evaluation requires approval. |
| [evaluation_constraint_initial.rs](evaluation_constraint_initial.rs) | Initial request-to-ceiling constraint intersection. |
| [evaluation_constraints.rs](evaluation_constraints.rs) | Exact executable validity and use-limit intersection reducer. |
| [evaluation_predicates.rs](evaluation_predicates.rs) | Constructive executable searches refining whole-request deny and coverage predicates. |
| [evaluation_result.rs](evaluation_result.rs) | Construction of final policy decisions after all policy folds succeed. |
| [failure.rs](failure.rs) | Stable typed failures for checked policy inputs and reducers. |
| [identity.rs](identity.rs) | Exact executable identity, revision, and canonical-order refinement helpers. |
| [independence.rs](independence.rs) | Canonical approver-independence conjunction values. |
| [layer.rs](layer.rs) | Authenticated restriction-tier ordering and canonical lower layers. |
| [lib.rs](lib.rs) | Verified, effect-free policy, capability, and authority-time decisions for Peritus. Public constructors validate ordinary-Rust inputs; outputs are only logical transitions. |
| [model.rs](model.rs) | Mathematical policy relations refined by executable reducers. |
| [operation.rs](operation.rs) | Authenticated operation descriptors and canonical registry lookup. |
| [operation_access.rs](operation_access.rs) | Public read-only operation-registry queries. |
| [operation_duplication.rs](operation_duplication.rs) | Exact immutable duplication of the policy-bound operation registry. |
| [operation_risk_fold.rs](operation_risk_fold.rs) | Canonical fixed-order fold for authenticated whole-request risks. |
| [operation_risks.rs](operation_risks.rs) | Exact policy-owned risk union for whole-request escalation. |
| [restriction_rule.rs](restriction_rule.rs) | Restriction-only deny and approval rule values. |
| [risk.rs](risk.rs) | Canonical nonempty risk classifications. |
| [role.rs](role.rs) | Stable security roles and compiled role-separation decisions. |
| [rule.rs](rule.rs) | Restriction-only policy rules and approval conjunction values. |
| [scope.rs](scope.rs) | Canonical exact permission pairs and logical use bounds. |
| [scope_selector.rs](scope_selector.rs) | Complete parent-relative scope selectors and exact executable refinement. |
| [scope_validation.rs](scope_validation.rs) | Exact canonical validation for selector identity collections. |
| [selector.rs](selector.rs) | Parent-relative selectors with exact finite wildcard semantics. |
| [time.rs](time.rs) | Linear greatest-observed authority-time state. |
| [use_limit.rs](use_limit.rs) | Exact unlimited or monotonically consumed logical-use bounds. |
| [validity.rs](validity.rs) | Epoch-bound authority instants and half-open validity windows. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [amendment/](amendment/AGENTS.md) | Implementation pieces for `amendment.rs`: Explicit single-tier policy-amendment preview without activation authority. |
| [ceiling/](ceiling/AGENTS.md) | Implementation pieces for `ceiling.rs`: Complete finite authority boundaries, default-deny grants, and immutable denials. |
| [definition/](definition/AGENTS.md) | Implementation pieces for `definition.rs`: Complete immutable policy-definition validation. |
| [failure/](failure/AGENTS.md) | Implementation pieces for `failure.rs`: Stable typed failures for checked policy inputs and reducers. |
| [proofs/](proofs/AGENTS.md) | Proof obligations for compiled separation, exact scope, restriction monotonicity, and reducers. |
| [scope_selector/](scope_selector/AGENTS.md) | Implementation pieces for `scope_selector.rs`: Complete parent-relative scope selectors and exact executable refinement. |
| [selector/](selector/AGENTS.md) | Implementation pieces for `selector.rs`: Parent-relative selectors with exact finite wildcard semantics. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
