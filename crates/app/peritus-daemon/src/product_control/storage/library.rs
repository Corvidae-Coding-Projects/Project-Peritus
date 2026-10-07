//! Atomic fork publication and rebuildable conversation-catalog projection.

use super::{
    ControlStore, Error, RECEIPT_NAMESPACE, ROOT_NAMESPACE, aggregate, command_id, event_id,
};
use peritus_codec::sha256;
use peritus_journal::{
    AppendRequest, CommandResolution, EventDraft, HeadExpectation, StateInstall,
};
use peritus_product_runner::control::{
    ControlError, ControlOperation, ControlReceipt, ConversationBranch, ConversationId,
    ConversationReplay,
};
use peritus_types::{EventId, EventSequence};
use std::collections::BTreeMap;

pub(super) const BRANCH_NAMESPACE: u16 = 3520;
impl ControlStore {
    /// Atomically reserves source budget and publishes the independent non-running child.
    pub fn accept_fork(
        &mut self,
        source: &ControlOperation,
        child: &ControlOperation,
        branch: &ConversationBranch,
    ) -> Result<ControlReceipt, Error> {
        self.require_conversation_scope(source.conversation())?;
        self.require_conversation_scope(child.conversation())?;
        let _commit = self.generation.acquire_commit(&self.cancellation)?;
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
        if let peritus_product_runner::control::ControlIntent::ReserveAutomaticFork {
            checkpoint,
            ..
        } = source.intent()
            && self.load_checkpoint(source.conversation(), checkpoint.id())?.as_ref()
                != Some(checkpoint.as_ref())
        {
            return Err(ControlError::IdempotencyConflict.into());
        }
        let mut source_replay = self.load_replay(source.conversation())?;
        let source_current = source_replay
            .current()
            .cloned()
            .ok_or(ControlError::NotFound)?;
        self.check_reserved_child(child.conversation(), Some(source))?;
        if self.load(child.conversation())?.is_some() {
            return Err(ControlError::IdempotencyConflict.into());
        }
        let receipt = source_replay.apply(source)?;
        let source_next = source_replay
            .current()
            .cloned()
            .ok_or(Error::Corrupt("fork source successor is missing"))?;
        let mut child_replay = ConversationReplay::default();
        child_replay.apply(child)?;
        let child_next = child_replay
            .current()
            .cloned()
            .ok_or(Error::Corrupt("fork child successor is missing"))?;
        let source_payload = source.canonical_bytes()?;
        let child_payload = child.canonical_bytes()?;
        let mut publications = super::checkpoints::snapshot::ControlPublications::default();
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
            self.control_event(source, &source_payload, &source_next, &mut publications)?,
            sha256(source.workspace_bytes()),
            Vec::new(),
        )?;
        let child_event = EventDraft::new(
            child_aggregate,
            EventSequence::new(child_next.revision()).map_err(|_| ControlError::Capacity)?,
            child_event_id(child)?,
            None,
            self.control_event(child, &child_payload, &child_next, &mut publications)?,
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
                self.control_projection(
                    source,
                    &source_next,
                    &source_replay,
                    &mut publications,
                )?,
            )?,
            StateInstall::new(
                ROOT_NAMESPACE,
                child.conversation().as_bytes().to_vec(),
                None,
                child_next.revision(),
                self.control_projection(
                    child,
                    &child_next,
                    &child_replay,
                    &mut publications,
                )?,
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
        publications.append_installs(&mut installs);
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
        let _leases = self.generation.acquire_publications(
            publications.claims(),
            &self.cancellation,
        )?;
        publications.activate(self)?;
        #[cfg(test)]
        publications.before_root_publication()?;
        let result = match self.journal.append(plan) {
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
        };
        if result.is_ok() {
            publications.finish()?;
            for conversation in [source.conversation(), child.conversation()] {
                if let Err(error) = self.refresh_replay_projection(conversation) {
                    use std::io::Write as _;
                    let _ = writeln!(
                        std::io::stderr().lock(),
                        "conversation projection refresh failed after accepted fork: {error}"
                    );
                }
            }
        }
        result
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

    /// Rebuilds the conversation catalog from current journal-owned projection metadata.
    pub fn conversation_ids(&self) -> Result<BTreeMap<ConversationId, u64>, Error> {
        self.require_index_read_scope()?;
        let mut cursor = None;
        let mut conversations = BTreeMap::new();
        loop {
            let page = self.journal.state_record_metadata_page(
                ROOT_NAMESPACE,
                cursor.as_deref(),
                usize::MAX,
            )?;
            let (records, next) = page.into_parts();
            for record in records {
                let key: [u8; 16] = record
                    .key()
                    .try_into()
                    .map_err(|_| Error::Corrupt("invalid conversation catalog key"))?;
                let id = ConversationId::new(key)
                    .map_err(|_| Error::Corrupt("invalid conversation catalog identity"))?;
                if conversations.insert(id, record.producing_position()).is_some() {
                    return Err(Error::Corrupt("duplicate conversation catalog identity"));
                }
            }
            let Some(next) = next else {
                break;
            };
            cursor = Some(next);
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
