//! Exact public reply delivery and source-bound reconstruction for later user turns.

use super::{ControlStore, ControlStoreError as Error};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, OperationId, PublicReplyReference,
};

impl ControlStore {
    /// Retains one exact public host-delivered reply after the last admitted invocation.
    /// This is not a claim of provider completion, goal completion, or execution authority.
    pub fn publish_reply(&mut self, start: &ControlOperation, text: &str) -> Result<(), Error> {
        let record = self.execution_record(start)?;
        let invocation =
            record.inputs().invocations().last().ok_or(ControlError::InvalidInput)?.invocation();
        let digest = peritus_codec::sha256(text.as_bytes());
        if let Some(prior) =
            record.replies().iter().find(|reply| reply.after_invocation() == invocation)
        {
            return if prior.digest() == digest && self.reply_text(prior)? == text {
                Ok(())
            } else {
                Err(ControlError::IdempotencyConflict.into())
            };
        }
        let mut identity = b"peritus-workbench/public-reply/v1".to_vec();
        identity.extend_from_slice(start.id().as_bytes());
        identity.extend_from_slice(invocation.as_bytes());
        let hash = peritus_codec::sha256(&identity);
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&hash.as_bytes()[..16]);
        bytes[0] |= 1;
        let id = OperationId::new(bytes)?;
        let reply =
            PublicReplyReference::new(id, invocation, digest.into_bytes(), text.len() as u64)?;
        let operation = ControlOperation::new(
            id,
            start.conversation(),
            peritus_types::ActorId::new(*start.actor_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            peritus_types::WorkspaceId::new(*start.workspace_bytes())
                .map_err(|_| ControlError::InvalidInput)?,
            record.revision(),
            ControlIntent::PublishReply(reply),
        );
        self.accept_reply(&operation, text.as_bytes().to_vec()).map(|_| ())
    }
}

pub(super) fn verify_text(reference: &PublicReplyReference, bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() as u64 != reference.bytes()
        || bytes.is_empty()
        || bytes.len() > 1024 * 1024
        || peritus_codec::sha256(bytes) != reference.digest()
        || std::str::from_utf8(bytes).is_err()
    {
        return Err(Error::Corrupt("public reply bytes differ from their immutable reference"));
    }
    Ok(())
}
