//! Explicit domain-separated hashes for authority-bearing E0 records.

use peritus_agent::{CompletionProposal, CompletionRequest};
use peritus_quality_policy::{
    AcceptanceDecision, AcceptanceEvidence, ApprovalOutcome, ApprovalSubject, FindingDisposition,
    GateFailure, GateOutcome,
};
use peritus_spec::FindingSeverity;
use peritus_types::{RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::directive::DirectivePayloadBinding;
use crate::{AcceptanceCertificate, ChildAggregateKind, DirectiveDestination, DirectiveKind};

#[must_use]
/// Hashes complete semantic E0 state while logically zeroing the stored digest field.
pub fn state_digest(value: &crate::OrchestratorState) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.state.v1\0");
    match crate::wire::state::canonical_state_bytes(value) {
        Ok(bytes) => hash.update(bytes),
        Err(error) => {
            hash.update(b"canonical-encoding-error\0");
            hash.update(u64::try_from(error.offset()).unwrap_or(u64::MAX).to_be_bytes());
        }
    }
    finish(hash)
}

#[must_use]
/// Hashes the complete D0 completion proposal retained by E0.
pub fn completion_digest(value: &CompletionProposal) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.agent-completion.v1\0");
    hash_bytes(&mut hash, value.summary().as_str().as_bytes());
    hash_count(&mut hash, value.evidence().len());
    for evidence in value.evidence() {
        hash.update(evidence.id().as_bytes());
        hash_revision(&mut hash, evidence.revision());
    }
    hash_count(&mut hash, value.uncertainties().len());
    for uncertainty in value.uncertainties() {
        hash_bytes(&mut hash, uncertainty.as_str().as_bytes());
    }
    hash_revision(&mut hash, value.revision());
    let transcripts = value.transcripts();
    hash.update(transcripts.context().as_bytes());
    hash.update(transcripts.model().as_bytes());
    hash.update(transcripts.tools().as_bytes());
    hash.update([completion_request_tag(value.requested())]);
    finish(hash)
}

#[must_use]
/// Hashes every B2 acceptance evidence collection and exact item field.
pub fn acceptance_evidence_digest(value: &AcceptanceEvidence) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.acceptance-evidence.v1\0");
    hash_count(&mut hash, value.gates().len());
    for gate in value.gates() {
        hash.update(gate.execution_id().as_bytes());
        hash.update(gate.gate_id().as_bytes());
        hash.update(gate.attempt().get().to_be_bytes());
        hash_revision(&mut hash, gate.revision());
        hash.update([gate_outcome_tag(gate.outcome())]);
        hash.update(gate.result_digest().as_bytes());
    }
    hash_count(&mut hash, value.reviews().len());
    for review in value.reviews() {
        hash.update(review.cycle_id().as_bytes());
        hash.update(review.cycle_ordinal().get().to_be_bytes());
        hash_revision(&mut hash, review.revision());
        let reviewer = review.reviewer();
        hash.update(reviewer.actor_id().as_bytes());
        hash.update(reviewer.provider().as_bytes());
        hash.update(reviewer.model_family().as_bytes());
        hash.update(reviewer.prompt_revision().as_bytes());
        hash.update(reviewer.context().as_bytes());
        hash.update(reviewer.ancestry().as_bytes());
        hash.update([u8::from(reviewer.independent_from_producer())]);
        hash_count(&mut hash, review.categories().len());
        for category in review.categories() {
            hash.update(category.digest().as_bytes());
        }
        hash_count(&mut hash, review.findings().len());
        for finding in review.findings() {
            hash.update(finding.finding_id().as_bytes());
            hash.update([severity_tag(finding.severity())]);
            hash_finding_disposition(&mut hash, finding.disposition());
            hash.update(finding.finding_digest().as_bytes());
        }
        hash.update(review.review_digest().as_bytes());
    }
    hash_count(&mut hash, value.evidence().len());
    for evidence in value.evidence() {
        hash.update(evidence.requirement_id().digest().as_bytes());
        hash_revision(&mut hash, evidence.revision());
        hash.update(evidence.artifact_digest().as_bytes());
    }
    hash_count(&mut hash, value.approvals().len());
    for approval in value.approvals() {
        hash.update(approval.request_id().as_bytes());
        hash_revision(&mut hash, approval.revision());
        match approval.subject() {
            ApprovalSubject::Acceptance => hash.update([1]),
            ApprovalSubject::FindingWaiver(finding) => {
                hash.update([2]);
                hash.update(finding.as_bytes());
            }
        }
        hash.update(approval.actor_id().as_bytes());
        hash.update(approval.authority().digest().as_bytes());
        hash.update([match approval.outcome() {
            ApprovalOutcome::Approved => 1,
            ApprovalOutcome::Denied => 2,
        }]);
        hash.update(approval.evidence_digest().as_bytes());
    }
    hash_count(&mut hash, value.waivers().len());
    for waiver in value.waivers() {
        hash.update(waiver.finding_id().as_bytes());
        hash_revision(&mut hash, waiver.revision());
        hash.update(waiver.approval_request_id().as_bytes());
        hash.update(waiver.authority().digest().as_bytes());
        hash.update(waiver.evidence_requirement_id().digest().as_bytes());
        hash.update(waiver.waiver_digest().as_bytes());
    }
    finish(hash)
}

#[must_use]
/// Hashes the private B2 decision through its public immutable observations.
pub fn acceptance_decision_digest(value: &AcceptanceDecision) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.acceptance-decision.v1\0");
    hash.update([u8::from(value.is_acceptable())]);
    hash_count(&mut hash, value.unmet_conditions().len());
    hash.update(value.gate_attempt_limit().to_be_bytes());
    hash.update(value.review_cycle_limit().to_be_bytes());
    finish(hash)
}

#[must_use]
/// Hashes the complete checked acceptance certificate.
pub fn certificate_digest(value: &AcceptanceCertificate) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.acceptance-certificate.v1\0");
    hash.update(value.contract_id().as_bytes());
    hash.update(value.contract_digest().as_bytes());
    hash.update(value.orchestrator_binding_digest().as_bytes());
    hash_revision(&mut hash, value.revision());
    hash.update(value.candidate_binding_digest().as_bytes());
    hash.update(value.gate_state_digest().as_bytes());
    hash.update(value.review_state_digest().as_bytes());
    hash.update(value.evidence_digest().as_bytes());
    hash.update(value.evaluation_request_digest().as_bytes());
    hash.update(value.decision_digest().as_bytes());
    hash.update(value.maximum_gate_attempts().to_be_bytes());
    hash.update(value.maximum_review_cycles().to_be_bytes());
    let plan = value.kernel_plan();
    hash.update(plan.begin_command_id().as_bytes());
    hash.update(plan.begin_event_id().as_bytes());
    match plan.expected_previous_kernel_event() {
        Some(event) => {
            hash.update([1]);
            hash.update(event.as_bytes());
        }
        None => hash.update([0]),
    }
    hash.update(plan.evaluate_command_id().as_bytes());
    hash.update(plan.evaluate_event_id().as_bytes());
    finish(hash)
}

#[must_use]
#[allow(clippy::too_many_arguments, reason = "complete B2 request binding remains explicit")]
/// Hashes the exact B2 evaluation request independently of its returned decision.
pub fn evaluation_request_digest(
    contract_id: peritus_types::AcceptanceSpecId,
    contract_digest: Sha256Digest,
    orchestrator_binding_digest: Sha256Digest,
    revision: RevisionTuple,
    candidate_binding_digest: Sha256Digest,
    gate_state_digest: Sha256Digest,
    review_state_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.acceptance-evaluation-request.v1\0");
    hash.update(contract_id.as_bytes());
    hash.update(contract_digest.as_bytes());
    hash.update(orchestrator_binding_digest.as_bytes());
    hash_revision(&mut hash, revision);
    hash.update(candidate_binding_digest.as_bytes());
    hash.update(gate_state_digest.as_bytes());
    hash.update(review_state_digest.as_bytes());
    hash.update(evidence_digest.as_bytes());
    finish(hash)
}

#[must_use]
/// Hashes one planned B0 acceptance envelope and its certificate.
pub fn kernel_directive_payload_digest(value: &AcceptanceCertificate, begin: bool) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.kernel-directive.v1\0");
    hash.update([if begin { 1 } else { 2 }]);
    hash.update(value.digest().as_bytes());
    hash_revision(&mut hash, value.revision());
    let plan = value.kernel_plan();
    if begin {
        hash.update(plan.begin_command_id().as_bytes());
        hash.update(plan.begin_event_id().as_bytes());
        match plan.expected_previous_kernel_event() {
            Some(event) => {
                hash.update([1]);
                hash.update(event.as_bytes());
            }
            None => hash.update([0]),
        }
    } else {
        hash.update(plan.evaluate_command_id().as_bytes());
        hash.update(plan.evaluate_event_id().as_bytes());
        hash.update(plan.evaluate_previous_event_id().as_bytes());
    }
    finish(hash)
}

#[must_use]
/// Hashes one non-B2/non-B0 directive's exact state-bound payload.
pub fn directive_payload_digest(
    kind: DirectiveKind,
    destination: DirectiveDestination,
    binding: DirectivePayloadBinding<'_>,
) -> Sha256Digest {
    let mut hash = domain(b"peritus.orchestrator.directive-payload.v1\0");
    hash.update([directive_kind_tag(kind), destination_tag(destination)]);
    match binding {
        DirectivePayloadBinding::Handoff(handoff) => hash.update(handoff.digest().as_bytes()),
        DirectivePayloadBinding::QualityCycle(cycle) => hash.update(cycle.digest().as_bytes()),
        DirectivePayloadBinding::Reconciliation(reconciliation) => {
            hash.update(reconciliation.checkpoint_state_digest().as_bytes());
            hash_count(&mut hash, reconciliation.child_heads().len());
            for head in reconciliation.child_heads() {
                hash.update([child_aggregate_tag(head.aggregate())]);
                hash.update(head.sequence().get().to_be_bytes());
                hash.update(head.last_event_id().as_bytes());
                hash.update(head.state_digest().as_bytes());
            }
        }
        DirectivePayloadBinding::Cancellation(cause) => hash.update(cause.as_bytes()),
    }
    finish(hash)
}

fn hash_finding_disposition(hash: &mut Sha256, value: FindingDisposition) {
    match value {
        FindingDisposition::Open => hash.update([1]),
        FindingDisposition::Resolved { revision, evidence_digest } => {
            hash.update([2]);
            hash_revision(hash, revision);
            hash.update(evidence_digest.as_bytes());
        }
        FindingDisposition::WaiverRequested => hash.update([3]),
    }
}

fn hash_revision(hash: &mut Sha256, revision: RevisionTuple) {
    hash.update(revision.acceptance_spec_id().as_bytes());
    hash.update(revision.harness_id().as_bytes());
    hash.update(revision.workspace_id().as_bytes());
    hash.update(revision.workspace_generation().get().to_be_bytes());
    hash.update(revision.workspace_revision().get().to_be_bytes());
    hash.update(revision.policy_id().as_bytes());
    hash.update(revision.provider_profile_id().as_bytes());
}

fn hash_bytes(hash: &mut Sha256, bytes: &[u8]) {
    hash_count(hash, bytes.len());
    hash.update(bytes);
}

fn hash_count(hash: &mut Sha256, count: usize) {
    hash.update(u64::try_from(count).unwrap_or(u64::MAX).to_be_bytes());
}

fn domain(bytes: &[u8]) -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash
}
fn finish(hash: Sha256) -> Sha256Digest {
    Sha256Digest::new(hash.finalize().into())
}

const fn directive_kind_tag(value: DirectiveKind) -> u8 {
    match value {
        DirectiveKind::StartWriter => 1,
        DirectiveKind::StartGates => 2,
        DirectiveKind::StartReview => 3,
        DirectiveKind::StartFixer => 4,
        DirectiveKind::EvaluateAcceptance => 5,
        DirectiveKind::FinalizeChildren => 6,
        DirectiveKind::BeginKernelAcceptance => 7,
        DirectiveKind::EvaluateKernelAcceptance => 8,
        DirectiveKind::PauseChildren => 9,
        DirectiveKind::CancelChildren => 10,
    }
}

const fn destination_tag(value: DirectiveDestination) -> u8 {
    match value {
        DirectiveDestination::Scheduler => 1,
        DirectiveDestination::Collaboration => 2,
        DirectiveDestination::Agent => 3,
        DirectiveDestination::Gates => 4,
        DirectiveDestination::Review => 5,
        DirectiveDestination::QualityEvaluator => 6,
        DirectiveDestination::Kernel => 7,
    }
}

const fn child_aggregate_tag(value: ChildAggregateKind) -> u8 {
    match value {
        ChildAggregateKind::Agent => 1,
        ChildAggregateKind::Gates => 2,
        ChildAggregateKind::Review => 3,
        ChildAggregateKind::Scheduler => 4,
        ChildAggregateKind::Collaboration => 5,
        ChildAggregateKind::Kernel => 6,
    }
}

const fn completion_request_tag(value: CompletionRequest) -> u8 {
    match value {
        CompletionRequest::RunGates => 1,
        CompletionRequest::RequestReview => 2,
        CompletionRequest::ContinueFixing => 3,
        CompletionRequest::RequestAuthority => 4,
        CompletionRequest::ReportBlocked => 5,
    }
}

const fn gate_outcome_tag(value: GateOutcome) -> u8 {
    match value {
        GateOutcome::Passed => 1,
        GateOutcome::Failed(GateFailure::PredicateFailed) => 2,
        GateOutcome::Failed(GateFailure::UnsuccessfulExit) => 3,
        GateOutcome::Failed(GateFailure::InvalidResult) => 4,
        GateOutcome::Failed(GateFailure::Infrastructure) => 5,
    }
}

const fn severity_tag(value: FindingSeverity) -> u8 {
    match value {
        FindingSeverity::Advisory => 1,
        FindingSeverity::Low => 2,
        FindingSeverity::Medium => 3,
        FindingSeverity::High => 4,
        FindingSeverity::Critical => 5,
    }
}
