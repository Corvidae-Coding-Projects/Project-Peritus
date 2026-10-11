# B2 Acceptance Specification

## Outcome

B2 supplies the immutable acceptance contract consumed by the lifecycle kernel and the pure policy
decision that says whether exact-revision evidence satisfies that contract. The implementation is
split into two verification-class `V` crates with no effects, persistence, serialization, command
execution, or model integration.

## Non-negotiable contracts

- Every contract has one `AcceptanceSpecId`, one content digest, and checked components.
- A contract is usable for a `RevisionTuple` only when the tuple's acceptance-specification ID is
  exactly the contract ID.
- Gate identifiers, requirement identifiers, review categories, and evidence requirements are
  unique and deterministically ordered.
- Every gate dependency names a declared gate; self-dependencies and cycles are rejected.
- Gate and review observations count only for the exact requested `RevisionTuple`.
- Acceptance requires every required gate to pass, every evidence requirement to be present, the
  reviewer quorum and category coverage to be met, reviewer independence to hold, and no
  unwaived blocker to remain.
- Missing, failed, stale, duplicated, or contradictory observations never become success.
- Human approval and waiver evidence are explicit policy inputs, never implicit defaults.

## Crate boundary

### `peritus-spec`

Owns checked, immutable contract data:

- `AcceptanceContract` and `ContractBinding`;
- `Requirement`, `RequirementId`, `Exclusion`, and `Assumption`;
- `GateDefinition`, `GateGraph`, `GateFreshnessScope`, and gate success/evidence declarations;
- `ReviewCategory`, `ReviewPolicy`, `ReviewerIndependence`, and severity threshold;
- `EvidenceRequirement`, retry/cycle limits, completion rules, and approval/waiver declarations;
- typed construction and validation failures;
- executable validation and adversarial tests for every canonical collection, plus Verus
  specifications/proofs for exact revision binding, gate-graph uniqueness and DAG validity, and
  immutable identity.

The crate depends only on `peritus-types` and `vstd` in production. It does not execute gates or
interpret evidence.

### `peritus-quality-policy`

Owns checked observations and the deterministic acceptance decision:

- exact-revision `GateObservation`, `ReviewObservation`, `EvidenceObservation`,
  `ApprovalObservation`, and `WaiverObservation`;
- gate outcome, finding severity, reviewer identity/independence facts, and blocker disposition;
- `AcceptanceEvidence`, `AcceptanceDecision`, and typed rejection reasons;
- a total, effect-free evaluator over an `AcceptanceContract`, requested `RevisionTuple`, and
  evidence set;
- Verus specifications/proofs for exact revision freshness, contract binding, completion-limit
  enforcement, an empty typed unmet-condition set, and structural phase completeness; adversarial
  tests refine the remaining gate, evidence, review, waiver, and approval semantics.

It depends on `peritus-spec`, `peritus-types`, and `vstd`. It does not own review workflows,
finding state machines, gate execution, persistence, or authorization.

## Downstream API

B0 supplies an immutable `AcceptanceContract`, an exact current `RevisionTuple`, and a checked
`AcceptanceEvidence` value to the quality evaluator. The result is either `Acceptable` or a
deterministically ordered set of typed unmet conditions. It is a logical decision only: B0 remains
responsible for legal lifecycle transitions and later slices remain responsible for durable
evidence provenance.

D1 consumes the validated gate graph and emits observations using the frozen gate identifiers.
D2 consumes the review policy and emits reviewer/category/blocker observations. Neither may
silently rewrite the contract.

## Error and compatibility policy

Public fields remain private. Constructors reject empty/duplicate/unknown/cyclic inputs and expose
typed errors with stable error kinds. Ordered accessors return canonical order. New optional policy
features may be added compatibly; changing acceptance meaning, defaults, or identity binding
requires a contract revision and architecture review.

## Parallel ownership

- Spec worker: `crates/foundation/peritus-spec/**` only.
- Quality worker: `crates/foundation/peritus-quality-policy/**` only, coding to the public contract
  above and adapting imports once the spec API lands.
- Integrator: root workspace manifests, architecture/verification inventories, cross-crate tests,
  formatting, Gate A, review, commit, and merge.

## Verification target

Focused ordinary tests, strict Clippy, rustdoc, and Verus verification must pass for both crates.
Adversarial tests cover unknown dependencies, cycles, duplicates, wrong-spec tuple binding, one
field of tuple drift at a time, stale observations, missing gates/categories/evidence, duplicate
reviewers, shared identities/ancestry, blockers, invalid waivers, and required human approval.
The complete workspace Gate A runs before merge and again at the exact merged `main` commit.
