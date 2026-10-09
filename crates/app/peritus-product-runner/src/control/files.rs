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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    shared_input: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input_revision: Option<u64>,
}
impl FileAttachment {
    /// Binds checked metadata, not a read authorization or an atomic artifact publication.
    ///
    /// # Errors
    /// Rejects source/version mismatch or invalid immutable metadata.
    pub fn new(source: FileSource, initial: FileVersion) -> Result<Self, ControlError> {
        let value = Self {
            input: source_input(initial.operation())?,
            source,
            initial,
            shared_input: false,
            input_revision: None,
        };
        value.validate()?;
        Ok(value)
    }
    /// Binds an attachment to the same immutable input as its atomically submitted message.
    ///
    /// # Errors
    /// Rejects invalid immutable source/version bindings; the host must commit the input together.
    pub fn for_message(
        source: FileSource,
        initial: FileVersion,
        input: InputId,
    ) -> Result<Self, ControlError> {
        Self::for_selection(source, initial, crate::control::InputSelection::new(input, 1)?)
    }
    /// Binds exact immutable bytes to one input content revision.
    ///
    /// # Errors
    /// Rejects invalid source/version bindings. Later edits do not inherit these bytes.
    pub fn for_selection(
        source: FileSource,
        initial: FileVersion,
        selected: crate::control::InputSelection,
    ) -> Result<Self, ControlError> {
        let value = Self {
            input: selected.id(),
            source,
            initial,
            shared_input: true,
            input_revision: Some(selected.revision()),
        };
        value.validate()?;
        Ok(value)
    }
    /// Reports whether this source shares its atomically admitted message's queue identity.
    #[must_use]
    pub const fn shares_message_input(&self) -> bool {
        self.shared_input
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
    /// Returns the exact input content revision for shared message sources.
    #[must_use]
    pub const fn input_revision(&self) -> Option<u64> {
        self.input_revision
    }
    /// Checks whether an input capture selects this exact source revision.
    #[must_use]
    pub fn matches_input(&self, input: crate::control::InputSelection) -> bool {
        input.id() == self.input
            && self.input_revision.is_none_or(|revision| revision == input.revision())
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
        if (!self.shared_input && self.input != source_input(self.operation())?)
            || self.input_revision == Some(0)
            || (!self.shared_input && self.input_revision.is_some())
        {
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
