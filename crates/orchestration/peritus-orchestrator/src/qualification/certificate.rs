//! Exact B2 certificate construction for the deterministic qualification journey.

use peritus_types::{CommandId, EventId, RevisionTuple, Sha256Digest};
use sha2::{Digest as _, Sha256};

use crate::{AcceptanceCertificate, ChildObservation, KernelAcceptancePlan, OrchestratorState};

use super::domain::digest;
use super::scenario::bytes;

pub(super) fn build(state: &OrchestratorState) -> Result<AcceptanceCertificate, &'static str> {
    let gate = state
        .children()
        .iter()
        .rev()
        .find_map(|child| match child {
            ChildObservation::Gates(value) => Some(value.head().state_digest()),
            _ => None,
        })
        .ok_or("acceptance fixture lacks gate truth")?;
    let review = state
        .children()
        .iter()
        .rev()
        .find_map(|child| match child {
            ChildObservation::Review(value) => Some(value.head().state_digest()),
            _ => None,
        })
        .ok_or("acceptance fixture lacks review truth")?;
    let plan = KernelAcceptancePlan::new(
        CommandId::new(bytes(2_000)).map_err(|_| "construct B0 begin command identity")?,
        EventId::new(bytes(2_001)).map_err(|_| "construct B0 begin event identity")?,
        Some(EventId::new(bytes(2_002)).map_err(|_| "construct prior B0 event identity")?),
        CommandId::new(bytes(2_003)).map_err(|_| "construct B0 evaluate command identity")?,
        EventId::new(bytes(2_004)).map_err(|_| "construct B0 evaluate event identity")?,
    )
    .map_err(|_| "construct B0 acceptance plan")?;
    let binding = state.binding();
    let candidate = state.current_candidate();
    let evidence = digest(2_005);
    let decision = digest(2_006);
    let request = crate::canonical::evaluation_request_digest(
        binding.contract_id(),
        binding.contract_digest(),
        binding.digest(),
        candidate.revision(),
        candidate.digest(),
        gate,
        review,
        evidence,
    );
    let fields = Fields { gate, review, evidence, decision, request, plan };
    let certificate_digest = certificate_digest(state, &fields)?;
    AcceptanceCertificate::from_wire(
        binding.contract_id(),
        binding.contract_digest(),
        binding.digest(),
        candidate.revision(),
        candidate.digest(),
        fields.gate,
        fields.review,
        fields.evidence,
        fields.request,
        fields.decision,
        binding.contract_gate_cycles(),
        binding.contract_review_cycles(),
        fields.plan,
        certificate_digest,
    )
    .map_err(|_| "construct exact B2 acceptance certificate")
}

struct Fields {
    gate: Sha256Digest,
    review: Sha256Digest,
    evidence: Sha256Digest,
    decision: Sha256Digest,
    request: Sha256Digest,
    plan: KernelAcceptancePlan,
}

fn certificate_digest(
    state: &OrchestratorState,
    fields: &Fields,
) -> Result<Sha256Digest, &'static str> {
    let binding = state.binding();
    let candidate = state.current_candidate();
    let mut hash = Sha256::new();
    hash.update(b"peritus.orchestrator.acceptance-certificate.v1\0");
    hash.update(binding.contract_id().as_bytes());
    hash.update(binding.contract_digest().as_bytes());
    hash.update(binding.digest().as_bytes());
    hash_revision(&mut hash, candidate.revision());
    for item in [
        candidate.digest(),
        fields.gate,
        fields.review,
        fields.evidence,
        fields.request,
        fields.decision,
    ] {
        hash.update(item.as_bytes());
    }
    hash.update(binding.contract_gate_cycles().to_be_bytes());
    hash.update(binding.contract_review_cycles().to_be_bytes());
    hash.update(fields.plan.begin_command_id().as_bytes());
    hash.update(fields.plan.begin_event_id().as_bytes());
    hash.update([1]);
    let predecessor = fields
        .plan
        .expected_previous_kernel_event()
        .ok_or("qualification B0 plan lacks its required predecessor")?;
    hash.update(predecessor.as_bytes());
    hash.update(fields.plan.evaluate_command_id().as_bytes());
    hash.update(fields.plan.evaluate_event_id().as_bytes());
    Ok(Sha256Digest::new(hash.finalize().into()))
}

fn hash_revision(hash: &mut Sha256, value: RevisionTuple) {
    hash.update(value.acceptance_spec_id().as_bytes());
    hash.update(value.harness_id().as_bytes());
    hash.update(value.workspace_id().as_bytes());
    hash.update(value.workspace_generation().get().to_be_bytes());
    hash.update(value.workspace_revision().get().to_be_bytes());
    hash.update(value.policy_id().as_bytes());
    hash.update(value.provider_profile_id().as_bytes());
}
