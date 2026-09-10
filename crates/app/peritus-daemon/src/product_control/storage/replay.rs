//! Verified immutable replay and exact historical checkpoint reads.

use super::{
    ControlStore, Error, FRAME_FAMILY, MANIFEST_NAMESPACE, MAX_RECORDS, REPLY_NAMESPACE,
    REQUEST_NAMESPACE, ROOT_NAMESPACE, aggregate,
};
use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_product_runner::control::{
    ControlError, ControlOperation, ConversationId, ConversationRecord,
};

impl ControlStore {
    /// Verifies bounded immutable history against the current C0 state root without recovery writes.
    pub fn load(&self, id: ConversationId) -> Result<Option<ConversationRecord>, Error> {
        self.load_at_revision(id, None)
    }

    /// Reads an exact historical state while still verifying the complete current archive.
    pub(crate) fn load_revision(
        &self,
        id: ConversationId,
        revision: u64,
    ) -> Result<Option<ConversationRecord>, Error> {
        self.load_at_revision(id, Some(revision))
    }

    fn load_at_revision(
        &self,
        id: ConversationId,
        revision: Option<u64>,
    ) -> Result<Option<ConversationRecord>, Error> {
        let aggregate = aggregate(id)?;
        if self.journal.head(aggregate)?.is_some_and(|head| head.sequence().get() > MAX_RECORDS) {
            return Err(ControlError::Capacity.into());
        }
        let (records, root) = self
            .journal
            .aggregate_checkpoint_snapshot(aggregate, ROOT_NAMESPACE, id.as_bytes())?
            .into_parts();
        let mut current = None;
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
            let (next, _) = ConversationRecord::apply(current.as_ref(), &operation)?;
            self.verify_request_archive(&operation, record.global_position(), current.as_ref())?;
            if revision == Some(next.revision()) {
                selected = Some(next.clone());
            }
            current = Some(next);
        }
        match (&current, &root, records.last()) {
            (None, None, None) => Ok(None),
            (Some(current), Some(root), Some(last))
                if root.revision() == current.revision()
                    // A fork publishes two aggregate events in one atomic batch. State installs
                    // identify the batch's last global position, while either aggregate event may
                    // precede it inside that same batch.
                    && root.producing_position() >= last.global_position()
                    && root.bytes() == current.canonical_bytes()? =>
            {
                Ok(if revision.is_some() { selected } else { Some(current.clone()) })
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
            ControlIntent::AttachFile { .. } | ControlIntent::RefreshFile { .. }
        ) {
            return self.verify_file_archive(operation, position);
        }
        if matches!(
            operation.intent(),
            ControlIntent::CreateCheckpoint(_)
                | ControlIntent::PrepareRestore { .. }
                | ControlIntent::SettleRestore { .. }
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
}
