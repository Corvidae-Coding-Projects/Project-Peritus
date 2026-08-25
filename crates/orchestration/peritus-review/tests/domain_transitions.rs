//! D2 reconciliation, confirmation, cancellation, and oscillation transition tests.

#![allow(clippy::unwrap_used, reason = "fixed integration fixtures use checked nonzero values")]

use peritus_context::ContextPlanId;
use peritus_evidence::EvidenceId;
use peritus_quality_policy::{ReviewCycleOrdinal, ReviewerIdentity};
use peritus_review::{
    Confidence, DispositionKind, Finding, FindingLocation, FindingSource, FixerResponse,
    OscillationKind, QualityProjection, ReviewAssignment, ReviewBinding, ReviewCommand,
    ReviewCommandKind, ReviewLimits, ReviewRunState, ReviewSubmission, ReviewTerminalKind, decide,
    start,
};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EnvironmentId, EventId, FindingId, GateId, Generation,
    HarnessId, PolicyId, ProviderProfileId, ReviewCycleId, RevisionNumber, RevisionTuple, RunId,
    Sha256Digest, WorkspaceId,
};

struct Fixture {
    contract: AcceptanceContract,
    revision: RevisionTuple,
    category: ReviewCategory,
    requirement: RequirementId,
    producer: ActorId,
    limits: ReviewLimits,
}

impl Fixture {
    fn new(maximum_cycles: u16) -> Self {
        let acceptance = AcceptanceSpecId::new(bytes(1)).unwrap();
        let gate_id = GateId::new(bytes(2)).unwrap();
        let gate_evidence = EvidenceRequirementId::new(digest(3));
        let category = ReviewCategory::new(digest(4));
        let requirement = RequirementId::new(digest(5));
        let graph = GateGraph::new(vec![
            GateDefinition::new(
                gate_id,
                GateExecutionPlan::new(
                    content(6),
                    EnvironmentId::new(bytes(7)).unwrap(),
                    content(8),
                    content(9),
                    GateSuccessRule::ExitCodeZero,
                    10_000,
                    content(10),
                    GateFreshnessScope::ExactRevisionTuple,
                )
                .unwrap(),
                Vec::new(),
                vec![gate_evidence],
            )
            .unwrap(),
        ])
        .unwrap();
        let revision = RevisionTuple::new(
            acceptance,
            HarnessId::new(bytes(11)).unwrap(),
            WorkspaceId::new(bytes(12)).unwrap(),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new(bytes(13)).unwrap(),
            ProviderProfileId::new(bytes(14)).unwrap(),
        );
        let contract = AcceptanceContract::new(
            acceptance,
            digest(15),
            ContractDocuments::new(
                content(16),
                content(17),
                content(18),
                content(19),
                content(20),
                content(21),
                content(22),
                content(23),
            ),
            vec![Requirement::new(requirement, content(24))],
            vec![Exclusion::new(content(25))],
            vec![Assumption::new(content(26))],
            graph,
            ReviewPolicy::new(
                vec![category],
                1,
                ReviewerIndependence::new(false, true, false, false, false, false),
                FindingSeverity::High,
            )
            .unwrap(),
            vec![
                EvidenceRequirement::new(
                    gate_evidence,
                    content(27),
                    EvidenceSource::Gate(gate_id),
                    ExportClassification::Internal,
                ),
                EvidenceRequirement::new(
                    EvidenceRequirementId::new(digest(28)),
                    content(29),
                    EvidenceSource::Review(category),
                    ExportClassification::Internal,
                ),
            ],
            CompletionPolicy::new(2, maximum_cycles).unwrap(),
            HumanApprovalPolicy::NotRequired,
            WaiverPolicy::Forbidden,
        )
        .unwrap();
        Self {
            contract,
            revision,
            category,
            requirement,
            producer: ActorId::new(bytes(30)).unwrap(),
            limits: limits(),
        }
    }

    fn binding(&self, revision: RevisionTuple, candidate: u8) -> ReviewBinding {
        ReviewBinding::from_contract(
            &self.contract,
            revision,
            digest(candidate),
            digest(candidate.wrapping_add(1)),
            vec![self.producer],
            vec![digest(31)],
            self.limits,
        )
        .unwrap()
    }

    fn assignment(&self, binding: &ReviewBinding, ordinal: u16, seed: u8) -> ReviewAssignment {
        let context = digest(seed.wrapping_add(70));
        ReviewAssignment::new(
            ReviewCycleId::new(bytes(seed)).unwrap(),
            ReviewCycleOrdinal::new(ordinal).unwrap(),
            binding,
            ReviewerIdentity::new(
                ActorId::new(bytes(seed.wrapping_add(40))).unwrap(),
                digest(seed.wrapping_add(41)),
                digest(seed.wrapping_add(42)),
                digest(seed.wrapping_add(43)),
                context,
                digest(seed.wrapping_add(44)),
                true,
            ),
            vec![self.category],
            ContextPlanId::new(context),
            true,
            self.limits,
        )
        .unwrap()
    }

    fn finding(
        &self,
        assignment: &ReviewAssignment,
        revision: RevisionTuple,
        id: u8,
        evidence: u8,
        semantic_seed: u8,
    ) -> Finding {
        self.finding_at_severity(
            assignment,
            revision,
            id,
            evidence,
            semantic_seed,
            FindingSeverity::High,
        )
    }

    fn finding_at_severity(
        &self,
        assignment: &ReviewAssignment,
        revision: RevisionTuple,
        id: u8,
        evidence: u8,
        semantic_seed: u8,
        severity: FindingSeverity,
    ) -> Finding {
        Finding::new(
            FindingId::new(bytes(id)).unwrap(),
            FindingSource::new(assignment.cycle_id(), assignment.reviewer().actor_id()),
            self.category,
            severity,
            FindingSeverity::High,
            Confidence::new(9_000).unwrap(),
            vec![self.requirement],
            vec![
                FindingLocation::new("src/reducer.rs".to_owned(), 10, 2, 10, 8, self.limits)
                    .unwrap(),
            ],
            vec![evidence_id(evidence)],
            format!("defect {semantic_seed}"),
            format!("reproduction {semantic_seed}"),
            format!("expected {semantic_seed}"),
            format!("remediation {semantic_seed}"),
            revision,
            self.limits,
        )
        .unwrap()
    }
}

#[test]
fn reconciliation_retains_all_sources_evidence_and_history() {
    let fixture = Fixture::new(8);
    let binding = fixture.binding(fixture.revision, 90);
    let mut state = started(&fixture, binding.clone());
    let first = fixture.assignment(&binding, 1, 2);
    state = assign_and_submit(
        &fixture,
        state,
        &first,
        vec![fixture.finding(&first, fixture.revision, 70, 71, 1)],
        2,
    );
    let second = fixture.assignment(&binding, 2, 4);
    state = assign_and_submit(
        &fixture,
        state,
        &second,
        vec![fixture.finding(&second, fixture.revision, 72, 73, 1)],
        4,
    );
    let canonical = FindingId::new(bytes(70)).unwrap();
    let duplicate = FindingId::new(bytes(72)).unwrap();
    assert!(
        decide(
            &state,
            &command(
                &state,
                6,
                ReviewCommandKind::ReconcileDuplicates {
                    canonical,
                    duplicates: vec![canonical],
                    reconciliation_digest: digest(74),
                },
            ),
        )
        .is_err()
    );
    state = decide(
        &state,
        &command(
            &state,
            6,
            ReviewCommandKind::ReconcileDuplicates {
                canonical,
                duplicates: vec![duplicate],
                reconciliation_digest: digest(74),
            },
        ),
    )
    .unwrap()
    .into_state();
    let retained = state.finding(canonical).unwrap();
    assert_eq!(retained.sources().len(), 2);
    assert!(
        retained
            .sources()
            .contains(&FindingSource::new(first.cycle_id(), first.reviewer().actor_id(),))
    );
    assert!(
        retained
            .sources()
            .contains(&FindingSource::new(second.cycle_id(), second.reviewer().actor_id(),))
    );
    assert!(retained.evidence().contains(&evidence_id(71)));
    assert!(retained.evidence().contains(&evidence_id(73)));
    assert!(retained.dispositions().len() >= 3, "both open histories plus reconciliation");
    let duplicate = state.finding(duplicate).unwrap();
    assert_eq!(duplicate.superseded_by(), Some(canonical));
    assert_eq!(duplicate.current_disposition(), DispositionKind::Superseded);
    assert!(
        decide(
            &state,
            &command(
                &state,
                7,
                ReviewCommandKind::ReconcileDuplicates {
                    canonical,
                    duplicates: vec![duplicate.id()],
                    reconciliation_digest: digest(75),
                },
            ),
        )
        .is_err()
    );
}

#[test]
fn dispute_invalidation_and_cycle_cancellation_follow_first_legal_transition() {
    let fixture = Fixture::new(8);
    let binding = fixture.binding(fixture.revision, 90);
    let mut state = started(&fixture, binding.clone());
    let reporter = fixture.assignment(&binding, 1, 2);
    let finding = fixture.finding(&reporter, fixture.revision, 70, 71, 1);
    state = assign_and_submit(&fixture, state, &reporter, vec![finding], 2);
    let finding_id = FindingId::new(bytes(70)).unwrap();
    let dispute_digest = digest(80);
    state = decide(
        &state,
        &command(
            &state,
            4,
            ReviewCommandKind::RecordFixerResponse {
                finding_id,
                response: FixerResponse::disputed(
                    ActorId::new(bytes(79)).unwrap(),
                    fixture.revision,
                    vec![evidence_id(80)],
                    dispute_digest,
                    fixture.limits,
                )
                .unwrap(),
            },
        ),
    )
    .unwrap()
    .into_state();
    assert!(state.oscillation().kinds().contains(&OscillationKind::Disagreement));
    let confirmer = fixture.assignment(&binding, 2, 5);
    state = decide(
        &state,
        &command(&state, 5, ReviewCommandKind::AssignReviewer { assignment: confirmer.clone() }),
    )
    .unwrap()
    .into_state();
    assert!(
        decide(
            &state,
            &command(
                &state,
                6,
                ReviewCommandKind::ConfirmInvalidation {
                    finding_id,
                    reviewer_cycle: confirmer.cycle_id(),
                    pending_response_digest: digest(81),
                    evidence: vec![evidence_id(81)],
                    confirmation_digest: digest(82),
                },
            ),
        )
        .is_err()
    );
    state = decide(
        &state,
        &command(
            &state,
            6,
            ReviewCommandKind::ConfirmInvalidation {
                finding_id,
                reviewer_cycle: confirmer.cycle_id(),
                pending_response_digest: dispute_digest,
                evidence: vec![evidence_id(81)],
                confirmation_digest: digest(82),
            },
        ),
    )
    .unwrap()
    .into_state();
    assert_eq!(
        state.finding(finding_id).unwrap().current_disposition(),
        DispositionKind::InvalidationConfirmed
    );
    assert!(QualityProjection::from_state(&state).unwrap().findings().is_empty());

    let cancelled = fixture.assignment(&binding, 3, 7);
    state = decide(
        &state,
        &command(&state, 7, ReviewCommandKind::AssignReviewer { assignment: cancelled.clone() }),
    )
    .unwrap()
    .into_state();
    state = decide(
        &state,
        &command(&state, 8, ReviewCommandKind::CancelCycle { cycle_id: cancelled.cycle_id() }),
    )
    .unwrap()
    .into_state();
    assert!(
        decide(
            &state,
            &command(&state, 9, ReviewCommandKind::CancelCycle { cycle_id: cancelled.cycle_id() }),
        )
        .is_err()
    );
}

#[test]
fn proposed_supersession_needs_exact_independent_confirmation() {
    let fixture = Fixture::new(8);
    let binding = fixture.binding(fixture.revision, 90);
    let mut state = started(&fixture, binding.clone());
    let first = fixture.assignment(&binding, 1, 2);
    state = assign_and_submit(
        &fixture,
        state,
        &first,
        vec![fixture.finding(&first, fixture.revision, 70, 71, 1)],
        2,
    );
    let second = fixture.assignment(&binding, 2, 4);
    state = assign_and_submit(
        &fixture,
        state,
        &second,
        vec![fixture.finding(&second, fixture.revision, 72, 73, 2)],
        4,
    );
    let original = FindingId::new(bytes(70)).unwrap();
    let replacement = FindingId::new(bytes(72)).unwrap();
    let proposal_digest = digest(83);
    state = decide(
        &state,
        &command(
            &state,
            6,
            ReviewCommandKind::RecordFixerResponse {
                finding_id: original,
                response: FixerResponse::supersession_proposed(
                    ActorId::new(bytes(80)).unwrap(),
                    fixture.revision,
                    replacement,
                    vec![evidence_id(81)],
                    proposal_digest,
                    fixture.limits,
                )
                .unwrap(),
            },
        ),
    )
    .unwrap()
    .into_state();
    let confirmer = fixture.assignment(&binding, 3, 7);
    state = decide(
        &state,
        &command(&state, 7, ReviewCommandKind::AssignReviewer { assignment: confirmer.clone() }),
    )
    .unwrap()
    .into_state();
    assert!(
        decide(
            &state,
            &command(
                &state,
                8,
                ReviewCommandKind::ConfirmSupersession {
                    finding_id: original,
                    superseding: replacement,
                    reviewer_cycle: confirmer.cycle_id(),
                    pending_response_digest: digest(84),
                    evidence: vec![evidence_id(85)],
                    confirmation_digest: digest(86),
                },
            ),
        )
        .is_err()
    );
    state = decide(
        &state,
        &command(
            &state,
            8,
            ReviewCommandKind::ConfirmSupersession {
                finding_id: original,
                superseding: replacement,
                reviewer_cycle: confirmer.cycle_id(),
                pending_response_digest: proposal_digest,
                evidence: vec![evidence_id(85)],
                confirmation_digest: digest(86),
            },
        ),
    )
    .unwrap()
    .into_state();
    assert_eq!(state.finding(original).unwrap().superseded_by(), Some(replacement));
    let replacement = state.finding(replacement).unwrap();
    assert_eq!(replacement.sources().len(), 2);
    assert!(replacement.evidence().contains(&evidence_id(71)));
    assert!(replacement.evidence().contains(&evidence_id(73)));
}

#[test]
fn repeated_fingerprint_and_cycle_exhaustion_finalize_needs_human() {
    let fixture = Fixture::new(2);
    let first_binding = fixture.binding(fixture.revision, 90);
    let mut state = started(&fixture, first_binding.clone());
    let first = fixture.assignment(&first_binding, 1, 2);
    state = assign_and_submit(
        &fixture,
        state,
        &first,
        vec![fixture.finding(&first, fixture.revision, 70, 71, 1)],
        2,
    );
    let next_revision = RevisionTuple::new(
        fixture.revision.acceptance_spec_id(),
        fixture.revision.harness_id(),
        fixture.revision.workspace_id(),
        fixture.revision.workspace_generation(),
        RevisionNumber::new(2).unwrap(),
        fixture.revision.policy_id(),
        fixture.revision.provider_profile_id(),
    );
    let next_binding = fixture.binding(next_revision, 92);
    state = decide(
        &state,
        &command(&state, 4, ReviewCommandKind::AdvanceRevision { binding: next_binding.clone() }),
    )
    .unwrap()
    .into_state();
    let second = fixture.assignment(&next_binding, 2, 5);
    state = assign_and_submit(
        &fixture,
        state,
        &second,
        vec![fixture.finding(&second, next_revision, 72, 73, 1)],
        5,
    );
    assert!(state.oscillation().triggered());
    assert!(state.oscillation().kinds().contains(&OscillationKind::RepeatedFindingSet));
    assert!(state.oscillation().kinds().contains(&OscillationKind::SeverityStagnation));
    assert!(state.oscillation().kinds().contains(&OscillationKind::ReviewCyclesExhausted));
    state =
        decide(&state, &command(&state, 7, ReviewCommandKind::FinalizeRun)).unwrap().into_state();
    assert_eq!(state.terminal().unwrap().kind(), ReviewTerminalKind::NeedsHuman);
    assert_ne!(state.terminal().unwrap().kind(), ReviewTerminalKind::Completed);
}

#[test]
fn worsening_maximum_severity_is_reported_as_regression() {
    let fixture = Fixture::new(8);
    let first_binding = fixture.binding(fixture.revision, 90);
    let mut state = started(&fixture, first_binding.clone());
    let first = fixture.assignment(&first_binding, 1, 2);
    state = assign_and_submit(
        &fixture,
        state,
        &first,
        vec![fixture.finding_at_severity(
            &first,
            fixture.revision,
            70,
            71,
            1,
            FindingSeverity::Medium,
        )],
        2,
    );
    let next_revision = RevisionTuple::new(
        fixture.revision.acceptance_spec_id(),
        fixture.revision.harness_id(),
        fixture.revision.workspace_id(),
        fixture.revision.workspace_generation(),
        RevisionNumber::new(2).unwrap(),
        fixture.revision.policy_id(),
        fixture.revision.provider_profile_id(),
    );
    let next_binding = fixture.binding(next_revision, 92);
    state = decide(
        &state,
        &command(&state, 4, ReviewCommandKind::AdvanceRevision { binding: next_binding.clone() }),
    )
    .unwrap()
    .into_state();
    let second = fixture.assignment(&next_binding, 2, 5);
    state = assign_and_submit(
        &fixture,
        state,
        &second,
        vec![fixture.finding_at_severity(&second, next_revision, 72, 73, 2, FindingSeverity::High)],
        5,
    );
    assert!(state.oscillation().kinds().contains(&OscillationKind::SeverityRegression));
    assert!(!state.oscillation().kinds().contains(&OscillationKind::SeverityStagnation));
}

fn started(fixture: &Fixture, binding: ReviewBinding) -> ReviewRunState {
    start(
        &ReviewCommand::new(
            CommandId::new(bytes(1)).unwrap(),
            EventId::new(bytes(1)).unwrap(),
            RunId::new(bytes(99)).unwrap(),
            0,
            None,
            digest(0),
            binding.revision(),
            ReviewCommandKind::StartRun { binding, limits: fixture.limits },
        )
        .unwrap(),
    )
    .unwrap()
    .into_state()
}

fn assign_and_submit(
    fixture: &Fixture,
    mut state: ReviewRunState,
    assignment: &ReviewAssignment,
    findings: Vec<Finding>,
    seed: u8,
) -> ReviewRunState {
    state = decide(
        &state,
        &command(
            &state,
            seed,
            ReviewCommandKind::AssignReviewer { assignment: assignment.clone() },
        ),
    )
    .unwrap()
    .into_state();
    let submission = ReviewSubmission::new(
        assignment.cycle_id(),
        assignment.revision(),
        vec![fixture.category],
        findings,
        FindingSeverity::High,
        fixture.limits,
    )
    .unwrap();
    decide(
        &state,
        &command(&state, seed.wrapping_add(1), ReviewCommandKind::SubmitReview { submission }),
    )
    .unwrap()
    .into_state()
}

fn command(state: &ReviewRunState, seed: u8, kind: ReviewCommandKind) -> ReviewCommand {
    ReviewCommand::new(
        CommandId::new(bytes(seed)).unwrap(),
        EventId::new(bytes(seed)).unwrap(),
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.binding().revision(),
        kind,
    )
    .unwrap()
}

fn limits() -> ReviewLimits {
    ReviewLimits::new(
        16, 16, 16, 128, 16, 16, 16, 32, 16, 64, 256, 4_096, 4_096, 1_048_576, 4_194_304,
    )
    .unwrap()
}

fn evidence_id(value: u8) -> EvidenceId {
    EvidenceId::new(bytes(value)).unwrap()
}
const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
const fn content(value: u8) -> ContentReference {
    ContentReference::new(digest(value))
}
