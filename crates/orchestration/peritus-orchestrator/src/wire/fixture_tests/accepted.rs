//! Accepted checkpoint truth requires both exact planned B0 observations.

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use sha2::{Digest, Sha256};

use super::{digest, values};
use crate::child::gates::GateObservationWire;
use crate::{
    AcceptanceCertificate, ChildAggregateKind, ChildHead, ChildObservation, ChildTerminalClass,
    GateChildObservation, GateObservationClass, KernelAcceptanceObservation,
    KernelAcceptanceOutcome, KernelAcceptancePlan, OrchestratorCounters, OrchestratorPhase,
    OrchestratorState, OrchestratorStateFrame, OrchestratorTerminal, ReviewChildObservation,
    ReviewObservationClass, TerminalCause,
};

#[test]
fn accepted_checkpoint_requires_planned_begin_and_evaluate_observations() {
    let (_, _, genesis) = values();
    let certificate = certificate(&genesis);
    let plan = certificate.kernel_plan();
    let begun = ChildObservation::KernelAcceptance(KernelAcceptanceObservation::from_wire(
        plan.begin_event_id(),
        plan.begin_command_id(),
        EventSequence::new(20).unwrap(),
        plan.expected_previous_kernel_event(),
        genesis.binding().run_id(),
        genesis.current_candidate().revision(),
        KernelAcceptanceOutcome::Begun,
    ));
    let accepted = ChildObservation::KernelAcceptance(KernelAcceptanceObservation::from_wire(
        plan.evaluate_event_id(),
        plan.evaluate_command_id(),
        EventSequence::new(21).unwrap(),
        Some(plan.evaluate_previous_event_id()),
        genesis.binding().run_id(),
        genesis.current_candidate().revision(),
        KernelAcceptanceOutcome::Accepted,
    ));

    let quality = quality_observations(&genesis, &certificate);
    let mut complete_observations = quality.clone();
    complete_observations.extend([begun.clone(), accepted.clone()]);
    let complete = accepted_state(&genesis, certificate.clone(), complete_observations);
    complete.validate().unwrap();
    assert_roundtrip(&complete);

    for kernel in [vec![begun], vec![accepted]] {
        let mut observations = quality.clone();
        observations.extend(kernel);
        let incomplete = accepted_state(&genesis, certificate.clone(), observations);
        let bytes = encode_message(
            &OrchestratorStateFrame::from_state(&incomplete),
            CodecLimits::PRODUCTION,
        )
        .unwrap();
        assert!(decode_message::<OrchestratorStateFrame>(&bytes, CodecLimits::PRODUCTION).is_err());
    }
}

fn quality_observations(
    state: &OrchestratorState,
    certificate: &AcceptanceCertificate,
) -> Vec<ChildObservation> {
    let cycle = state.current_quality_cycle();
    let revision = state.current_candidate().revision();
    let gate_head = ChildHead::new(
        ChildAggregateKind::Gates,
        EventSequence::new(18).unwrap(),
        EventId::new([68; 16]).unwrap(),
        certificate.gate_state_digest(),
        Some(ChildTerminalClass::Completed),
    )
    .unwrap();
    let gate = GateChildObservation::from_wire(&GateObservationWire {
        orchestrator_run_id: state.binding().run_id(),
        gate_run_id: cycle.gate_run_id(),
        revision,
        plan_digest: cycle.gate_plan_digest(),
        snapshot_digest: state.current_candidate().quality_snapshot_digest(),
        evidence_digest: digest(69),
        class: GateObservationClass::Passed,
        head: gate_head,
    })
    .unwrap();
    let review_head = ChildHead::new(
        ChildAggregateKind::Review,
        EventSequence::new(19).unwrap(),
        EventId::new([69; 16]).unwrap(),
        certificate.review_state_digest(),
        Some(ChildTerminalClass::Completed),
    )
    .unwrap();
    let review = ReviewChildObservation::from_wire(
        state.binding().run_id(),
        revision,
        cycle.review_binding_digest(),
        true,
        Vec::new(),
        ReviewObservationClass::Completed,
        review_head,
    )
    .unwrap();
    vec![ChildObservation::Gates(gate), ChildObservation::Review(review)]
}

fn assert_roundtrip(state: &OrchestratorState) {
    let bytes = encode_message(&OrchestratorStateFrame::from_state(state), CodecLimits::PRODUCTION)
        .unwrap();
    assert!(decode_message::<OrchestratorStateFrame>(&bytes, CodecLimits::PRODUCTION).is_ok());
}

pub fn certificate(state: &OrchestratorState) -> AcceptanceCertificate {
    let binding = state.binding();
    let candidate = state.current_candidate();
    let fields = CertificateFields {
        contract: binding.contract_digest(),
        binding: binding.digest(),
        candidate: candidate.digest(),
        gate: digest(70),
        review: digest(71),
        evidence: digest(72),
        decision: digest(73),
        plan: KernelAcceptancePlan::new(
            CommandId::new([74; 16]).unwrap(),
            EventId::new([75; 16]).unwrap(),
            Some(EventId::new([76; 16]).unwrap()),
            CommandId::new([77; 16]).unwrap(),
            EventId::new([78; 16]).unwrap(),
        )
        .unwrap(),
    };
    let request = crate::canonical::evaluation_request_digest(
        binding.contract_id(),
        fields.contract,
        fields.binding,
        candidate.revision(),
        fields.candidate,
        fields.gate,
        fields.review,
        fields.evidence,
    );
    let digest = certificate_digest(state, &fields, request);
    AcceptanceCertificate::from_wire(
        binding.contract_id(),
        fields.contract,
        fields.binding,
        candidate.revision(),
        fields.candidate,
        fields.gate,
        fields.review,
        fields.evidence,
        request,
        fields.decision,
        binding.contract_gate_cycles(),
        binding.contract_review_cycles(),
        fields.plan,
        digest,
    )
    .unwrap()
}

struct CertificateFields {
    contract: Sha256Digest,
    binding: Sha256Digest,
    candidate: Sha256Digest,
    gate: Sha256Digest,
    review: Sha256Digest,
    evidence: Sha256Digest,
    decision: Sha256Digest,
    plan: KernelAcceptancePlan,
}

fn certificate_digest(
    state: &OrchestratorState,
    fields: &CertificateFields,
    request: Sha256Digest,
) -> Sha256Digest {
    let binding = state.binding();
    let revision = state.current_candidate().revision();
    let mut hash = Sha256::new();
    hash.update(b"peritus.orchestrator.acceptance-certificate.v1\0");
    hash.update(binding.contract_id().as_bytes());
    hash.update(fields.contract.as_bytes());
    hash.update(fields.binding.as_bytes());
    hash_revision(&mut hash, revision);
    for item in
        [fields.candidate, fields.gate, fields.review, fields.evidence, request, fields.decision]
    {
        hash.update(item.as_bytes());
    }
    hash.update(binding.contract_gate_cycles().to_be_bytes());
    hash.update(binding.contract_review_cycles().to_be_bytes());
    let plan = fields.plan;
    hash.update(plan.begin_command_id().as_bytes());
    hash.update(plan.begin_event_id().as_bytes());
    hash.update([1]);
    hash.update(plan.expected_previous_kernel_event().unwrap().as_bytes());
    hash.update(plan.evaluate_command_id().as_bytes());
    hash.update(plan.evaluate_event_id().as_bytes());
    Sha256Digest::new(hash.finalize().into())
}

fn accepted_state(
    base: &OrchestratorState,
    certificate: AcceptanceCertificate,
    observations: Vec<ChildObservation>,
) -> OrchestratorState {
    let terminal = OrchestratorTerminal::new(
        TerminalCause::KernelAccepted,
        certificate.digest(),
        base.current_candidate().revision(),
    )
    .unwrap();
    let zero = assemble(base, certificate.clone(), observations.clone(), terminal, digest(0));
    let state_digest = crate::canonical::state_digest(&zero);
    assemble(base, certificate, observations, terminal, state_digest)
}

fn assemble(
    base: &OrchestratorState,
    certificate: AcceptanceCertificate,
    observations: Vec<ChildObservation>,
    terminal: OrchestratorTerminal,
    state_digest: Sha256Digest,
) -> OrchestratorState {
    let counters = base.counters();
    OrchestratorState::from_wire(
        base.binding().clone(),
        base.ownership().clone(),
        OrchestratorPhase::Terminal,
        base.sequence(),
        base.last_event_id(),
        state_digest,
        base.current_candidate().clone(),
        base.candidate_history().to_vec(),
        base.current_quality_cycle().clone(),
        base.quality_cycle_history().to_vec(),
        None,
        OrchestratorCounters::from_wire(
            counters.revisions(),
            counters.writer_cycles(),
            counters.fixer_cycles(),
            counters.gate_cycles(),
            counters.review_cycles(),
            counters.handoffs(),
            counters.child_directives(),
            u16::try_from(observations.len()).unwrap(),
            counters.cancellation_reconciliations(),
        ),
        base.handoffs().to_vec(),
        None,
        Vec::new(),
        observations,
        Vec::new(),
        None,
        Some(certificate),
        None,
        base.used_commands().to_vec(),
        Some(terminal),
        None,
        None,
        Vec::new(),
    )
}

fn hash_revision(hash: &mut Sha256, value: peritus_types::RevisionTuple) {
    hash.update(value.acceptance_spec_id().as_bytes());
    hash.update(value.harness_id().as_bytes());
    hash.update(value.workspace_id().as_bytes());
    hash.update(value.workspace_generation().get().to_be_bytes());
    hash.update(value.workspace_revision().get().to_be_bytes());
    hash.update(value.policy_id().as_bytes());
    hash.update(value.provider_profile_id().as_bytes());
}
