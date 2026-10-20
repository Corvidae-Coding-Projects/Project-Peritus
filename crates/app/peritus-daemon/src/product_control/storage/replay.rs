//! Verified immutable replay and exact historical checkpoint reads.

use super::{
    ControlStore, Error, FRAME_FAMILY, MANIFEST_NAMESPACE, REPLY_NAMESPACE, REQUEST_NAMESPACE,
    ROOT_NAMESPACE, aggregate,
};
use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_journal::ReplayObservation;
use peritus_product_runner::control::{
    ControlOperation, ConversationId, ConversationRecord, ConversationReplay,
};
use std::{collections::BTreeMap, sync::Arc};

const CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(super) struct ReplayCache {
    entries: BTreeMap<ConversationId, CachedReplay>,
    clock: u64,
    bytes: usize,
    hits: u64,
    misses: u64,
}

pub(super) struct CachedReplay {
    replay: Arc<ConversationReplay>,
    observation: ReplayObservation,
    used: u64,
    bytes: usize,
}

impl ControlStore {
    /// Verifies bounded immutable history against the current C0 state root without recovery writes.
    pub fn load(&self, id: ConversationId) -> Result<Option<ConversationRecord>, Error> {
        Ok(self.load_replay(id)?.current().cloned())
    }

    /// Inspects verified current state without cloning the full retained conversation.
    pub(crate) fn inspect<T>(
        &self,
        id: ConversationId,
        read: impl FnOnce(&ConversationRecord) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let replay = self.load_replay(id)?;
        read(replay.current().ok_or(peritus_product_runner::control::ControlError::NotFound)?)
    }

    fn load_replay(&self, id: ConversationId) -> Result<Arc<ConversationReplay>, Error> {
        self.observed_replay(id).map(|(replay, _)| replay)
    }

    pub(super) fn observed_replay(
        &self,
        id: ConversationId,
    ) -> Result<(Arc<ConversationReplay>, ReplayObservation), Error> {
        {
            let mut cache = self.replay.borrow_mut();
            cache.clock = cache.clock.wrapping_add(1);
            let clock = cache.clock;
            if let Some(cached) = cache.entries.get_mut(&id)
                && self.journal.replay_observation_is_current(&cached.observation)?
            {
                cached.used = clock;
                let result = (Arc::clone(&cached.replay), cached.observation.clone());
                cache.hits = cache.hits.saturating_add(1);
                return Ok(result);
            }
            cache.misses = cache.misses.saturating_add(1);
        }
        let observation = self.journal.observe_replay()?;
        let (replay, _) = self.load_at_revision(id, None)?;
        // Archive reads occur outside the chain snapshot. Never bless a replay assembled
        // across an external commit, even if the final root happens to remain identical.
        if !self.journal.replay_observation_is_current(&observation)? {
            return Err(peritus_product_runner::control::ControlError::StaleRevision.into());
        }
        let replay = Arc::new(replay);
        self.cache_replay_arc(id, Arc::clone(&replay), observation.clone());
        Ok((replay, observation))
    }

    pub(super) fn cache_replay(
        &self,
        conversation: ConversationId,
        replay: ConversationReplay,
        observation: ReplayObservation,
    ) {
        self.cache_replay_arc(conversation, Arc::new(replay), observation);
    }

    fn cache_replay_arc(
        &self,
        id: ConversationId,
        replay: Arc<ConversationReplay>,
        observation: ReplayObservation,
    ) {
        // Eviction limits only this optimization. Oversized histories remain usable through
        // verified cold replay and are never rejected by a run or conversation budget.
        let bytes = match replay.current().map(ConversationRecord::canonical_bytes).transpose() {
            Ok(bytes) => bytes.map_or(0, |value| value.len()).saturating_mul(2),
            Err(_) => return,
        };
        let mut cache = self.replay.borrow_mut();
        if let Some(old) = cache.entries.remove(&id) {
            cache.bytes = cache.bytes.saturating_sub(old.bytes);
        }
        if bytes > CACHE_BYTES {
            return;
        }
        while cache.bytes.saturating_add(bytes) > CACHE_BYTES {
            let Some(oldest) =
                cache.entries.iter().min_by_key(|(_, value)| value.used).map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(old) = cache.entries.remove(&oldest) {
                cache.bytes = cache.bytes.saturating_sub(old.bytes);
            }
        }
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        cache.bytes = cache.bytes.saturating_add(bytes);
        cache.entries.insert(id, CachedReplay { replay, observation, used, bytes });
    }

    pub(super) fn reusable_conversations(
        &self,
        except: ConversationId,
    ) -> Result<Vec<ConversationId>, Error> {
        self.replay
            .borrow()
            .entries
            .iter()
            .filter(|(id, _)| **id != except)
            .filter_map(|(id, cached)| {
                match self.journal.replay_observation_is_current(&cached.observation) {
                    Ok(true) => Some(Ok(*id)),
                    Ok(false) => None,
                    Err(error) => Some(Err(error.into())),
                }
            })
            .collect()
    }

    pub(super) fn advance_unmodified_replays(
        &self,
        ids: &[ConversationId],
        observation: &ReplayObservation,
    ) {
        let mut cache = self.replay.borrow_mut();
        for id in ids {
            if let Some(cached) = cache.entries.get_mut(id) {
                cached.observation = observation.clone();
            }
        }
    }

    pub(crate) fn replay_cache_summary(&self) -> String {
        let cache = self.replay.borrow();
        format!(
            "{} cached conversations; {} cache hits; {} cold replays; {} estimated retained bytes",
            cache.entries.len(),
            cache.hits,
            cache.misses,
            cache.bytes
        )
    }

    /// Reads an exact historical state while still verifying the complete current archive.
    pub(crate) fn load_revision(
        &self,
        id: ConversationId,
        revision: u64,
    ) -> Result<Option<ConversationRecord>, Error> {
        self.load_at_revision(id, Some(revision)).map(|(_, selected)| selected)
    }

    fn load_at_revision(
        &self,
        id: ConversationId,
        revision: Option<u64>,
    ) -> Result<(ConversationReplay, Option<ConversationRecord>), Error> {
        let aggregate = aggregate(id)?;
        let (records, root) = self
            .journal
            .aggregate_checkpoint_snapshot(aggregate, ROOT_NAMESPACE, id.as_bytes())?
            .into_parts();
        let mut replay = ConversationReplay::default();
        let mut selected = None;
        for record in &records {
            let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| Error::Corrupt("invalid control event frame"))?;
            if frame.header().family() != FRAME_FAMILY || frame.header().schema_version() != 1 {
                return Err(Error::Corrupt("unsupported control event generation"));
            }
            let operation = ControlOperation::parse(frame.payload())?;
            if operation.conversation() != id
                || record.command_id().as_bytes() != operation.id().as_bytes()
            {
                return Err(Error::Corrupt("control event scope or operation identity mismatch"));
            }
            self.verify_request_archive(&operation, record.global_position(), replay.current())?;
            replay.apply(&operation)?;
            let next = replay.current().ok_or(Error::Corrupt("missing replay successor"))?;
            if revision == Some(next.revision()) {
                selected = Some(next.clone());
            }
        }
        match (replay.current(), &root, records.last()) {
            (None, None, None) => Ok((replay, selected)),
            (Some(current), Some(root), Some(last))
                if root.revision() == current.revision()
                    // A fork publishes two aggregate events in one atomic batch. State installs
                    // identify the batch's last global position, while either aggregate event may
                    // precede it inside that same batch.
                    && root.producing_position() >= last.global_position()
                    && root.bytes() == current.canonical_bytes()? =>
            {
                Ok((replay, selected))
            }
            _ => Err(Error::Corrupt("control projection does not match immutable replay")),
        }
    }

    pub(super) fn verify_request_archive(
        &self,
        operation: &ControlOperation,
        position: u64,
        before: Option<&ConversationRecord>,
    ) -> Result<(), Error> {
        use peritus_product_runner::control::{ControlIntent, QueueIntent};
        if matches!(operation.intent(), ControlIntent::RecordInitialization { .. }) {
            return self.verify_initialization_archive(operation, position);
        }
        if matches!(
            operation.intent(),
            ControlIntent::AttachFile { .. }
                | ControlIntent::RefreshFile { .. }
                | ControlIntent::SubmitMessage { .. }
                | ControlIntent::AcceptBriefProposal { .. }
                | ControlIntent::RefreshFiles { .. }
        ) {
            return self.verify_file_archive(operation, position);
        }
        if matches!(
            operation.intent(),
            ControlIntent::CreateCheckpoint(_)
                | ControlIntent::CreateAutomaticCheckpoint(_)
                | ControlIntent::PrepareRestore { .. }
                | ControlIntent::PrepareAutomaticRestore { .. }
                | ControlIntent::SettleRestore { .. }
                | ControlIntent::SettleAutomaticRestore { .. }
        ) {
            return self.verify_checkpoint_archive(operation, position);
        }
        if let ControlIntent::AttachImage { image, .. } = operation.intent() {
            return self.verify_image_archive(image, position);
        }
        if let ControlIntent::PublishReply(reply) = operation.intent() {
            let artifact = self
                .journal
                .state_record(REPLY_NAMESPACE, operation.id().as_bytes())?
                .ok_or(Error::Corrupt("public reply artifact missing"))?;
            if artifact.revision() != 1 || artifact.producing_position() != position {
                return Err(Error::Corrupt("public reply was not published with its reference"));
            }
            return super::super::replies::verify_text(reply, artifact.bytes());
        }
        let ControlIntent::Queue(QueueIntent::Incorporate { request_digest, .. }) =
            operation.intent()
        else {
            return Ok(());
        };
        let request = self
            .journal
            .state_record(REQUEST_NAMESPACE, operation.id().as_bytes())?
            .ok_or(Error::Corrupt("incorporation request artifact missing"))?;
        let manifest = self
            .journal
            .state_record(MANIFEST_NAMESPACE, operation.id().as_bytes())?
            .ok_or(Error::Corrupt("incorporation manifest missing"))?;
        if request.revision() != 1
            || manifest.revision() != 1
            || request.producing_position() != position
            || manifest.producing_position() != position
            || sha256(request.bytes()).as_bytes() != request_digest
        {
            return Err(Error::Corrupt("request archive differs from atomic incorporation"));
        }
        super::super::inputs::verify_manifest(operation, manifest.bytes(), before)
    }

    /// Loads one retained or replay-only automatic checkpoint after complete history verification.
    pub(crate) fn load_checkpoint(
        &self,
        conversation: ConversationId,
        checkpoint: peritus_product_runner::control::CheckpointId,
    ) -> Result<Option<peritus_product_runner::control::UserCheckpoint>, Error> {
        let replay = self.load_replay(conversation)?;
        if let Some(value) = replay
            .current()
            .and_then(|record| record.checkpoints().iter().find(|value| value.id() == checkpoint))
        {
            return Ok(Some(value.clone()));
        }
        Ok(replay.automatic_checkpoint(checkpoint).cloned())
    }
}
