//! Atomic fork publication and rebuildable conversation-catalog projection.

use super::{
    ControlStore, Error, FRAME_FAMILY, MAX_RECORDS, RECEIPT_NAMESPACE, ROOT_NAMESPACE, aggregate,
    command_id, event_id,
};
use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_journal::{
    AppendRequest, CommandResolution, EventDraft, ExactFrame, HeadExpectation, StateInstall,
};
use peritus_product_runner::control::{
    ControlError, ControlOperation, ControlReceipt, ConversationBranch, ConversationId,
    ConversationRecord,
};
use peritus_types::{EventId, EventSequence};
use std::collections::BTreeMap;

pub(super) const BRANCH_NAMESPACE: u16 = 3520;
const MAX_LIBRARY_EVENTS: usize = 65_536;

impl ControlStore {
    /// Atomically reserves source budget and publishes the independent non-running child.
    pub fn accept_fork(
        &mut self,
        source: &ControlOperation,
        child: &ControlOperation,
        branch: &ConversationBranch,
    ) -> Result<ControlReceipt, Error> {
        if source.id() != child.id()
            || source.id() != branch.operation()
            || source.conversation() != branch.source()
            || child.conversation() != branch.child()
            || source.actor_bytes() != child.actor_bytes()
        {
            return Err(ControlError::InvalidInput.into());
        }
        if let Some(receipt) = self.resolve_fork(source, child, branch)? {
            self.verify_fork(branch)?;
            return Ok(receipt);
        }
        let source_current = self.load(source.conversation())?.ok_or(ControlError::NotFound)?;
        self.check_reserved_child(child.conversation(), Some(source))?;
        if self.load(child.conversation())?.is_some() {
            return Err(ControlError::IdempotencyConflict.into());
        }
        let (source_next, receipt) = ConversationRecord::apply(Some(&source_current), source)?;
        let (child_next, _) = ConversationRecord::apply(None, child)?;
        if source_next.revision() > MAX_RECORDS || child_next.revision() > MAX_RECORDS {
            return Err(ControlError::Capacity.into());
        }
        let source_payload = source.canonical_bytes()?;
        let child_payload = child.canonical_bytes()?;
        let branch_payload = branch.canonical_bytes()?;
        let source_aggregate = aggregate(source.conversation())?;
        let child_aggregate = aggregate(child.conversation())?;
        let source_head = self.journal.head(source_aggregate)?;
        let child_head = self.journal.head(child_aggregate)?;
        if child_head.is_some() {
            return Err(ControlError::IdempotencyConflict.into());
        }
        let source_event = EventDraft::new(
            source_aggregate,
            EventSequence::new(source_next.revision()).map_err(|_| ControlError::Capacity)?,
            event_id(source)?,
            source_head.map(peritus_journal::AggregateHead::event_id),
            frame(&source_payload)?,
            sha256(source.workspace_bytes()),
            Vec::new(),
        )?;
        let child_event = EventDraft::new(
            child_aggregate,
            EventSequence::new(child_next.revision()).map_err(|_| ControlError::Capacity)?,
            child_event_id(child)?,
            None,
            frame(&child_payload)?,
            sha256(child.workspace_bytes()),
            Vec::new(),
        )?;
        let mut heads = vec![
            source_head.map_or(HeadExpectation::Absent(source_aggregate), HeadExpectation::Present),
            HeadExpectation::Absent(child_aggregate),
        ];
        heads.sort_by_key(|head| head.key());
        let mut events = vec![source_event, child_event];
        events.sort_by_key(EventDraft::aggregate);
        let mut installs = vec![
            StateInstall::new(
                ROOT_NAMESPACE,
                source.conversation().as_bytes().to_vec(),
                Some(source_current.revision()),
                source_next.revision(),
                source_next.canonical_bytes()?,
            )?,
            StateInstall::new(
                ROOT_NAMESPACE,
                child.conversation().as_bytes().to_vec(),
                None,
                child_next.revision(),
                child_next.canonical_bytes()?,
            )?,
            StateInstall::new(
                RECEIPT_NAMESPACE,
                source.id().as_bytes().to_vec(),
                None,
                1,
                receipt.canonical_bytes()?,
            )?,
            StateInstall::new(
                BRANCH_NAMESPACE,
                child.conversation().as_bytes().to_vec(),
                None,
                1,
                branch_payload,
            )?,
        ];
        installs.sort_by(|left, right| {
            (left.namespace(), left.key()).cmp(&(right.namespace(), right.key()))
        });
        let request_binding = fork_binding(source, child, branch)?;
        let plan = AppendRequest::new(
            self.store,
            command_id(source)?,
            sha256(&request_binding),
            heads,
            events,
            installs,
            Vec::new(),
            None,
            None,
            Vec::new(),
        )
        .plan()?;
        match self.journal.append(plan) {
            Ok(_) => {
                self.verify_fork(branch)?;
                self.resolve_fork(source, child, branch)?
                    .ok_or(Error::Corrupt("committed fork receipt missing"))
            }
            Err(error) => match self.resolve_fork(source, child, branch)? {
                Some(receipt) => {
                    self.verify_fork(branch)?;
                    Ok(receipt)
                }
                None => Err(error.into()),
            },
        }
    }

    /// Resolves only the exact three-part fork command fingerprint.
    pub fn resolve_fork(
        &self,
        source: &ControlOperation,
        child: &ControlOperation,
        branch: &ConversationBranch,
    ) -> Result<Option<ControlReceipt>, Error> {
        let binding = fork_binding(source, child, branch)?;
        match self.journal.resolve_command(command_id(source)?, sha256(&binding))? {
            CommandResolution::DefinitelyAbsent => Ok(None),
            CommandResolution::Conflict { .. } => Err(ControlError::IdempotencyConflict.into()),
            CommandResolution::Committed(batch) => {
                let row = self
                    .journal
                    .state_record(RECEIPT_NAMESPACE, source.id().as_bytes())?
                    .ok_or(Error::Corrupt("accepted fork has no durable receipt"))?;
                if row.producing_position() != batch.last_position() || row.revision() != 1 {
                    return Err(Error::Corrupt("fork receipt was not published atomically"));
                }
                self.verify_fork(branch)?;
                Ok(Some(ControlReceipt::resolve(row.bytes(), source)?))
            }
        }
    }

    /// Loads exact child lineage, if the conversation is a fork.
    pub fn branch(&self, child: ConversationId) -> Result<Option<ConversationBranch>, Error> {
        let Some(record) = self.journal.state_record(BRANCH_NAMESPACE, child.as_bytes())? else {
            return Ok(None);
        };
        if record.revision() != 1 {
            return Err(Error::Corrupt("invalid conversation branch revision"));
        }
        let branch = ConversationBranch::parse(record.bytes())?;
        if branch.child() != child || self.load(child)?.is_none() {
            return Err(Error::Corrupt("conversation branch projection is detached"));
        }
        Ok(Some(branch))
    }

    /// Rebuilds the bounded conversation catalog from authoritative immutable control events.
    pub fn conversation_ids(&self) -> Result<BTreeMap<ConversationId, u64>, Error> {
        let mut cursor = 0;
        let mut seen = 0usize;
        let mut conversations = BTreeMap::new();
        loop {
            let window = self.journal.global_events_after(cursor, 4096)?;
            if window.has_retention_gap_after(cursor) {
                return Err(Error::Corrupt("control history cannot rebuild conversation index"));
            }
            if window.records().is_empty() {
                break;
            }
            for record in window.records() {
                seen = seen.checked_add(1).ok_or(ControlError::Capacity)?;
                if seen > MAX_LIBRARY_EVENTS {
                    return Err(ControlError::Capacity.into());
                }
                if record.frame_family() != FRAME_FAMILY {
                    return Err(Error::Corrupt("unexpected control history family"));
                }
                let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
                    .map_err(|_| Error::Corrupt("invalid control history frame"))?;
                let operation = ControlOperation::parse(frame.payload())?;
                conversations.insert(operation.conversation(), record.global_position());
                cursor = record.global_position();
            }
        }
        Ok(conversations)
    }

    fn verify_fork(&self, branch: &ConversationBranch) -> Result<(), Error> {
        let stored = self.branch(branch.child())?.ok_or(Error::Corrupt("fork lineage missing"))?;
        if &stored != branch {
            return Err(Error::Corrupt("fork lineage differs from accepted operation"));
        }
        Ok(())
    }
}

fn fork_binding(
    source: &ControlOperation,
    child: &ControlOperation,
    branch: &ConversationBranch,
) -> Result<Vec<u8>, Error> {
    let mut binding = source.canonical_bytes()?;
    binding.extend_from_slice(&child.canonical_bytes()?);
    binding.extend_from_slice(&branch.canonical_bytes()?);
    Ok(binding)
}

fn frame(payload: &[u8]) -> Result<ExactFrame, Error> {
    ExactFrame::new(
        encode_frame(FRAME_FAMILY, 1, payload, CodecLimits::PRODUCTION)
            .map_err(|_| Error::Corrupt("cannot encode bounded fork event"))?,
    )
    .map_err(Into::into)
}

fn child_event_id(operation: &ControlOperation) -> Result<EventId, Error> {
    let mut binding = b"peritus-product-control/fork-child-event/v1".to_vec();
    binding.extend_from_slice(operation.id().as_bytes());
    binding.extend_from_slice(operation.conversation().as_bytes());
    let digest = sha256(&binding);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] |= 1;
    EventId::new(bytes).map_err(|_| ControlError::InvalidInput.into())
}
