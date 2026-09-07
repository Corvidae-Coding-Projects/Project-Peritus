//! Exact-sized, real D2 events sent through public A3, with explicit prerequisite commits.

mod fixture;

use peritus_codec::{CodecLimits, HEADER_LEN, encode_message};
use peritus_kernel::CommandEnvelope;
use peritus_review::{
    ReviewCommand, ReviewCommandFrame, ReviewCommandKind, ReviewEventFrame, ReviewRunState,
    ReviewTransition, decide, start,
};
use peritus_types::{CommandId, EventId, RunId, Sha256Digest};

use crate::{SubjectError, a3::A3Client, identity::IdentitySource, scheduler::submit_frames};
use fixture::EventFixture;

/// One actual review submission and its two separately counted prerequisite events.
pub struct EventOperation {
    commands: [ReviewCommand; 3],
    event: Vec<u8>,
}

impl EventOperation {
    pub fn prepare(
        identities: &mut IdentitySource,
        revision: peritus_types::RevisionTuple,
        payload_bytes: u32,
        seed: u64,
    ) -> Result<Self, SubjectError> {
        let fixture = EventFixture::new(revision)?;
        let run_id = identities.next(RunId::new)?;
        let genesis = ReviewCommand::new(
            identities.next(CommandId::new)?,
            identities.next(EventId::new)?,
            run_id,
            0,
            None,
            Sha256Digest::new([0; 32]),
            revision,
            ReviewCommandKind::StartRun {
                binding: fixture.binding.clone(),
                limits: fixture.limits,
            },
        )?;
        let first = start(&genesis)?;
        let assignment = fixture.assignment(identities)?;
        let assign = successor(
            identities,
            first.state(),
            ReviewCommandKind::AssignReviewer { assignment: assignment.clone() },
        )?;
        let second = decide(first.state(), &assign)?;
        let command_id = identities.next(CommandId::new)?;
        let event_id = identities.next(EventId::new)?;
        let finding_id = identities.next(peritus_types::FindingId::new)?;
        let build = |text| -> Result<(ReviewCommand, ReviewTransition), SubjectError> {
            let submission = fixture.submission(&assignment, finding_id, text)?;
            let state = second.state();
            let command = ReviewCommand::new(
                command_id,
                event_id,
                run_id,
                state.sequence().get(),
                Some(state.last_event_id()),
                state.state_digest(),
                revision,
                ReviewCommandKind::SubmitReview { submission },
            )?;
            let transition = decide(state, &command)?;
            Ok((command, transition))
        };
        let (_, minimal) = build("x".to_owned())?;
        let overhead = encoded_event(&minimal)?.len() - HEADER_LEN - 1;
        let text_length = usize::try_from(payload_bytes).unwrap_or(usize::MAX)
            .checked_sub(overhead).filter(|length| *length > 0)
            .ok_or_else(|| SubjectError::Configuration(format!(
                "planned event payload {payload_bytes} cannot fit the {overhead}-byte review event body"
            )))?;
        if text_length > fixture.limits.text_bytes() as usize {
            return Err(SubjectError::Configuration(
                "planned event exceeds review text bound".to_owned(),
            ));
        }
        let (submission, transition) = build(seeded_text(text_length, seed))?;
        let event = encoded_event(&transition)?;
        if event.len() - HEADER_LEN != payload_bytes as usize {
            return Err(SubjectError::Configuration(
                "review event encoded length differs from plan".to_owned(),
            ));
        }
        Ok(Self { commands: [genesis, assign, submission], event })
    }

    pub const fn event_id(&self) -> [u8; 16] {
        *self.commands[2].event_id().as_bytes()
    }
    pub fn event_digest(&self) -> peritus_benchmarks::Sha256Digest {
        peritus_benchmarks::Sha256Digest::of_bytes(&self.event)
    }

    pub fn submit(
        &self,
        client: &mut A3Client,
        identities: &mut IdentitySource,
        committed: &mut u32,
    ) -> Result<(), SubjectError> {
        for command in &self.commands {
            let envelope = CommandEnvelope::new(
                command.command_id(),
                command.event_id(),
                command.expected_previous_event(),
                command.revision(),
            );
            let bytes = encode_message(
                &ReviewCommandFrame::from_command(command),
                CodecLimits::PRODUCTION,
            )?;
            submit_frames(client, identities, envelope, bytes)?;
            *committed += 1;
        }
        Ok(())
    }
}

fn encoded_event(transition: &ReviewTransition) -> Result<Vec<u8>, SubjectError> {
    Ok(encode_message(&ReviewEventFrame(transition.event().clone()), CodecLimits::PRODUCTION)?)
}

fn successor(
    identities: &mut IdentitySource,
    state: &ReviewRunState,
    kind: ReviewCommandKind,
) -> Result<ReviewCommand, SubjectError> {
    Ok(ReviewCommand::new(
        identities.next(CommandId::new)?,
        identities.next(EventId::new)?,
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.binding().revision(),
        kind,
    )?)
}

fn seeded_text(length: usize, mut seed: u64) -> String {
    (0..length)
        .map(|_| {
            seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut value = (seed ^ (seed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            char::from(b'!' + u8::try_from((value ^ (value >> 31)) % 94).expect("bounded ASCII"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoded_event_payload_is_exact_and_too_small_is_rejected() {
        let mut identities = IdentitySource::new(9);
        let revision = crate::scheduler::qualification_revision(&mut identities).expect("revision");
        for bytes in [3_072, 4_096, 6_144, 8_192, 10_240] {
            let operation =
                EventOperation::prepare(&mut identities, revision, bytes, 1102).expect("event");
            assert_eq!(operation.event.len() - HEADER_LEN, bytes as usize);
            let decoded: ReviewEventFrame =
                peritus_codec::decode_message(&operation.event, CodecLimits::PRODUCTION)
                    .expect("decode");
            assert_eq!(*decoded.into_event().id().as_bytes(), operation.event_id());
        }
        assert!(EventOperation::prepare(&mut identities, revision, 1, 1102).is_err());
        assert!(EventOperation::prepare(&mut identities, revision, u32::MAX, 1102).is_err());
    }
}
