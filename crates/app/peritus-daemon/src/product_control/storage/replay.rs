//! Journal-authoritative current projections, indexed history, and resumable integrity replay.

use super::{
    ControlStore, Error, MANIFEST_NAMESPACE, REQUEST_NAMESPACE, ROOT_NAMESPACE, aggregate,
    command_id,
    projection::{CurrentProjection, EventBinding, RootBinding},
};
use peritus_codec::sha256;
use peritus_journal::{AggregateHead, AggregateKey, CommittedRecord, DurableStateRecord};
use peritus_product_runner::control::{
    ControlOperation, ConversationId, ConversationRecord, ConversationReplay,
};
use std::collections::BTreeMap;

const REPLAY_WINDOW: usize = peritus_journal::MAX_GLOBAL_WINDOW_RECORDS;

struct ReplayCursor {
    head: Option<EventBinding>,
    replay: ConversationReplay,
    legacy_records: BTreeMap<u64, ConversationRecord>,
}

struct BoundProjection {
    event: EventBinding,
    root: RootBinding,
    record: ConversationRecord,
    replay: Option<Vec<u8>>,
}

enum SuffixOutcome {
    Complete(ConversationReplay, Option<ConversationRecord>),
    NeedsGenesis,
}

impl ControlStore {
    /// Reads the exact current journal-owned projection without replaying immutable history.
    pub fn load(&self, id: ConversationId) -> Result<Option<ConversationRecord>, Error> {
        Ok(self.load_replay(id)?.current().cloned())
    }

    pub(super) fn load_replay(&self, id: ConversationId) -> Result<ConversationReplay, Error> {
        self.verified_replay(id)
    }

    pub(super) fn refresh_replay_projection(&self, id: ConversationId) -> Result<(), Error> {
        let Some(projections) = self.projections.as_ref() else {
            return Ok(());
        };
        let aggregate = aggregate(id)?;
        let Some(bound) = self.current_bound_projection(id, aggregate, true)? else {
            return Ok(());
        };
        let Some(replay) = bound.replay else {
            return Ok(());
        };
        projections.write_current(
            id,
            &CurrentProjection { head: bound.event, root: bound.root, replay },
        )
    }

    /// Reads an exact historical projection from journal-owned state history.
    pub(crate) fn load_revision(
        &self,
        id: ConversationId,
        revision: u64,
    ) -> Result<Option<ConversationRecord>, Error> {
        let replay = self.verified_replay(id)?;
        let Some(current) = replay.current() else {
            return Ok(None);
        };
        if revision == 0 || revision > current.revision() {
            return Ok(None);
        }
        if revision == current.revision() {
            return Ok(Some(current.clone()));
        }
        let aggregate = aggregate(id)?;
        if let Some(bound) = self.bound_projection_revision(id, aggregate, revision, false)? {
            return Ok(Some(bound.record));
        }

        // Legacy events do not commit their successor digest. Preserve their exact compatibility
        // by deriving the requested record from immutable history rather than trusting a cache.
        let (_, record) = self.verify_from_genesis(id, aggregate, Some(revision))?;
        record
            .map(Some)
            .ok_or(Error::Corrupt("rebuilt conversation history is incomplete"))
    }

    fn verified_replay(&self, id: ConversationId) -> Result<ConversationReplay, Error> {
        let aggregate = aggregate(id)?;
        let durable_head = self.journal.head(aggregate)?;
        let durable_root = self.journal.state_record(ROOT_NAMESPACE, id.as_bytes())?;
        match (durable_head, durable_root.as_ref()) {
            (None, None) => return Ok(ConversationReplay::default()),
            (Some(_), Some(_)) => {}
            _ => return Err(Error::Corrupt("control head and projection root disagree")),
        }
        if let Some(bound) = self.bound_projection_root(
            id,
            aggregate,
            durable_root
                .as_ref()
                .ok_or(Error::Corrupt("control projection root disappeared"))?,
            durable_head,
            true,
        )? {
            let witness = bound
                .replay
                .ok_or(Error::Corrupt("authoritative replay witness disappeared"))?;
            let replay = ConversationReplay::from_checkpoint_bytes(&witness)?;
            if replay.current() != Some(&bound.record) {
                return Err(Error::Corrupt("authoritative replay witness differs from its root"));
            }
            return Ok(replay);
        }
        self.verify_from_genesis(id, aggregate, None).map(|(replay, _)| replay)
    }

    fn current_bound_projection(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        include_replay: bool,
    ) -> Result<Option<BoundProjection>, Error> {
        let head = self.journal.head(aggregate)?;
        let root = self.journal.state_record(ROOT_NAMESPACE, id.as_bytes())?;
        match (head, root.as_ref()) {
            (None, None) => Ok(None),
            (Some(head), Some(root)) => {
                self.bound_projection_root(id, aggregate, root, Some(head), include_replay)
            }
            _ => Err(Error::Corrupt("control head and projection root disagree")),
        }
    }

    fn bound_projection_revision(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        revision: u64,
        include_replay: bool,
    ) -> Result<Option<BoundProjection>, Error> {
        let Some(root) = self
            .journal
            .state_record_revision(ROOT_NAMESPACE, id.as_bytes(), revision)?
        else {
            return Ok(None);
        };
        self.bound_projection_root(id, aggregate, &root, None, include_replay)
    }

    fn bound_projection_root(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        root: &DurableStateRecord,
        expected_head: Option<AggregateHead>,
        include_replay: bool,
    ) -> Result<Option<BoundProjection>, Error> {
        if root.namespace() != ROOT_NAMESPACE
            || root.key() != id.as_bytes()
            || root.revision() == 0
        {
            return Err(Error::Corrupt("control projection root identity is invalid"));
        }
        let event_cursor = root
            .revision()
            .checked_sub(1)
            .ok_or(Error::Corrupt("control projection root has no aggregate event"))?;
        let events = self.journal.aggregate_events_after(aggregate, event_cursor, 1)?;
        let event = events
            .first()
            .ok_or(Error::Corrupt("control projection aggregate event is missing"))?;
        let binding = event_binding(event);
        if event.aggregate() != aggregate || event.sequence().get() != root.revision() {
            return Err(Error::Corrupt("control projection event differs from its revision"));
        }
        if expected_head.is_some_and(|head| !binding_matches_head(binding, head)) {
            return Err(Error::Corrupt("control projection event differs from its durable head"));
        }
        let (operation, successor_digest) =
            self.decode_control_event_evidence(event.frame_bytes())?;
        if operation.conversation() != id
            || event.command_id() != command_id(&operation)?
        {
            return Err(Error::Corrupt(
                "control projection operation or command identity mismatch",
            ));
        }
        if root.producing_position() < event.global_position() {
            return Err(Error::Corrupt("control projection predates its aggregate event"));
        }
        let batch = self
            .journal
            .command_batch(event.command_id())?
            .ok_or(Error::Corrupt("control projection command batch is missing"))?;
        if batch.last_position() != root.producing_position()
            || !batch.records().contains(event)
        {
            return Err(Error::Corrupt(
                "control projection was not installed by its exact command batch",
            ));
        }
        let Some(successor_digest) = successor_digest else {
            return Ok(None);
        };
        let record = self.decode_control_projection_record(root.bytes())?;
        if record.id() != id
            || record.revision() != root.revision()
            || sha256(&record.canonical_bytes()?).into_bytes() != successor_digest
        {
            return Err(Error::Corrupt("control projection differs from its producing event"));
        }
        let replay = if include_replay {
            let (evidence_record, witness) =
                self.decode_control_projection_evidence(root.bytes())?;
            if evidence_record != record {
                return Err(Error::Corrupt("control projection decoders disagree"));
            }
            let Some(witness) = witness else {
                return Ok(None);
            };
            Some(witness)
        } else {
            None
        };
        Ok(Some(BoundProjection {
            event: binding,
            root: RootBinding {
                revision: root.revision(),
                digest: root.digest().into_bytes(),
                producing_position: root.producing_position(),
            },
            record,
            replay,
        }))
    }

    fn resume_cursor(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        requested_revision: Option<u64>,
    ) -> Result<Option<ReplayCursor>, Error> {
        let Some(projection) = self
            .projections
            .as_ref()
            .and_then(|projections| projections.read_current(id).ok().flatten())
        else {
            return Ok(None);
        };
        if requested_revision.is_some_and(|revision| revision <= projection.head.sequence) {
            return Ok(None);
        }
        let Some(bound) = self.bound_projection_revision(
            id,
            aggregate,
            projection.head.sequence,
            true,
        )? else {
            return Ok(None);
        };
        if bound.event != projection.head || bound.root != projection.root {
            return Ok(None);
        }
        let Some(witness) = bound.replay else {
            return Ok(None);
        };
        if witness != projection.replay {
            return Ok(None);
        }
        let Ok(replay) = ConversationReplay::from_checkpoint_bytes(&witness) else {
            return Ok(None);
        };
        if replay.current() != Some(&bound.record) {
            return Ok(None);
        }
        Ok(Some(ReplayCursor {
            head: Some(bound.event),
            replay,
            legacy_records: BTreeMap::new(),
        }))
    }

    fn genesis_cursor(&self) -> ReplayCursor {
        ReplayCursor {
            head: None,
            replay: ConversationReplay::default(),
            legacy_records: BTreeMap::new(),
        }
    }

    fn verify_from_genesis(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        requested_revision: Option<u64>,
    ) -> Result<(ConversationReplay, Option<ConversationRecord>), Error> {
        if let Some(cursor) = self.resume_cursor(id, aggregate, requested_revision)?
            && let SuffixOutcome::Complete(replay, requested) =
                self.verify_suffix(id, aggregate, cursor, requested_revision, true)?
        {
            return Ok((replay, requested));
        }
        match self.verify_suffix(
            id,
            aggregate,
            self.genesis_cursor(),
            requested_revision,
            false,
        )? {
            SuffixOutcome::Complete(replay, requested) => Ok((replay, requested)),
            SuffixOutcome::NeedsGenesis => {
                Err(Error::Corrupt("genesis control replay requested another restart"))
            }
        }
    }

    fn verify_suffix(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        mut cursor: ReplayCursor,
        requested_revision: Option<u64>,
        resumed: bool,
    ) -> Result<SuffixOutcome, Error> {
        let mut requested_record = None;
        loop {
            let sequence = cursor.head.map_or(0, |binding| binding.sequence);
            let records = self.journal.aggregate_events_after(aggregate, sequence, REPLAY_WINDOW)?;
            if records.is_empty() {
                break;
            }
            for record in &records {
                if !record_follows(cursor.head, record) {
                    return Err(Error::Corrupt("control aggregate predecessor chain is broken"));
                }
                let (operation, committed_successor) =
                    self.decode_control_event_evidence(record.frame_bytes())?;
                if operation.conversation() != id
                    || record.command_id() != command_id(&operation)?
                {
                    return Err(Error::Corrupt(
                        "control event scope or operation identity mismatch",
                    ));
                }
                let historical = match self
                    .request_source_revision(&operation, record.global_position())?
                {
                    Some(revision) => {
                        if let Some(bound) =
                            self.bound_projection_revision(id, aggregate, revision, false)?
                        {
                            Some(bound.record)
                        } else if let Some(record) = cursor.legacy_records.get(&revision) {
                            Some(record.clone())
                        } else if resumed {
                            return Ok(SuffixOutcome::NeedsGenesis);
                        } else {
                            return Err(Error::Corrupt("request source history is unavailable"));
                        }
                    }
                    None => None,
                };
                self.verify_request_archive_source(
                    &operation,
                    record.global_position(),
                    cursor.replay.current(),
                    historical.as_ref(),
                )?;
                cursor.replay.apply(&operation)?;
                let successor = cursor
                    .replay
                    .current()
                    .ok_or(Error::Corrupt("missing replay successor"))?;
                if successor.id() != id || successor.revision() != record.sequence().get() {
                    return Err(Error::Corrupt("control replay revision differs from its event"));
                }
                let successor_digest = sha256(&successor.canonical_bytes()?).into_bytes();
                if committed_successor.is_some_and(|digest| digest != successor_digest) {
                    return Err(Error::Corrupt(
                        "control event successor differs from immutable replay",
                    ));
                }
                if committed_successor.is_none() {
                    cursor.legacy_records.insert(successor.revision(), successor.clone());
                }
                if requested_revision == Some(successor.revision()) {
                    requested_record = Some(successor.clone());
                }
                cursor.head = Some(event_binding(record));
            }
            self.write_replay_checkpoint(id, aggregate, &cursor)?;
        }
        let replay = self.finish_replay(id, aggregate, cursor)?;
        Ok(SuffixOutcome::Complete(replay, requested_record))
    }

    fn finish_replay(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        cursor: ReplayCursor,
    ) -> Result<ConversationReplay, Error> {
        let durable_head = self
            .journal
            .head(aggregate)?
            .ok_or(Error::Corrupt("control aggregate head disappeared"))?;
        let root = self
            .journal
            .state_record(ROOT_NAMESPACE, id.as_bytes())?
            .ok_or(Error::Corrupt("control projection root disappeared"))?;
        let binding = cursor.head.ok_or(Error::Corrupt("control replay did not reach its head"))?;
        let current = cursor
            .replay
            .current()
            .ok_or(Error::Corrupt("control replay has no current state"))?;
        // This validates aggregate-event identity and exact command-batch publication even when a
        // legacy event cannot independently bind its successor digest.
        let bound = self.bound_projection_root(
            id,
            aggregate,
            &root,
            Some(durable_head),
            false,
        )?;
        let (authoritative, replay_witness) =
            self.decode_control_projection_evidence(root.bytes())?;
        let replay_bytes = cursor.replay.checkpoint_bytes()?;
        if !binding_matches_head(binding, durable_head)
            || root.revision() != current.revision()
            || root.revision() != binding.sequence
            || &authoritative != current
            || bound
                .as_ref()
                .is_some_and(|projection| &projection.record != current)
            || replay_witness
                .as_deref()
                .is_some_and(|witness| witness != replay_bytes.as_slice())
        {
            return Err(Error::Corrupt("control projection does not match immutable replay"));
        }
        self.write_replay_checkpoint(id, aggregate, &cursor)?;
        Ok(cursor.replay)
    }

    fn write_replay_checkpoint(
        &self,
        id: ConversationId,
        aggregate: AggregateKey,
        cursor: &ReplayCursor,
    ) -> Result<(), Error> {
        let (Some(projections), Some(head)) = (self.projections.as_ref(), cursor.head) else {
            return Ok(());
        };
        let Some(bound) =
            self.bound_projection_revision(id, aggregate, head.sequence, true)?
        else {
            return Ok(());
        };
        let replay = cursor.replay.checkpoint_bytes()?;
        if bound.event != head
            || cursor.replay.current() != Some(&bound.record)
            || bound.replay.as_deref() != Some(replay.as_slice())
        {
            return Err(Error::Corrupt(
                "replay checkpoint differs from journal-owned historical witness",
            ));
        }
        if let Err(error) = projections.write_current(
            id,
            &CurrentProjection { head, root: bound.root, replay },
        ) {
            use std::io::Write as _;
            let _ = writeln!(
                std::io::stderr().lock(),
                "conversation replay checkpoint write failed: {error}"
            );
        }
        Ok(())
    }

    pub(super) fn verify_request_archive(
        &self,
        operation: &ControlOperation,
        position: u64,
        before: Option<&ConversationRecord>,
    ) -> Result<(), Error> {
        self.verify_request_archive_source(operation, position, before, None)
    }

    fn verify_request_archive_source(
        &self,
        operation: &ControlOperation,
        position: u64,
        before: Option<&ConversationRecord>,
        historical: Option<&ConversationRecord>,
    ) -> Result<(), Error> {
        use peritus_product_runner::control::{ControlIntent, QueueIntent};
        if matches!(operation.intent(), ControlIntent::RecordInitialization { .. }) {
            return self.verify_initialization_archive(operation, position);
        }
        if matches!(
            operation.intent(),
            ControlIntent::AttachFile { .. }
                | ControlIntent::AttachFileSource { .. }
                | ControlIntent::RefreshFile { .. }
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
            return super::replies::verify_reply_publication(self, reply, position);
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
        super::super::inputs::verify_manifest(operation, manifest.bytes(), before, historical)
    }

    fn request_source_revision(
        &self,
        operation: &ControlOperation,
        position: u64,
    ) -> Result<Option<u64>, Error> {
        use peritus_product_runner::control::{ControlIntent, QueueIntent};
        if !matches!(operation.intent(), ControlIntent::Queue(QueueIntent::Incorporate { .. })) {
            return Ok(None);
        }
        let manifest = self
            .journal
            .state_record(MANIFEST_NAMESPACE, operation.id().as_bytes())?
            .ok_or(Error::Corrupt("incorporation manifest missing"))?;
        if manifest.revision() != 1 || manifest.producing_position() != position {
            return Err(Error::Corrupt(
                "incorporation manifest was not published atomically",
            ));
        }
        super::super::inputs::manifest_source_revision(manifest.bytes())
    }

    /// Loads one retained or replay-only automatic checkpoint after exact projection validation.
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

fn event_binding(record: &CommittedRecord) -> EventBinding {
    EventBinding {
        sequence: record.sequence().get(),
        id: *record.event_id().as_bytes(),
        hash: record.event_hash().into_bytes(),
        global_position: record.global_position(),
    }
}

fn binding_matches_head(binding: EventBinding, head: AggregateHead) -> bool {
    binding.sequence == head.sequence().get()
        && binding.id == *head.event_id().as_bytes()
        && binding.hash == head.event_hash().into_bytes()
}

fn record_follows(previous: Option<EventBinding>, record: &CommittedRecord) -> bool {
    previous.map_or_else(
        || {
            record.sequence().get() == 1
                && record.previous_event_id().is_none()
                && record.previous_event_hash().into_bytes() == [0; 32]
        },
        |previous| {
            previous.sequence.checked_add(1) == Some(record.sequence().get())
                && record.previous_event_id().map(|id| *id.as_bytes()) == Some(previous.id)
                && record.previous_event_hash().into_bytes() == previous.hash
        },
    )
}
