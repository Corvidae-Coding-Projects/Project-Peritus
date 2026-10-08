//! One C0 transaction binds exact events, finalized artifact roots, and checkpoint CAS.

use super::super::error;
use super::{FRAME_FAMILY, LocalAppendReceipt, LocalStore, STATE_KEY, STATE_NAMESPACE};
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
    ) -> Result<LocalAppendReceipt, DeveloperLoopError> {
        self.check_cancelled()?;
        let sequence =
            self.sequence().checked_add(1).ok_or_else(|| error("event sequence overflow"))?;
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
        let command = self.identity.command(sequence)?;
        let payload_digest = sha256(payload);
        let cancellation = self.journal_cancellation.clone();
        let committed = loop {
            self.check_cancelled()?;
            let plan = AppendRequest::new(
                self.identity.store,
                command,
                payload_digest,
                vec![expectation],
                vec![draft.clone()],
                installs.clone(),
                dependencies.clone(),
                None,
                None,
                Vec::new(),
            )
            .plan()
            .map_err(|_| error("validate journal transaction"))?;
            match cancellation.run(|| self.journal.append(plan)) {
                Ok(committed) => break committed,
                Err(failure) if failure.is_storage_exhausted() => {
                    self.prepare_storage_pressure_wait()?;
                    self.wait_for_storage_pressure()?;
                }
                Err(failure) => {
                    return Err(
                        self.journal_failure("commit local journal transaction", failure)
                    );
                }
            }
        };
        self.head = cancellation.run(|| self
            .journal
            .head(self.identity.aggregate)
        ).map_err(|failure| self.journal_failure("observe committed journal head", failure))?;
        self.generation = next_generation;
        Ok(LocalAppendReceipt { owner: committed.batch_hash() })
    }
}
