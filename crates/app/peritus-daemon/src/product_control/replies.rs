//! Exact public reply delivery and source-bound reconstruction for later user turns.

use super::{ControlStore, ControlStoreError as Error};
use peritus_product_runner::control::{ControlOperation, PublicReplyReference};

impl ControlStore {
    /// Retains one exact public host-delivered reply after the last admitted invocation.
    /// This is not a claim of provider completion, goal completion, or execution authority.
    pub fn publish_reply(&mut self, start: &ControlOperation, text: &str) -> Result<(), Error> {
        let digest = peritus_codec::sha256(text.as_bytes());
        let prepared = self.prepare_public_reply(start, digest, text.len() as u64)?;
        if prepared.retained() {
            let existing = self.reply_text(prepared.reference())?;
            return if existing == text {
                Ok(())
            } else {
                Err(peritus_product_runner::control::ControlError::IdempotencyConflict.into())
            };
        }
        let published = prepared.publish(text)?;
        self.accept_published_reply(published)?;
        Ok(())
    }
}

pub(super) fn verify_text(reference: &PublicReplyReference, bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() as u64 != reference.bytes()
        || bytes.is_empty()
        || peritus_codec::sha256(bytes) != reference.digest()
        || std::str::from_utf8(bytes).is_err()
    {
        return Err(Error::Corrupt("public reply bytes differ from their immutable reference"));
    }
    Ok(())
}
