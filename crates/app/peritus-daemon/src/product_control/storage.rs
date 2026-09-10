//! Exact operation replay, expected-revision state publication and original-receipt recovery.

use super::ControlStoreError as Error;
use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, EventDraft,
    ExactFrame, HeadExpectation, SqliteJournal, SqliteJournalOptions, StateInstall, StoreId,
};
use peritus_product_runner::control::{
    ControlError, ControlOperation, ControlReceipt, ConversationId, ConversationRecord, OperationId,
};
use peritus_types::{CommandId, EventId, EventSequence};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    time::Duration,
};

const FRAME_FAMILY: u16 = 3402;
const ROOT_NAMESPACE: u16 = 3402;
const RECEIPT_NAMESPACE: u16 = 3403;
const REQUEST_NAMESPACE: u16 = 3404;
const MANIFEST_NAMESPACE: u16 = 3405;
const REPLY_NAMESPACE: u16 = 3406;
const HOST_GOAL_OPERATION_NAMESPACE: u16 = 3420;
const MAX_RECORDS: u64 = 16_384;
const MAX_DATABASE_BYTES: u64 = 256 * 1024 * 1024;

mod checkpoints;
mod files;
mod guidance;
mod images;
mod initialization;
mod library;
mod permissions;
mod replay;
mod reservations;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub struct ControlStore {
    journal: SqliteJournal,
    store: StoreId,
    // Fields are dropped in declaration order: close the journal before releasing ownership.
    _owner: ControlOwner,
}

struct ControlOwner(File);

impl Drop for ControlOwner {
    fn drop(&mut self) {
        // File close alone leaves an advisory lock held by temporarily inherited/duplicated
        // handles. Only this guard owns the lock; explicitly release it after journal close.
        if let Err(error) = self.0.unlock() {
            use std::io::Write as _;
            let _ = std::io::stderr()
                .lock()
                .write_all(format!("control-store ownership unlock failed: {error}\n").as_bytes());
        }
    }
}

impl ControlStore {
    /// Opens the caller-validated private control generation; never opens legacy run JSON.
    pub fn open(root: &Path, store: StoreId) -> Result<Self, Error> {
        std::fs::create_dir_all(root)?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))?;
        owner.try_lock().map_err(|error| Error::Io(error.into()))?;
        let owner = ControlOwner(owner);
        let mut journal = SqliteJournal::open(
            root.join("control.sqlite3"),
            store,
            SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
        )?;
        let pages = journal.storage_pages()?;
        journal.limit_storage_pages(MAX_DATABASE_BYTES / pages.page_size())?;
        Ok(Self { journal, store, _owner: owner })
    }

    /// Atomically publishes exact intent, successor state and original receipt through C0.
    pub fn accept(&mut self, operation: &ControlOperation) -> Result<ControlReceipt, Error> {
        if matches!(
            operation.intent(),
            peritus_product_runner::control::ControlIntent::SetPermissions { .. }
                | peritus_product_runner::control::ControlIntent::UpdateGuidance(_)
                | peritus_product_runner::control::ControlIntent::RecordInitialization { .. }
        ) {
            return Err(ControlError::InvalidInput.into());
        }
        self.accept_archived(operation, None)
    }

    pub(super) fn host_goal_operation(
        &self,
        id: OperationId,
    ) -> Result<Option<ControlOperation>, Error> {
        let Some(record) =
            self.journal.state_record(HOST_GOAL_OPERATION_NAMESPACE, id.as_bytes())?
        else {
            return Ok(None);
        };
        if record.revision() != 1 {
            return Err(Error::Corrupt("invalid host goal operation revision"));
        }
        let operation = ControlOperation::parse(record.bytes())?;
        if operation.id() != id {
            return Err(Error::Corrupt("host goal operation identity mismatch"));
        }
        Ok(Some(operation))
    }

    pub(super) fn accept_host_goal_operation(
        &mut self,
        operation: &ControlOperation,
    ) -> Result<ControlReceipt, Error> {
        self.accept_installs(
            operation,
            vec![StateInstall::new(
                HOST_GOAL_OPERATION_NAMESPACE,
                operation.id().as_bytes().to_vec(),
                None,
                1,
                operation.canonical_bytes()?,
            )?],
        )
    }

    pub(super) fn accept_archived(
        &mut self,
        operation: &ControlOperation,
        archive: Option<super::inputs::RequestArchive>,
    ) -> Result<ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let archived = super::inputs::validate_archive(operation, archive)?;
        let mut installs = Vec::new();
        if let Some((request, manifest)) = archived {
            for (namespace, bytes) in [(REQUEST_NAMESPACE, request), (MANIFEST_NAMESPACE, manifest)]
            {
                installs.push(StateInstall::new(
                    namespace,
                    operation.id().as_bytes().to_vec(),
                    None,
                    1,
                    bytes,
                )?);
            }
        }
        self.accept_installs(operation, installs)
    }

    pub(super) fn accept_reply(
        &mut self,
        operation: &ControlOperation,
        text: Vec<u8>,
    ) -> Result<ControlReceipt, Error> {
        use peritus_product_runner::control::ControlIntent;
        let ControlIntent::PublishReply(reply) = operation.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        super::replies::verify_text(reply, &text)?;
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        self.accept_installs(
            operation,
            vec![StateInstall::new(
                REPLY_NAMESPACE,
                operation.id().as_bytes().to_vec(),
                None,
                1,
                text,
            )?],
        )
    }

    fn accept_installs(
        &mut self,
        operation: &ControlOperation,
        mut artifacts: Vec<StateInstall>,
    ) -> Result<ControlReceipt, Error> {
        let current = self.load(operation.conversation())?;
        self.check_creation_reservation(operation, current.is_none())?;
        let (next, receipt) = ConversationRecord::apply(current.as_ref(), operation)?;
        if next.revision() > MAX_RECORDS {
            return Err(ControlError::Capacity.into());
        }
        let payload = operation.canonical_bytes()?;
        let aggregate = aggregate(operation.conversation())?;
        let head = self.journal.head(aggregate)?;
        let frame = ExactFrame::new(
            encode_frame(FRAME_FAMILY, 1, &payload, CodecLimits::PRODUCTION)
                .map_err(|_| Error::Corrupt("cannot encode bounded control event"))?,
        )?;
        let event = EventDraft::new(
            aggregate,
            EventSequence::new(next.revision()).map_err(|_| ControlError::Capacity)?,
            event_id(operation)?,
            head.map(peritus_journal::AggregateHead::event_id),
            frame,
            sha256(operation.workspace_bytes()),
            Vec::new(),
        )?;
        let mut installs = vec![
            StateInstall::new(
                ROOT_NAMESPACE,
                operation.conversation().as_bytes().to_vec(),
                current.as_ref().map(ConversationRecord::revision),
                next.revision(),
                next.canonical_bytes()?,
            )?,
            StateInstall::new(
                RECEIPT_NAMESPACE,
                operation.id().as_bytes().to_vec(),
                None,
                1,
                receipt.canonical_bytes()?,
            )?,
        ];
        installs.append(&mut artifacts);
        let plan = AppendRequest::new(
            self.store,
            command_id(operation)?,
            sha256(&payload),
            vec![head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present)],
            vec![event],
            installs,
            Vec::new(),
            None,
            None,
            Vec::new(),
        )
        .plan()?;
        // Even an indeterminate append is resolved by its original exact identity. A caller
        // never receives success solely from the pre-commit pure transition above.
        match self.journal.append(plan) {
            Ok(_) => self.resolve(operation)?.ok_or(Error::Corrupt("committed receipt missing")),
            Err(error) => match self.resolve(operation)? {
                Some(receipt) => Ok(receipt),
                None => Err(error.into()),
            },
        }
    }

    pub fn resolve(&self, operation: &ControlOperation) -> Result<Option<ControlReceipt>, Error> {
        match self
            .journal
            .resolve_command(command_id(operation)?, sha256(&operation.canonical_bytes()?))?
        {
            CommandResolution::DefinitelyAbsent => Ok(None),
            CommandResolution::Conflict { .. } => Err(ControlError::IdempotencyConflict.into()),
            CommandResolution::Committed(batch) => {
                let row = self
                    .journal
                    .state_record(RECEIPT_NAMESPACE, operation.id().as_bytes())?
                    .ok_or(Error::Corrupt("accepted operation has no durable receipt"))?;
                if row.producing_position() != batch.last_position() || row.revision() != 1 {
                    return Err(Error::Corrupt("receipt was not published with its operation"));
                }
                self.verify_request_archive(operation, batch.last_position(), None)?;
                Ok(Some(ControlReceipt::resolve(row.bytes(), operation)?))
            }
        }
    }

    /// Finds the exact accepted operation by its client/host identity within one conversation.
    pub fn operation(
        &self,
        conversation: ConversationId,
        id: OperationId,
    ) -> Result<Option<ControlOperation>, Error> {
        let (records, _) = self
            .journal
            .aggregate_checkpoint_snapshot(
                aggregate(conversation)?,
                ROOT_NAMESPACE,
                conversation.as_bytes(),
            )?
            .into_parts();
        for record in records.into_iter().rev() {
            if record.command_id().as_bytes() != id.as_bytes() {
                continue;
            }
            let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| Error::Corrupt("invalid control event frame"))?;
            let operation = ControlOperation::parse(frame.payload())?;
            if operation.id() != id || operation.conversation() != conversation {
                return Err(Error::Corrupt("control operation lookup mismatch"));
            }
            return Ok(Some(operation));
        }
        Ok(None)
    }

    /// Reads only the exact semantic request accepted by an authenticated original operation.
    /// The archive excludes credentials and the transport request ID by protocol definition.
    pub fn accepted_request(&self, operation: &ControlOperation) -> Result<Option<Vec<u8>>, Error> {
        if self.resolve(operation)?.is_none() {
            return Ok(None);
        }
        Ok(self
            .journal
            .state_record(REQUEST_NAMESPACE, operation.id().as_bytes())?
            .map(|record| record.bytes().to_vec()))
    }

    pub(crate) fn reply_text(
        &self,
        reference: &peritus_product_runner::control::PublicReplyReference,
    ) -> Result<String, Error> {
        let artifact = self
            .journal
            .state_record(REPLY_NAMESPACE, reference.operation().as_bytes())?
            .ok_or(Error::Corrupt("public reply artifact missing"))?;
        super::replies::verify_text(reference, artifact.bytes())?;
        String::from_utf8(artifact.bytes().to_vec())
            .map_err(|_| Error::Corrupt("invalid public reply encoding"))
    }

    // Only the scoped inspector calls this after authenticating and validating the root.
    pub(super) fn invocation_manifest(
        &self,
        conversation: ConversationId,
        invocation: peritus_product_runner::control::InvocationId,
    ) -> Result<Vec<u8>, Error> {
        use peritus_product_runner::control::{ControlIntent, QueueIntent};
        let (records, _) = self
            .journal
            .aggregate_checkpoint_snapshot(
                aggregate(conversation)?,
                ROOT_NAMESPACE,
                conversation.as_bytes(),
            )?
            .into_parts();
        for record in records {
            let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| Error::Corrupt("invalid control event"))?;
            let operation = ControlOperation::parse(frame.payload())?;
            if matches!(operation.intent(), ControlIntent::Queue(QueueIntent::Incorporate { invocation: bound, .. }) if *bound == invocation)
            {
                self.verify_request_archive(&operation, record.global_position(), None)?;
                return self
                    .journal
                    .state_record(MANIFEST_NAMESPACE, operation.id().as_bytes())?
                    .map(|record| record.bytes().to_vec())
                    .ok_or(Error::Corrupt("sealed manifest missing"));
            }
        }
        Err(ControlError::NotFound.into())
    }
}

fn aggregate(id: ConversationId) -> Result<AggregateKey, Error> {
    Ok(AggregateKey::new(AggregateKind::Application, AggregateId::new(*id.as_bytes())?))
}
fn command_id(operation: &ControlOperation) -> Result<CommandId, Error> {
    CommandId::new(*operation.id().as_bytes()).map_err(|_| ControlError::InvalidInput.into())
}
fn event_id(operation: &ControlOperation) -> Result<EventId, Error> {
    let mut binding = b"peritus-product-control/event/v1".to_vec();
    binding.extend_from_slice(operation.id().as_bytes());
    let digest = sha256(&binding);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] |= 1;
    EventId::new(bytes).map_err(|_| ControlError::InvalidInput.into())
}
