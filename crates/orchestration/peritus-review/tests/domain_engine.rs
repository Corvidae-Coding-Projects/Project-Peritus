//! D2 review lifecycle, quorum, disposition, waiver, and terminal integration tests.

#![allow(clippy::unwrap_used, reason = "fixed integration fixtures use checked nonzero values")]

use peritus_context::ContextPlanId;
use peritus_evidence::EvidenceId;
use peritus_quality_policy::{ReviewCycleOrdinal, ReviewerIdentity, WaiverObservation};
use peritus_review::{
    Confidence, Finding, FindingLocation, FindingSource, FixerResponse, ObservedWaiver,
    QuorumDimension, ReviewAssignment, ReviewBinding, ReviewCommand, ReviewCommandKind,
    ReviewLimits, ReviewRunState, ReviewSubmission, ReviewTerminalKind, decide, replay, start,
};
use peritus_spec::{
    AcceptanceContract, Assumption, CompletionPolicy, ContentReference, ContractDocuments,
    EvidenceRequirement, EvidenceRequirementId, EvidenceSource, Exclusion, ExportClassification,
    FindingSeverity, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, HumanApprovalPolicy, Requirement, RequirementId, ReviewCategory, ReviewPolicy,
    ReviewerIndependence, WaiverPolicy,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, ApprovalRequestId, CommandId, EnvironmentId, EventId, FindingId,
    GateId, Generation, HarnessId, PolicyId, ProviderProfileId, ReviewCycleId, RevisionNumber,
    RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

struct Fixture {
    contract: AcceptanceContract,
    revision: RevisionTuple,
    category_a: ReviewCategory,
    category_b: ReviewCategory,
    requirement: RequirementId,
    waiver_authority: ContentReference,
    waiver_evidence: EvidenceRequirementId,
    limits: ReviewLimits,
    producer: ActorId,
}

impl Fixture {
    #[allow(
        clippy::too_many_lines,
        reason = "the deterministic domain fixture keeps its complete identity set in one constructor"
    )]
    fn new(quorum: u16, maximum_cycles: u16, waiver: bool) -> Self {
        let acceptance = AcceptanceSpecId::new(bytes(1)).unwrap();
        let category_a = ReviewCategory::new(digest(40));
        let category_b = ReviewCategory::new(digest(41));
        let requirement = RequirementId::new(digest(7));
        let gate_id = GateId::new(bytes(20)).unwrap();
        let gate_evidence = EvidenceRequirementId::new(digest(100));
        let review_evidence = EvidenceRequirementId::new(digest(101));
        let waiver_evidence = EvidenceRequirementId::new(digest(102));
        let waiver_authority = ContentReference::new(digest(110));
        let gate_plan = GateExecutionPlan::new(
            content(10),
            EnvironmentId::new(bytes(11)).unwrap(),
            content(12),
            content(13),
            GateSuccessRule::ExitCodeZero,
            10_000,
            content(14),
            GateFreshnessScope::ExactRevisionTuple,
        )
        .unwrap();
        let graph = GateGraph::new(vec![
            GateDefinition::new(gate_id, gate_plan, Vec::new(), vec![gate_evidence]).unwrap(),
        ])
        .unwrap();
        let review_policy = ReviewPolicy::new(
            vec![category_a, category_b],
            quorum,
            ReviewerIndependence::new(true, true, true, true, true, true),
            FindingSeverity::High,
        )
        .unwrap();
        let waiver_policy = if waiver {
            WaiverPolicy::Allowed { authority: waiver_authority, evidence: waiver_evidence }
        } else {
            WaiverPolicy::Forbidden
        };
        let mut evidence = vec![
            EvidenceRequirement::new(
                gate_evidence,
                content(120),
                EvidenceSource::Gate(gate_id),
                ExportClassification::Internal,
            ),
            EvidenceRequirement::new(
                review_evidence,
                content(121),
                EvidenceSource::Review(category_a),
                ExportClassification::Internal,
            ),
        ];
        if waiver {
            evidence.push(EvidenceRequirement::new(
                waiver_evidence,
                content(122),
                EvidenceSource::WaiverAuthorization,
                ExportClassification::Restricted,
            ));
        }
        let revision = RevisionTuple::new(
            acceptance,
            HarnessId::new(bytes(2)).unwrap(),
            WorkspaceId::new(bytes(3)).unwrap(),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new(bytes(4)).unwrap(),
            ProviderProfileId::new(bytes(5)).unwrap(),
        );
        let contract = AcceptanceContract::new(
            acceptance,
            digest(6),
            ContractDocuments::new(
                content(16),
                content(17),
                content(18),
                content(19),
                content(21),
                content(22),
                content(23),
                content(24),
            ),
            vec![Requirement::new(requirement, content(8))],
            vec![Exclusion::new(content(9))],
            vec![Assumption::new(content(15))],
            graph,
            review_policy,
            evidence,
            CompletionPolicy::new(2, maximum_cycles).unwrap(),
            HumanApprovalPolicy::NotRequired,
            waiver_policy,
        )
        .unwrap();
        Self {
            contract,
            revision,
            category_a,
            category_b,
            requirement,
            waiver_authority,
            waiver_evidence,
            limits: limits(),
            producer: ActorId::new(bytes(30)).unwrap(),
        }
    }

    fn binding(&self, candidate: u8) -> ReviewBinding {
        ReviewBinding::from_contract(
            &self.contract,
            self.revision,
            digest(candidate),
            digest(candidate.wrapping_add(1)),
            vec![self.producer],
            vec![digest(31)],
            self.limits,
        )
        .unwrap()
    }

    fn assignment(
        &self,
        binding: &ReviewBinding,
        ordinal: u16,
        seed: u8,
        provider: u8,
        fresh: bool,
    ) -> ReviewAssignment {
        let context = digest(seed.wrapping_add(70));
        ReviewAssignment::new(
            ReviewCycleId::new(bytes(seed)).unwrap(),
            ReviewCycleOrdinal::new(ordinal).unwrap(),
            binding,
            ReviewerIdentity::new(
                ActorId::new(bytes(seed.wrapping_add(20))).unwrap(),
                digest(provider),
                digest(seed.wrapping_add(50)),
                digest(seed.wrapping_add(60)),
                context,
                digest(seed.wrapping_add(80)),
                true,
            ),
            vec![self.category_a, self.category_b],
            ContextPlanId::new(context),
            fresh,
            self.limits,
        )
        .unwrap()
    }
}

#[test]
fn checked_bounds_and_context_plan_binding_reject_invalid_input() {
    assert!(Confidence::new(10_001).is_err());
    assert!(ReviewLimits::new(0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1).is_err());
    let fixture = Fixture::new(1, 3, false);
    assert!(FindingLocation::new("../outside".to_owned(), 1, 1, 1, 2, fixture.limits).is_err());
    assert!(FindingLocation::new("src/lib.rs".to_owned(), 2, 1, 1, 1, fixture.limits).is_err());
    let binding = fixture.binding(90);
    let reviewer = ReviewerIdentity::new(
        ActorId::new(bytes(50)).unwrap(),
        digest(51),
        digest(52),
        digest(53),
        digest(54),
        digest(55),
        true,
    );
    let result = ReviewAssignment::new(
        ReviewCycleId::new(bytes(56)).unwrap(),
        ReviewCycleOrdinal::new(1).unwrap(),
        &binding,
        reviewer,
        vec![fixture.category_a],
        ContextPlanId::new(digest(99)),
        true,
        fixture.limits,
    );
    assert!(result.is_err(), "reviewer context must equal the C6 plan digest");
}

#[test]
fn independent_quorum_dimensions_and_truthful_completion() {
    let fixture = Fixture::new(2, 4, false);
    let binding = fixture.binding(90);
    let start_command = genesis(&fixture, binding.clone(), 1);
    let first = start(&start_command).unwrap();
    let mut events = vec![first.event().clone()];
    let mut state = first.into_state();
    for (ordinal, seed) in [(1, 2), (2, 4)] {
        let assignment = fixture.assignment(&binding, ordinal, seed, seed, true);
        let transition = decide(
            &state,
            &command(
                &state,
                seed,
                ReviewCommandKind::AssignReviewer { assignment: assignment.clone() },
            ),
        )
        .unwrap();
        events.push(transition.event().clone());
        state = transition.into_state();
        let submission = ReviewSubmission::new(
            assignment.cycle_id(),
            fixture.revision,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            FindingSeverity::High,
            fixture.limits,
        )
        .unwrap();
        let transition = decide(
            &state,
            &command(&state, seed.wrapping_add(1), ReviewCommandKind::SubmitReview { submission }),
        )
        .unwrap();
        events.push(transition.event().clone());
        state = transition.into_state();
    }
    for dimension in dimensions() {
        assert!(state.quorum().passes(dimension), "failed dimension: {dimension:?}");
    }
    let transition = decide(&state, &command(&state, 8, ReviewCommandKind::FinalizeRun)).unwrap();
    events.push(transition.event().clone());
    state = transition.into_state();
    assert_eq!(state.terminal().unwrap().kind(), ReviewTerminalKind::Completed);
    assert!(decide(&state, &command(&state, 9, ReviewCommandKind::CancelRun)).is_err());
    assert_eq!(replay(&events).unwrap(), state);
}

#[test]
fn each_independence_fact_stays_visible_in_the_report() {
    let fixture = Fixture::new(2, 4, false);
    let binding = fixture.binding(90);
    let mut state = start(&genesis(&fixture, binding.clone(), 1)).unwrap().into_state();
    for (ordinal, seed) in [(1, 2), (2, 4)] {
        let assignment = fixture.assignment(&binding, ordinal, seed, 77, true);
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
            fixture.revision,
            vec![fixture.category_a, fixture.category_b],
            Vec::new(),
            FindingSeverity::High,
            fixture.limits,
        )
        .unwrap();
        state = decide(
            &state,
            &command(&state, seed.wrapping_add(1), ReviewCommandKind::SubmitReview { submission }),
        )
        .unwrap()
        .into_state();
    }
    assert!(state.quorum().passes(QuorumDimension::SubmittedReviewCount));
    assert!(state.quorum().passes(QuorumDimension::DistinctReviewerIdentities));
    assert!(!state.quorum().passes(QuorumDimension::DistinctProviders));
    assert!(!state.quorum().complete());
    assert!(decide(&state, &command(&state, 8, ReviewCommandKind::FinalizeRun)).is_err());
}

#[test]
fn fixer_evidence_needs_independent_current_reviewer_confirmation() {
    let fixture = Fixture::new(1, 3, false);
    let binding = fixture.binding(90);
    let mut state = start(&genesis(&fixture, binding.clone(), 1)).unwrap().into_state();
    let first = fixture.assignment(&binding, 1, 2, 2, true);
    state = decide(
        &state,
        &command(&state, 2, ReviewCommandKind::AssignReviewer { assignment: first.clone() }),
    )
    .unwrap()
    .into_state();
    let finding_id = FindingId::new(bytes(70)).unwrap();
    let finding = Finding::new(
        finding_id,
        FindingSource::new(first.cycle_id(), first.reviewer().actor_id()),
        fixture.category_a,
        FindingSeverity::High,
        FindingSeverity::High,
        Confidence::new(9_000).unwrap(),
        vec![fixture.requirement],
        vec![FindingLocation::new("src/lib.rs".to_owned(), 1, 1, 1, 8, fixture.limits).unwrap()],
        vec![evidence_id(71)],
        "incorrect state transition".to_owned(),
        "apply the stale command".to_owned(),
        "the command is rejected".to_owned(),
        "check the predecessor fence".to_owned(),
        fixture.revision,
        fixture.limits,
    )
    .unwrap();
    let submission = ReviewSubmission::new(
        first.cycle_id(),
        fixture.revision,
        vec![fixture.category_a, fixture.category_b],
        vec![finding],
        FindingSeverity::High,
        fixture.limits,
    )
    .unwrap();
    state = decide(&state, &command(&state, 3, ReviewCommandKind::SubmitReview { submission }))
        .unwrap()
        .into_state();
    assert!(decide(&state, &command(&state, 4, ReviewCommandKind::FinalizeRun)).is_err());
    let fixer = ActorId::new(bytes(80)).unwrap();
    let response_digest = digest(81);
    let response = FixerResponse::fixed(
        fixer,
        fixture.revision,
        vec![evidence_id(82)],
        response_digest,
        fixture.limits,
    )
    .unwrap();
    state = decide(
        &state,
        &command(&state, 4, ReviewCommandKind::RecordFixerResponse { finding_id, response }),
    )
    .unwrap()
    .into_state();
    assert!(!state.finding(finding_id).unwrap().is_conserved());
    let confirmer = fixture.assignment(&binding, 2, 5, 5, true);
    state = decide(
        &state,
        &command(&state, 5, ReviewCommandKind::AssignReviewer { assignment: confirmer.clone() }),
    )
    .unwrap()
    .into_state();
    state = decide(
        &state,
        &command(
            &state,
            6,
            ReviewCommandKind::ConfirmResolution {
                finding_id,
                reviewer_cycle: confirmer.cycle_id(),
                pending_response_digest: response_digest,
                evidence: vec![evidence_id(83)],
                confirmation_digest: digest(84),
            },
        ),
    )
    .unwrap()
    .into_state();
    assert!(state.finding(finding_id).unwrap().is_conserved());
    state =
        decide(&state, &command(&state, 7, ReviewCommandKind::FinalizeRun)).unwrap().into_state();
    assert_eq!(state.terminal().unwrap().kind(), ReviewTerminalKind::Completed);
}

#[test]
fn external_waiver_is_consumed_exactly_and_revision_advance_resets_current_truth() {
    let fixture = Fixture::new(1, 3, true);
    let binding = fixture.binding(90);
    let (mut state, finding_id) = state_with_one_finding(&fixture, &binding);
    let request_id = ApprovalRequestId::new(bytes(90)).unwrap();
    let request_digest = digest(91);
    let request = FixerResponse::waiver_requested(
        ActorId::new(bytes(89)).unwrap(),
        fixture.revision,
        request_id,
        fixture.waiver_authority,
        fixture.waiver_evidence,
        request_digest,
    );
    state = decide(
        &state,
        &command(&state, 4, ReviewCommandKind::RequestWaiver { finding_id, request }),
    )
    .unwrap()
    .into_state();
    let wrong = ObservedWaiver::from_external(
        WaiverObservation::new(
            finding_id,
            fixture.revision,
            request_id,
            fixture.waiver_authority,
            fixture.waiver_evidence,
            digest(92),
        ),
        digest(1),
    );
    assert!(
        decide(&state, &command(&state, 5, ReviewCommandKind::ObserveWaiver { waiver: wrong }))
            .is_err()
    );
    let exact = ObservedWaiver::from_external(
        WaiverObservation::new(
            finding_id,
            fixture.revision,
            request_id,
            fixture.waiver_authority,
            fixture.waiver_evidence,
            digest(92),
        ),
        request_digest,
    );
    state = decide(&state, &command(&state, 5, ReviewCommandKind::ObserveWaiver { waiver: exact }))
        .unwrap()
        .into_state();
    assert!(state.finding(finding_id).unwrap().is_conserved());

    let next_revision = RevisionTuple::new(
        fixture.revision.acceptance_spec_id(),
        fixture.revision.harness_id(),
        fixture.revision.workspace_id(),
        fixture.revision.workspace_generation(),
        RevisionNumber::new(2).unwrap(),
        fixture.revision.policy_id(),
        fixture.revision.provider_profile_id(),
    );
    let next = ReviewBinding::from_contract(
        &fixture.contract,
        next_revision,
        digest(93),
        digest(94),
        vec![fixture.producer],
        vec![digest(31)],
        fixture.limits,
    )
    .unwrap();
    state =
        decide(&state, &command(&state, 6, ReviewCommandKind::AdvanceRevision { binding: next }))
            .unwrap()
            .into_state();
    assert_eq!(state.quorum().submitted_reviews(), 0);
    assert!(state.unconserved_current_findings().is_empty());
}

#[test]
fn cancellation_budget_exhaustion_and_failure_never_become_completion() {
    let fixture = Fixture::new(1, 3, false);
    let cases = [
        (ReviewCommandKind::CancelRun, ReviewTerminalKind::Cancelled),
        (
            ReviewCommandKind::ExhaustBudget { reason_digest: digest(120) },
            ReviewTerminalKind::NeedsHuman,
        ),
        (ReviewCommandKind::FailRun { failure_digest: digest(121) }, ReviewTerminalKind::Failed),
    ];
    for (kind, expected) in cases {
        let binding = fixture.binding(90);
        let state = start(&genesis(&fixture, binding, 1)).unwrap().into_state();
        let state = decide(&state, &command(&state, 2, kind)).unwrap().into_state();
        assert_eq!(state.terminal().unwrap().kind(), expected);
        assert_ne!(state.terminal().unwrap().kind(), ReviewTerminalKind::Completed);
    }
}

fn state_with_one_finding(
    fixture: &Fixture,
    binding: &ReviewBinding,
) -> (ReviewRunState, FindingId) {
    let mut state = start(&genesis(fixture, binding.clone(), 1)).unwrap().into_state();
    let assignment = fixture.assignment(binding, 1, 2, 2, true);
    state = decide(
        &state,
        &command(&state, 2, ReviewCommandKind::AssignReviewer { assignment: assignment.clone() }),
    )
    .unwrap()
    .into_state();
    let finding_id = FindingId::new(bytes(70)).unwrap();
    let finding = Finding::new(
        finding_id,
        FindingSource::new(assignment.cycle_id(), assignment.reviewer().actor_id()),
        fixture.category_a,
        FindingSeverity::High,
        FindingSeverity::High,
        Confidence::new(8_000).unwrap(),
        vec![fixture.requirement],
        vec![FindingLocation::new("src/state.rs".to_owned(), 2, 1, 2, 4, fixture.limits).unwrap()],
        vec![evidence_id(71)],
        "blocking finding".to_owned(),
        "reproduce finding".to_owned(),
        "expected behavior".to_owned(),
        "apply remediation".to_owned(),
        fixture.revision,
        fixture.limits,
    )
    .unwrap();
    let submission = ReviewSubmission::new(
        assignment.cycle_id(),
        fixture.revision,
        vec![fixture.category_a, fixture.category_b],
        vec![finding],
        FindingSeverity::High,
        fixture.limits,
    )
    .unwrap();
    state = decide(&state, &command(&state, 3, ReviewCommandKind::SubmitReview { submission }))
        .unwrap()
        .into_state();
    (state, finding_id)
}

fn genesis(fixture: &Fixture, binding: ReviewBinding, seed: u8) -> ReviewCommand {
    ReviewCommand::new(
        CommandId::new(bytes(seed)).unwrap(),
        EventId::new(bytes(seed)).unwrap(),
        RunId::new(bytes(99)).unwrap(),
        0,
        None,
        digest(0),
        binding.revision(),
        ReviewCommandKind::StartRun { binding, limits: fixture.limits },
    )
    .unwrap()
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

const fn dimensions() -> [QuorumDimension; 9] {
    [
        QuorumDimension::SubmittedReviewCount,
        QuorumDimension::RequiredCategoryCoverage,
        QuorumDimension::DistinctReviewerIdentities,
        QuorumDimension::ProducerIndependence,
        QuorumDimension::DistinctContexts,
        QuorumDimension::DistinctModelFamilies,
        QuorumDimension::DistinctProviders,
        QuorumDimension::NoSharedAncestry,
        QuorumDimension::FreshContext,
    ]
}

fn limits() -> ReviewLimits {
    ReviewLimits::new(
        16, 16, 16, 128, 16, 16, 16, 32, 16, 32, 256, 4_096, 4_096, 1_048_576, 4_194_304,
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
