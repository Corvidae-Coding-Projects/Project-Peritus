//! Acceptance certificate and terminal fact codecs.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};

use crate::{AcceptanceCertificate, KernelAcceptancePlan, OrchestratorTerminal};

pub fn write_certificate(
    writer: &mut CanonicalWriter,
    value: &AcceptanceCertificate,
) -> Result<(), CodecError> {
    super::write_id(writer, value.contract_id().as_bytes())?;
    for digest in [value.contract_digest(), value.orchestrator_binding_digest()] {
        super::write_digest(writer, digest)?;
    }
    super::write_revision(writer, value.revision())?;
    for digest in [
        value.candidate_binding_digest(),
        value.gate_state_digest(),
        value.review_state_digest(),
        value.evidence_digest(),
        value.evaluation_request_digest(),
        value.decision_digest(),
    ] {
        super::write_digest(writer, digest)?;
    }
    writer.write_u16(value.maximum_gate_attempts())?;
    writer.write_u16(value.maximum_review_cycles())?;
    let plan = value.kernel_plan();
    super::write_id(writer, plan.begin_command_id().as_bytes())?;
    super::write_id(writer, plan.begin_event_id().as_bytes())?;
    super::write_event_option(writer, plan.expected_previous_kernel_event())?;
    super::write_id(writer, plan.evaluate_command_id().as_bytes())?;
    super::write_id(writer, plan.evaluate_event_id().as_bytes())?;
    super::write_digest(writer, value.digest())
}

pub fn read_certificate(
    reader: &mut CanonicalReader<'_>,
) -> Result<AcceptanceCertificate, CodecError> {
    let contract_id = super::read_acceptance_id(reader)?;
    let contract_digest = super::read_digest(reader)?;
    let binding_digest = super::read_digest(reader)?;
    let revision = super::read_revision(reader)?;
    let candidate = super::read_digest(reader)?;
    let gate = super::read_digest(reader)?;
    let review = super::read_digest(reader)?;
    let evidence = super::read_digest(reader)?;
    let evaluation_request = super::read_digest(reader)?;
    let decision = super::read_digest(reader)?;
    let gates = reader.read_u16()?;
    let reviews = reader.read_u16()?;
    let plan = KernelAcceptancePlan::new(
        super::read_command_id(reader)?,
        super::read_event_id(reader)?,
        super::read_event_option(reader)?,
        super::read_command_id(reader)?,
        super::read_event_id(reader)?,
    )
    .map_err(|_| super::invalid(reader))?;
    AcceptanceCertificate::from_wire(
        contract_id,
        contract_digest,
        binding_digest,
        revision,
        candidate,
        gate,
        review,
        evidence,
        evaluation_request,
        decision,
        gates,
        reviews,
        plan,
        super::read_digest(reader)?,
    )
    .map_err(|_| super::invalid(reader))
}

pub fn write_terminal(
    writer: &mut CanonicalWriter,
    value: OrchestratorTerminal,
) -> Result<(), CodecError> {
    writer.write_u8(super::terminal_kind_tag(value.kind()))?;
    writer.write_u8(super::terminal_cause_tag(value.cause()))?;
    super::write_digest(writer, value.cause_digest())?;
    super::write_revision(writer, value.revision())?;
    super::write_digest(writer, value.digest())
}

pub fn read_terminal(reader: &mut CanonicalReader<'_>) -> Result<OrchestratorTerminal, CodecError> {
    let value = OrchestratorTerminal::from_wire(
        super::read_terminal_kind(reader)?,
        super::read_terminal_cause(reader)?,
        super::read_digest(reader)?,
        super::read_revision(reader)?,
        super::read_digest(reader)?,
    );
    value.validate().map_err(|_| super::invalid(reader))?;
    Ok(value)
}
