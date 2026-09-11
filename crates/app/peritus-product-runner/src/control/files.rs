//! Durable exact file references; source descriptors are inert, never filesystem capabilities.

use super::{ControlError, ControlText, InputId, OperationId};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use serde::Serialize;

mod ledger;
mod source;
#[cfg(test)]
mod tests;
mod version;
pub use ledger::{FileAttachments, FileSelection};
pub use source::{FileMode, FileRange, FileSource};
pub use version::{FileObservation, FileVersion};

/// Initial confirmed reference with a stable caption identity and exact source descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileAttachment {
    input: InputId,
    source: FileSource,
    initial: FileVersion,
}
impl FileAttachment {
    /// Binds checked metadata, not a read authorization or an atomic artifact publication.
    ///
    /// # Errors
    /// Rejects source/version mismatch or invalid immutable metadata.
    pub fn new(source: FileSource, initial: FileVersion) -> Result<Self, ControlError> {
        let value = Self { input: source_input(initial.operation())?, source, initial };
        value.validate()?;
        Ok(value)
    }
    /// Returns the initial import operation, stable across later refreshes.
    #[must_use]
    pub const fn operation(&self) -> OperationId {
        self.initial.operation()
    }
    /// Returns the caption's durable input identity.
    #[must_use]
    pub const fn input(&self) -> InputId {
        self.input
    }
    /// Borrows source identity, range semantics and the original inclusion mode.
    #[must_use]
    pub const fn source(&self) -> &FileSource {
        &self.source
    }
    /// Borrows the originally confirmed immutable observation.
    #[must_use]
    pub const fn initial(&self) -> &FileVersion {
        &self.initial
    }
    pub(super) fn validate(&self) -> Result<(), ControlError> {
        if self.input != source_input(self.operation())? {
            return Err(ControlError::InvalidInput);
        }
        self.source.validate()?;
        self.initial.validate(&self.source)
    }
}

fn source_input(operation: OperationId) -> Result<InputId, ControlError> {
    let mut bytes = b"peritus-file-input-v1\0".to_vec();
    bytes.extend_from_slice(operation.as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    InputId::new(id)
}
