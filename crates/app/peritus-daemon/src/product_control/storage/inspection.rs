//! Exact accepted-operation and immutable request/reply inspection through shared page decoders.

use super::{
    ControlError, ControlOperation, ControlStore, ConversationId, Error, MANIFEST_NAMESPACE,
    OperationId, REQUEST_NAMESPACE, ROOT_NAMESPACE, aggregate,
};

impl ControlStore {
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
            let operation = self.decode_control_event(record.frame_bytes())?;
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

    // Only the scoped inspector calls this after authenticating and validating the root.
    pub(in crate::product_control) fn invocation_manifest(
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
            let operation = self.decode_control_event(record.frame_bytes())?;
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
