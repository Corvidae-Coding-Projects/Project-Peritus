//! One C0 transaction binds exact events, finalized artifact roots, and checkpoint CAS.

use super::super::error;
use super::{FRAME_FAMILY, LocalStore, MAX_RECORDS, STATE_KEY, STATE_NAMESPACE};
use peritus_agent::DeveloperLoopError;
use peritus_codec::{CodecLimits, encode_frame, sha256};
use peritus_journal::{
    AppendRequest, ArtifactDependency, EventDraft, ExactFrame, HeadExpectation, StateInstall,
};
use peritus_types::{EventSequence, Sha256Digest};

impl LocalStore {
    pub(in crate::local_context) fn append(
        &mut self,
        payload: &[u8],
        references: &[Sha256Digest],
        checkpoint: Option<(u64, Vec<u8>)>,
    ) -> Result<(), DeveloperLoopError> {
        let sequence = self
            .sequence()
            .checked_add(1)
            .filter(|sequence| *sequence <= MAX_RECORDS)
            .ok_or_else(|| error("journal record capacity exceeded"))?;
        let event = self.identity.event(sequence)?;
        let frame = ExactFrame::new(
            encode_frame(FRAME_FAMILY, 1, payload, CodecLimits::PRODUCTION)
                .map_err(|_| error("encode local journal event"))?,
        )
        .map_err(|_| error("validate local journal frame"))?;
        let draft = EventDraft::new(
            self.identity.aggregate,
            EventSequence::new(sequence).map_err(|_| error("event sequence overflow"))?,
            event,
            self.head.map(peritus_journal::AggregateHead::event_id),
            frame,
            self.identity.scope,
            Vec::new(),
        )
        .map_err(|_| error("validate event draft"))?;
        let mut next_generation = self.generation;
        let installs = if let Some((expected, bytes)) = checkpoint {
            if expected != self.generation {
                return Err(error("stale checkpoint generation"));
            }
            next_generation =
                expected.checked_add(1).ok_or_else(|| error("checkpoint generation overflow"))?;
            vec![
                StateInstall::new(
                    STATE_NAMESPACE,
                    STATE_KEY.to_vec(),
                    (expected != 0).then_some(expected),
                    next_generation,
                    bytes,
                )
                .map_err(|_| error("validate checkpoint publication"))?,
            ]
        } else {
            Vec::new()
        };
        let mut digests = references.to_vec();
        digests.sort();
        digests.dedup();
        let dependencies = digests.into_iter().map(ArtifactDependency::new).collect();
        let expectation = self
            .head
            .map_or(HeadExpectation::Absent(self.identity.aggregate), HeadExpectation::Present);
        let plan = AppendRequest::new(
            self.identity.store,
            self.identity.command(sequence)?,
            sha256(payload),
            vec![expectation],
            vec![draft],
            installs,
            dependencies,
            None,
            None,
            Vec::new(),
        )
        .plan()
        .map_err(|_| error("validate journal transaction"))?;
        self.journal.append(plan).map_err(|_| error("commit local journal transaction"))?;
        self.head = self
            .journal
            .head(self.identity.aggregate)
            .map_err(|_| error("observe committed journal head"))?;
        self.generation = next_generation;
        Ok(())
    }
}
