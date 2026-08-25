//! Canonical family-76 E0 command codec.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};

use crate::{FixerCompletion, OrchestratorCommand, OrchestratorCommandKind};

/// Canonical family-76 schema-v1 orchestrator command frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorCommandFrame(OrchestratorCommand);

impl OrchestratorCommandFrame {
    /// Copies a checked command into its canonical transport frame.
    #[must_use]
    pub fn from_command(command: &OrchestratorCommand) -> Self {
        Self(command.clone())
    }
    /// Consumes the frame and returns its checked command.
    #[must_use]
    pub fn into_command(self) -> OrchestratorCommand {
        self.0
    }
}

impl CanonicalEncode for OrchestratorCommandFrame {
    const FAMILY: u16 = 76;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let value = &self.0;
        crate::canonical::wire::write_id(writer, value.command_id().as_bytes())?;
        crate::canonical::wire::write_id(writer, value.event_id().as_bytes())?;
        crate::canonical::wire::write_id(writer, value.run_id().as_bytes())?;
        writer.write_u64(value.expected_sequence())?;
        crate::canonical::wire::write_event_option(writer, value.expected_previous_event())?;
        crate::canonical::wire::write_digest(writer, value.prior_state_digest())?;
        crate::canonical::wire::write_revision(writer, value.revision())?;
        write_kind(writer, value.kind())
    }
}

impl CanonicalDecode for OrchestratorCommandFrame {
    const FAMILY: u16 = 76;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = crate::canonical::wire::read_command_id(reader)?;
        let event_id = crate::canonical::wire::read_event_id(reader)?;
        let run_id = crate::canonical::wire::read_run_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let previous = crate::canonical::wire::read_event_option(reader)?;
        if (expected_sequence == 0) != previous.is_none() {
            return Err(crate::canonical::wire::invalid(reader));
        }
        Ok(Self(OrchestratorCommand::from_wire(
            command_id,
            event_id,
            run_id,
            expected_sequence,
            previous,
            crate::canonical::wire::read_digest(reader)?,
            crate::canonical::wire::read_revision(reader)?,
            read_kind(reader)?,
        )))
    }
}

pub(super) fn write_kind(
    writer: &mut CanonicalWriter,
    kind: &OrchestratorCommandKind,
) -> Result<(), CodecError> {
    use OrchestratorCommandKind as K;
    match kind {
        K::Start { genesis } => {
            writer.write_u8(1)?;
            crate::canonical::wire::domain::write_binding(writer, genesis.binding())?;
            crate::canonical::wire::domain::write_candidate(writer, genesis.candidate())?;
            crate::canonical::wire::domain::write_ownership(writer, genesis.ownership())?;
            crate::canonical::wire::domain::write_handoff(writer, genesis.writer_handoff())
        }
        K::PublishDirective { directive } => {
            writer.write_u8(2)?;
            crate::canonical::wire::domain::write_directive(writer, directive)
        }
        K::AcknowledgeDirective { directive_id } => {
            writer.write_u8(3)?;
            crate::canonical::wire::write_id(writer, directive_id.as_bytes())
        }
        K::ObserveHandoffActivation { activation } => {
            writer.write_u8(4)?;
            crate::canonical::wire::observation::write_activation(writer, activation)
        }
        K::ObserveWriter { observation, candidate, quality_cycle } => {
            writer.write_u8(5)?;
            crate::canonical::wire::observation::write_observation(
                writer,
                &crate::ChildObservation::Agent(observation.clone()),
            )?;
            write_candidate_option(writer, candidate.as_ref())?;
            write_quality_cycle_option(writer, quality_cycle.as_ref())
        }
        K::ObserveGates { observation, review_handoff } => {
            writer.write_u8(6)?;
            crate::canonical::wire::observation::write_gate(writer, observation)?;
            write_handoff_option(writer, review_handoff.as_ref())
        }
        K::ObserveReview { observation, fixer_handoff } => {
            writer.write_u8(7)?;
            crate::canonical::wire::observation::write_review(writer, observation)?;
            write_handoff_option(writer, fixer_handoff.as_ref())
        }
        K::ObserveFixer { completion } => {
            writer.write_u8(8)?;
            write_fixer(writer, completion)
        }
        K::ObserveRoleInfrastructure { scheduler, collaboration } => {
            writer.write_u8(9)?;
            crate::canonical::wire::observation::write_observation(
                writer,
                &crate::ChildObservation::Scheduler(scheduler.clone()),
            )?;
            crate::canonical::wire::observation::write_observation(
                writer,
                &crate::ChildObservation::Collaboration(collaboration.clone()),
            )
        }
        K::AdvanceCandidate { quality_cycle } => {
            writer.write_u8(10)?;
            crate::canonical::wire::domain::write_quality_cycle(writer, quality_cycle)
        }
        K::RecordAcceptanceCertificate { certificate } => {
            writer.write_u8(11)?;
            crate::canonical::wire::domain::write_certificate(writer, certificate)
        }
        K::ObserveKernelAcceptance { observation } => {
            writer.write_u8(12)?;
            crate::canonical::wire::observation::write_kernel(writer, *observation)
        }
        K::Pause { reconciliation } => {
            writer.write_u8(13)?;
            crate::canonical::wire::domain::write_reconciliation(writer, reconciliation)
        }
        K::Resume { reconciliation } => {
            writer.write_u8(14)?;
            crate::canonical::wire::domain::write_reconciliation(writer, reconciliation)
        }
        K::Cancel { cause_digest } => write_digest_kind(writer, 15, *cause_digest),
        K::ReconcileCancellation { observation } => {
            writer.write_u8(16)?;
            crate::canonical::wire::observation::write_observation(writer, observation)
        }
        K::Reject { cause_digest } => write_digest_kind(writer, 17, *cause_digest),
        K::Fail { cause_digest } => write_digest_kind(writer, 18, *cause_digest),
        K::Exhaust { cause_digest } => write_digest_kind(writer, 19, *cause_digest),
        K::Finalize => writer.write_u8(20),
    }
}

pub(super) fn read_kind(
    reader: &mut CanonicalReader<'_>,
) -> Result<OrchestratorCommandKind, CodecError> {
    use OrchestratorCommandKind as K;
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => {
            let binding = crate::canonical::wire::domain::read_binding(reader)?;
            let limits = binding.limits();
            Ok(K::Start {
                genesis: Box::new(crate::OrchestratorGenesis::new(
                    binding,
                    crate::canonical::wire::domain::read_candidate(reader, limits)?,
                    crate::canonical::wire::domain::read_ownership(reader, limits)?,
                    crate::canonical::wire::domain::read_handoff(reader, limits)?,
                )),
            })
        }
        2 => Ok(K::PublishDirective {
            directive: crate::canonical::wire::domain::read_directive(reader)?,
        }),
        3 => Ok(K::AcknowledgeDirective { directive_id: read_directive_id(reader)? }),
        4 => Ok(K::ObserveHandoffActivation {
            activation: crate::canonical::wire::observation::read_activation(reader)?,
        }),
        5 => {
            let crate::ChildObservation::Agent(observation) =
                crate::canonical::wire::observation::read_observation(reader)?
            else {
                return Err(crate::canonical::wire::invalid(reader));
            };
            Ok(K::ObserveWriter {
                observation,
                candidate: read_candidate_option(reader)?,
                quality_cycle: read_quality_cycle_option(reader)?,
            })
        }
        6 => Ok(K::ObserveGates {
            observation: crate::canonical::wire::observation::read_gate(reader)?,
            review_handoff: read_handoff_option(reader)?,
        }),
        7 => Ok(K::ObserveReview {
            observation: crate::canonical::wire::observation::read_review(reader)?,
            fixer_handoff: read_handoff_option(reader)?,
        }),
        8 => Ok(K::ObserveFixer { completion: read_fixer(reader)? }),
        9 => {
            let crate::ChildObservation::Scheduler(scheduler) =
                crate::canonical::wire::observation::read_observation(reader)?
            else {
                return Err(crate::canonical::wire::invalid(reader));
            };
            let crate::ChildObservation::Collaboration(collaboration) =
                crate::canonical::wire::observation::read_observation(reader)?
            else {
                return Err(crate::canonical::wire::invalid(reader));
            };
            Ok(K::ObserveRoleInfrastructure { scheduler, collaboration })
        }
        10 => Ok(K::AdvanceCandidate {
            quality_cycle: crate::canonical::wire::domain::read_quality_cycle(reader)?,
        }),
        11 => Ok(K::RecordAcceptanceCertificate {
            certificate: crate::canonical::wire::domain::read_certificate(reader)?,
        }),
        12 => Ok(K::ObserveKernelAcceptance {
            observation: crate::canonical::wire::observation::read_kernel(reader)?,
        }),
        13 => Ok(K::Pause {
            reconciliation: crate::canonical::wire::domain::read_reconciliation(reader)?,
        }),
        14 => Ok(K::Resume {
            reconciliation: crate::canonical::wire::domain::read_reconciliation(reader)?,
        }),
        15 => Ok(K::Cancel { cause_digest: crate::canonical::wire::read_digest(reader)? }),
        16 => Ok(K::ReconcileCancellation {
            observation: crate::canonical::wire::observation::read_observation(reader)?,
        }),
        17 => Ok(K::Reject { cause_digest: crate::canonical::wire::read_digest(reader)? }),
        18 => Ok(K::Fail { cause_digest: crate::canonical::wire::read_digest(reader)? }),
        19 => Ok(K::Exhaust { cause_digest: crate::canonical::wire::read_digest(reader)? }),
        20 => Ok(K::Finalize),
        _ => Err(crate::canonical::wire::unknown(offset)),
    }
}

pub(super) fn write_fixer(
    writer: &mut CanonicalWriter,
    value: &FixerCompletion,
) -> Result<(), CodecError> {
    crate::canonical::wire::observation::write_observation(
        writer,
        &crate::ChildObservation::Agent(value.observation().clone()),
    )?;
    writer.write_option_tag(value.proposed_candidate().is_some())?;
    if let Some(candidate) = value.proposed_candidate() {
        crate::canonical::wire::domain::write_candidate(writer, candidate)?;
    }
    writer.write_option_tag(value.review_observation().is_some())?;
    if let Some(review) = value.review_observation() {
        crate::canonical::wire::observation::write_observation(
            writer,
            &crate::ChildObservation::ReviewFixer(review.clone()),
        )?;
    }
    Ok(())
}

pub(super) fn read_fixer(reader: &mut CanonicalReader<'_>) -> Result<FixerCompletion, CodecError> {
    let crate::ChildObservation::Agent(observation) =
        crate::canonical::wire::observation::read_observation(reader)?
    else {
        return Err(crate::canonical::wire::invalid(reader));
    };
    let candidate = reader
        .read_option_tag()?
        .then(|| {
            crate::canonical::wire::domain::read_candidate(
                reader,
                crate::canonical::wire::domain::wire_limits(),
            )
        })
        .transpose()?;
    let review = if reader.read_option_tag()? {
        let crate::ChildObservation::ReviewFixer(value) =
            crate::canonical::wire::observation::read_observation(reader)?
        else {
            return Err(crate::canonical::wire::invalid(reader));
        };
        Some(value)
    } else {
        None
    };
    Ok(FixerCompletion::from_wire(observation, candidate, review))
}

pub(super) fn write_handoff_option(
    writer: &mut CanonicalWriter,
    value: Option<&crate::Handoff>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        crate::canonical::wire::domain::write_handoff(writer, value)?;
    }
    Ok(())
}

pub(super) fn write_candidate_option(
    writer: &mut CanonicalWriter,
    value: Option<&crate::CandidateBinding>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        crate::canonical::wire::domain::write_candidate(writer, value)?;
    }
    Ok(())
}

pub(super) fn write_quality_cycle_option(
    writer: &mut CanonicalWriter,
    value: Option<&crate::QualityCycleBinding>,
) -> Result<(), CodecError> {
    match value {
        Some(value) => {
            writer.write_u8(1)?;
            crate::canonical::wire::domain::write_quality_cycle(writer, value)
        }
        None => writer.write_u8(0),
    }
}

pub(super) fn read_quality_cycle_option(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<crate::QualityCycleBinding>, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        0 => Ok(None),
        1 => crate::canonical::wire::domain::read_quality_cycle(reader).map(Some),
        _ => Err(crate::canonical::wire::unknown(offset)),
    }
}

pub(super) fn read_candidate_option(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<crate::CandidateBinding>, CodecError> {
    reader
        .read_option_tag()?
        .then(|| {
            crate::canonical::wire::domain::read_candidate(
                reader,
                crate::canonical::wire::domain::wire_limits(),
            )
        })
        .transpose()
}

pub(super) fn read_handoff_option(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<crate::Handoff>, CodecError> {
    reader
        .read_option_tag()?
        .then(|| {
            crate::canonical::wire::domain::read_handoff(
                reader,
                crate::canonical::wire::domain::wire_limits(),
            )
        })
        .transpose()
}

fn write_digest_kind(
    writer: &mut CanonicalWriter,
    tag: u8,
    digest: peritus_types::Sha256Digest,
) -> Result<(), CodecError> {
    writer.write_u8(tag)?;
    crate::canonical::wire::write_digest(writer, digest)
}

fn read_directive_id(reader: &mut CanonicalReader<'_>) -> Result<crate::DirectiveId, CodecError> {
    let offset = reader.offset();
    crate::DirectiveId::new(reader.read_fixed()?)
        .map_err(|_| crate::canonical::wire::invalid_at(offset))
}
