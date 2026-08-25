//! Canonical family-77 E0 event codec.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
};

use crate::{OrchestratorEvent, OrchestratorEventKind};

/// Canonical family-77 schema-v1 orchestrator event frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorEventFrame(OrchestratorEvent);

impl OrchestratorEventFrame {
    /// Copies a checked event into its canonical transport frame.
    #[must_use]
    pub fn from_event(event: &OrchestratorEvent) -> Self {
        Self(event.clone())
    }

    /// Consumes the frame and returns its checked event.
    #[must_use]
    pub fn into_event(self) -> OrchestratorEvent {
        self.0
    }
}

impl CanonicalEncode for OrchestratorEventFrame {
    const FAMILY: u16 = 77;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let value = &self.0;
        crate::canonical::wire::write_id(writer, value.id().as_bytes())?;
        crate::canonical::wire::write_id(writer, value.command_id().as_bytes())?;
        writer.write_u64(value.sequence().get())?;
        crate::canonical::wire::write_event_option(writer, value.previous_event())?;
        crate::canonical::wire::write_id(writer, value.run_id().as_bytes())?;
        crate::canonical::wire::write_revision(writer, value.revision())?;
        crate::canonical::wire::write_digest(writer, value.prior_state_digest())?;
        crate::canonical::wire::write_digest(writer, value.successor_state_digest())?;
        write_kind(writer, value.kind())
    }
}

impl CanonicalDecode for OrchestratorEventFrame {
    const FAMILY: u16 = 77;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let id = crate::canonical::wire::read_event_id(reader)?;
        let command = crate::canonical::wire::read_command_id(reader)?;
        let sequence = crate::canonical::wire::read_sequence(reader)?;
        let previous = crate::canonical::wire::read_event_option(reader)?;
        if (sequence.get() == 1) != previous.is_none() {
            return Err(crate::canonical::wire::invalid(reader));
        }
        Ok(Self(OrchestratorEvent::from_wire(
            id,
            command,
            sequence,
            previous,
            crate::canonical::wire::read_run_id(reader)?,
            crate::canonical::wire::read_revision(reader)?,
            crate::canonical::wire::read_digest(reader)?,
            crate::canonical::wire::read_digest(reader)?,
            read_kind(reader)?,
        )))
    }
}

fn write_kind(
    writer: &mut CanonicalWriter,
    kind: &OrchestratorEventKind,
) -> Result<(), CodecError> {
    use OrchestratorEventKind as K;
    match kind {
        K::Started { genesis } => {
            writer.write_u8(1)?;
            crate::canonical::wire::domain::write_binding(writer, genesis.binding())?;
            crate::canonical::wire::domain::write_candidate(writer, genesis.candidate())?;
            crate::canonical::wire::domain::write_ownership(writer, genesis.ownership())?;
            crate::canonical::wire::domain::write_handoff(writer, genesis.writer_handoff())
        }
        K::DirectivePublished { directive } => {
            writer.write_u8(2)?;
            crate::canonical::wire::domain::write_directive(writer, directive)
        }
        K::DirectiveAcknowledged { directive_id } => {
            writer.write_u8(3)?;
            crate::canonical::wire::write_id(writer, directive_id.as_bytes())
        }
        K::HandoffActivated { activation } => {
            writer.write_u8(4)?;
            crate::canonical::wire::observation::write_activation(writer, activation)
        }
        K::WriterObserved { observation, candidate, quality_cycle } => {
            writer.write_u8(5)?;
            crate::canonical::wire::observation::write_observation(
                writer,
                &crate::ChildObservation::Agent(observation.clone()),
            )?;
            super::command::write_candidate_option(writer, candidate.as_ref())?;
            super::command::write_quality_cycle_option(writer, quality_cycle.as_ref())
        }
        K::GatesObserved { observation, review_handoff } => {
            writer.write_u8(6)?;
            crate::canonical::wire::observation::write_gate(writer, observation)?;
            super::command::write_handoff_option(writer, review_handoff.as_ref())
        }
        K::ReviewObserved { observation, fixer_handoff } => {
            writer.write_u8(7)?;
            crate::canonical::wire::observation::write_review(writer, observation)?;
            super::command::write_handoff_option(writer, fixer_handoff.as_ref())
        }
        K::FixerObserved { completion } => {
            writer.write_u8(8)?;
            super::command::write_fixer(writer, completion)
        }
        K::RoleInfrastructureObserved { scheduler, collaboration } => {
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
        K::CandidateAdvanced { candidate, quality_cycle } => {
            writer.write_u8(10)?;
            crate::canonical::wire::domain::write_candidate(writer, candidate)?;
            crate::canonical::wire::domain::write_quality_cycle(writer, quality_cycle)
        }
        K::AcceptanceCertificateRecorded { certificate } => {
            writer.write_u8(11)?;
            crate::canonical::wire::domain::write_certificate(writer, certificate)
        }
        K::KernelAcceptanceObserved { observation } => {
            writer.write_u8(12)?;
            crate::canonical::wire::observation::write_kernel(writer, *observation)
        }
        K::Paused { phase, reconciliation } => {
            writer.write_u8(13)?;
            crate::canonical::wire::write_phase(writer, *phase)?;
            crate::canonical::wire::domain::write_reconciliation(writer, reconciliation)
        }
        K::Resumed { phase, reconciliation } => {
            writer.write_u8(14)?;
            crate::canonical::wire::write_phase(writer, *phase)?;
            crate::canonical::wire::domain::write_reconciliation(writer, reconciliation)
        }
        K::CancellationRequested { cause_digest } => write_digest_kind(writer, 15, *cause_digest),
        K::CancellationReconciled { observation } => {
            writer.write_u8(16)?;
            crate::canonical::wire::observation::write_observation(writer, observation)
        }
        K::Rejected { terminal } => write_terminal_kind(writer, 17, *terminal),
        K::Failed { terminal } => write_terminal_kind(writer, 18, *terminal),
        K::Exhausted { terminal } => write_terminal_kind(writer, 19, *terminal),
        K::Finalized { terminal } => write_terminal_kind(writer, 20, *terminal),
    }
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<OrchestratorEventKind, CodecError> {
    use OrchestratorEventKind as K;
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => {
            let binding = crate::canonical::wire::domain::read_binding(reader)?;
            let limits = binding.limits();
            Ok(K::Started {
                genesis: Box::new(crate::OrchestratorGenesis::new(
                    binding,
                    crate::canonical::wire::domain::read_candidate(reader, limits)?,
                    crate::canonical::wire::domain::read_ownership(reader, limits)?,
                    crate::canonical::wire::domain::read_handoff(reader, limits)?,
                )),
            })
        }
        2 => Ok(K::DirectivePublished {
            directive: crate::canonical::wire::domain::read_directive(reader)?,
        }),
        3 => Ok(K::DirectiveAcknowledged { directive_id: read_directive_id(reader)? }),
        4 => Ok(K::HandoffActivated {
            activation: crate::canonical::wire::observation::read_activation(reader)?,
        }),
        5 => {
            let crate::ChildObservation::Agent(observation) =
                crate::canonical::wire::observation::read_observation(reader)?
            else {
                return Err(crate::canonical::wire::invalid(reader));
            };
            Ok(K::WriterObserved {
                observation,
                candidate: super::command::read_candidate_option(reader)?,
                quality_cycle: super::command::read_quality_cycle_option(reader)?,
            })
        }
        6 => Ok(K::GatesObserved {
            observation: crate::canonical::wire::observation::read_gate(reader)?,
            review_handoff: super::command::read_handoff_option(reader)?,
        }),
        7 => Ok(K::ReviewObserved {
            observation: crate::canonical::wire::observation::read_review(reader)?,
            fixer_handoff: super::command::read_handoff_option(reader)?,
        }),
        8 => Ok(K::FixerObserved { completion: super::command::read_fixer(reader)? }),
        9 => read_role_infrastructure(reader),
        10 => Ok(K::CandidateAdvanced {
            candidate: crate::canonical::wire::domain::read_candidate(
                reader,
                crate::canonical::wire::domain::wire_limits(),
            )?,
            quality_cycle: crate::canonical::wire::domain::read_quality_cycle(reader)?,
        }),
        11 => Ok(K::AcceptanceCertificateRecorded {
            certificate: crate::canonical::wire::domain::read_certificate(reader)?,
        }),
        12 => Ok(K::KernelAcceptanceObserved {
            observation: crate::canonical::wire::observation::read_kernel(reader)?,
        }),
        13 => Ok(K::Paused {
            phase: crate::canonical::wire::read_phase(reader)?,
            reconciliation: crate::canonical::wire::domain::read_reconciliation(reader)?,
        }),
        14 => Ok(K::Resumed {
            phase: crate::canonical::wire::read_phase(reader)?,
            reconciliation: crate::canonical::wire::domain::read_reconciliation(reader)?,
        }),
        15 => Ok(K::CancellationRequested {
            cause_digest: crate::canonical::wire::read_digest(reader)?,
        }),
        16 => Ok(K::CancellationReconciled {
            observation: crate::canonical::wire::observation::read_observation(reader)?,
        }),
        17 => Ok(K::Rejected { terminal: crate::canonical::wire::domain::read_terminal(reader)? }),
        18 => Ok(K::Failed { terminal: crate::canonical::wire::domain::read_terminal(reader)? }),
        19 => Ok(K::Exhausted { terminal: crate::canonical::wire::domain::read_terminal(reader)? }),
        20 => Ok(K::Finalized { terminal: crate::canonical::wire::domain::read_terminal(reader)? }),
        _ => Err(crate::canonical::wire::unknown(offset)),
    }
}

fn read_role_infrastructure(
    reader: &mut CanonicalReader<'_>,
) -> Result<OrchestratorEventKind, CodecError> {
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
    Ok(OrchestratorEventKind::RoleInfrastructureObserved { scheduler, collaboration })
}

fn write_digest_kind(
    writer: &mut CanonicalWriter,
    tag: u8,
    digest: peritus_types::Sha256Digest,
) -> Result<(), CodecError> {
    writer.write_u8(tag)?;
    crate::canonical::wire::write_digest(writer, digest)
}

fn write_terminal_kind(
    writer: &mut CanonicalWriter,
    tag: u8,
    terminal: crate::OrchestratorTerminal,
) -> Result<(), CodecError> {
    writer.write_u8(tag)?;
    crate::canonical::wire::domain::write_terminal(writer, terminal)
}

fn read_directive_id(reader: &mut CanonicalReader<'_>) -> Result<crate::DirectiveId, CodecError> {
    let offset = reader.offset();
    crate::DirectiveId::new(reader.read_fixed()?)
        .map_err(|_| crate::canonical::wire::invalid_at(offset))
}
