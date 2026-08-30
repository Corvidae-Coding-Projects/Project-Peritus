//! Real scheduler command construction and public-A3 submission.

use peritus_app_protocol::{
    AppProtocolLimits, AppRequestPayload, AppResponsePayload, CommandBinding, CommandDisposition,
    CommandSubmissionFrames, CorrelationId, IdempotencyKey, RequestId,
};
use peritus_codec::{CodecLimits, encode_message};
use peritus_kernel::CommandEnvelope;
use peritus_protocol::CommandEnvelopeDto;
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandFrame, SchedulerCommandKind, SchedulerId, SchedulerLimits,
    SchedulerPhase, SchedulerState, decide, start,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use crate::{SubjectError, a3::A3Client, identity::IdentitySource};

const HUMAN_ACTOR: [u8; 16] = [0x22; 16];

pub struct SchedulerRun {
    state: SchedulerState,
}

impl SchedulerRun {
    pub fn create(
        client: &mut A3Client,
        identities: &mut IdentitySource,
        revision: RevisionTuple,
    ) -> Result<Self, SubjectError> {
        let run_id = identities.next(RunId::new)?;
        let binding = SchedulerBinding::new(
            run_id,
            SchedulerId::new(identities.bytes()?)?,
            revision,
            scheduler_limits()?,
            resources()?,
        )?;
        let command = SchedulerCommand::new(
            identities.next(CommandId::new)?,
            identities.next(EventId::new)?,
            run_id,
            0,
            None,
            Sha256Digest::new([0; 32]),
            revision,
            SchedulerCommandKind::StartScheduler { binding },
        )?;
        let transition = start(&command)?;
        submit(client, identities, &command)?;
        Ok(Self { state: transition.into_state() })
    }

    pub fn append_event(
        &mut self,
        client: &mut A3Client,
        identities: &mut IdentitySource,
    ) -> Result<(), SubjectError> {
        let kind = match self.state.phase() {
            SchedulerPhase::Active => SchedulerCommandKind::PauseScheduler,
            SchedulerPhase::Paused => SchedulerCommandKind::ResumeScheduler,
            _ => {
                return Err(SubjectError::UnexpectedResponse(
                    "cannot append a qualification event to a draining scheduler".to_owned(),
                ));
            }
        };
        self.transition(client, identities, kind)
    }

    pub fn finish(
        &mut self,
        client: &mut A3Client,
        identities: &mut IdentitySource,
    ) -> Result<(), SubjectError> {
        self.transition(client, identities, SchedulerCommandKind::DrainScheduler)?;
        self.transition(client, identities, SchedulerCommandKind::FinalizeScheduler)
    }

    fn transition(
        &mut self,
        client: &mut A3Client,
        identities: &mut IdentitySource,
        kind: SchedulerCommandKind,
    ) -> Result<(), SubjectError> {
        let command = SchedulerCommand::new(
            identities.next(CommandId::new)?,
            identities.next(EventId::new)?,
            self.state.run_id(),
            self.state.sequence().get(),
            Some(self.state.last_event_id()),
            self.state.state_digest(),
            self.state.binding().revision(),
            kind,
        )?;
        let transition = decide(&self.state, &command)?;
        submit(client, identities, &command)?;
        self.state = transition.into_state();
        Ok(())
    }
}

pub fn qualification_revision(
    identities: &mut IdentitySource,
) -> Result<RevisionTuple, SubjectError> {
    Ok(RevisionTuple::new(
        identities.next(AcceptanceSpecId::new)?,
        identities.next(HarnessId::new)?,
        identities.next(WorkspaceId::new)?,
        Generation::first(),
        RevisionNumber::first(),
        identities.next(PolicyId::new)?,
        identities.next(ProviderProfileId::new)?,
    ))
}

fn submit(
    client: &mut A3Client,
    identities: &mut IdentitySource,
    command: &SchedulerCommand,
) -> Result<(), SubjectError> {
    let request_id = identities.next(RequestId::new)?;
    let correlation_id = identities.next(CorrelationId::new)?;
    let envelope = CommandEnvelope::new(
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    );
    let envelope_bytes =
        encode_message(&CommandEnvelopeDto::from(envelope), CodecLimits::PRODUCTION)?;
    let command_bytes =
        encode_message(&SchedulerCommandFrame::from_command(command), CodecLimits::PRODUCTION)?;
    let frames = CommandSubmissionFrames::parse(
        envelope_bytes,
        command_bytes,
        AppProtocolLimits::PRODUCTION,
    )?;
    let binding = CommandBinding::new(
        ActorId::new(HUMAN_ACTOR).map_err(SubjectError::Identifier)?,
        client.session_id(),
        request_id,
        correlation_id,
        IdempotencyKey::new(identities.key()?)
            .map_err(|error| SubjectError::Configuration(format!("idempotency key: {error:?}")))?,
        Some(command.revision()),
        frames,
    )?;
    let response = client.request(
        request_id,
        correlation_id,
        AppRequestPayload::SubmitCommand(binding),
        identities,
    )?;
    match response.payload() {
        AppResponsePayload::CommandResult(result)
            if matches!(
                result.disposition(),
                CommandDisposition::Committed | CommandDisposition::Replayed
            ) && result.committed_events().is_some_and(|range| range.count() == 1) =>
        {
            Ok(())
        }
        AppResponsePayload::CommandResult(result) => {
            Err(SubjectError::CommandRejected(result.error().map_or_else(
                || format!("unexpected disposition {:?}", result.disposition()),
                |error| format!("{:?}", error.code()),
            )))
        }
        AppResponsePayload::Error(error) => {
            Err(SubjectError::CommandRejected(format!("A3 error {:?}", error.code())))
        }
        payload => Err(SubjectError::UnexpectedResponse(format!(
            "command submission returned {payload:?}"
        ))),
    }
}

fn scheduler_limits() -> Result<SchedulerLimits, SubjectError> {
    SchedulerLimits::new(
        SchedulerLimits::MAX_QUEUED_WORK,
        SchedulerLimits::MAX_RETAINED_WORK,
        64,
        64,
        8,
        64,
        8,
        64,
        64,
        SchedulerLimits::MAX_PAYLOAD_BYTES,
        SchedulerLimits::MAX_STATE_BYTES,
    )
    .map_err(SubjectError::Scheduler)
}

fn resources() -> Result<ResourceVector, SubjectError> {
    ResourceVector::new(
        vec![
            ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(64)?),
            ResourceEntry::new(
                ResourceKind::MEMORY_BYTES,
                ResourceQuantity::new(64 * 1_073_741_824)?,
            ),
        ],
        8,
    )
    .map_err(SubjectError::Scheduler)
}
