//! Durable user context preferences over exact, already-admitted sources.

use super::{
    ControlError, ConversationRecord, InputSelection, InputState, InvocationId, OperationId,
};
use serde::Deserialize;
use serde::Serialize;

/// Exact source selected in the context inspector.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ContextTarget {
    /// A current immutable user-input revision. Inputs may be pinned but never excluded here.
    Input(InputSelection),
    /// A host-published public reply after the named invocation.
    PublicReply(InvocationId),
    /// An immutable image import operation.
    Image(OperationId),
    /// A file reference operation; the current version is resolved when the request is captured.
    File(OperationId),
}

/// Explicit next-request treatment. Absence from the ledger means ordinary host policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPreference {
    /// Retain the source even when it would otherwise be optional or awaiting a later turn.
    Pinned,
    /// Exclude an optional source without deleting its immutable history.
    Excluded,
}

/// One canonical source preference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSelection {
    target: ContextTarget,
    preference: ContextPreference,
}
impl ContextSelection {
    /// Returns the exact selected source.
    #[must_use]
    pub const fn target(self) -> ContextTarget {
        self.target
    }
    /// Returns the explicit next-request preference.
    #[must_use]
    pub const fn preference(self) -> ContextPreference {
        self.preference
    }
}

/// Bounded canonical context-preference ledger.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSelections {
    entries: Vec<ContextSelection>,
}
impl ContextSelections {
    /// Returns whether no explicit source preference is active.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Borrows canonical current preferences.
    #[must_use]
    pub fn entries(&self) -> &[ContextSelection] {
        &self.entries
    }
    /// Returns a source's active preference, if any.
    #[must_use]
    pub fn preference(&self, target: ContextTarget) -> Option<ContextPreference> {
        self.entries
            .binary_search_by_key(&target, |entry| entry.target)
            .ok()
            .map(|index| self.entries[index].preference)
    }

    pub(super) fn set(
        &mut self,
        record: &ConversationRecord,
        target: ContextTarget,
        preference: Option<ContextPreference>,
    ) -> Result<(), ControlError> {
        validate_target(record, target, preference)?;
        match self.entries.binary_search_by_key(&target, |entry| entry.target) {
            Ok(index) => match preference {
                Some(value) => self.entries[index].preference = value,
                None => {
                    self.entries.remove(index);
                }
            },
            Err(index) => {
                if let Some(value) = preference {
                    if self.entries.len() >= 1024 {
                        return Err(ControlError::Capacity);
                    }
                    self.entries.insert(index, ContextSelection { target, preference: value });
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate(&self, record: &ConversationRecord) -> Result<(), ControlError> {
        if self.entries.len() > 1024
            || self.entries.windows(2).any(|pair| pair[0].target >= pair[1].target)
        {
            return Err(ControlError::InvalidInput);
        }
        for entry in &self.entries {
            validate_target(record, entry.target, Some(entry.preference))?;
        }
        Ok(())
    }
}

fn validate_target(
    record: &ConversationRecord,
    target: ContextTarget,
    preference: Option<ContextPreference>,
) -> Result<(), ControlError> {
    match target {
        ContextTarget::Input(selected) => {
            let input = record.inputs().latest(selected.id()).ok_or(ControlError::NotFound)?;
            if input.selection() != selected
                || !matches!(input.state(), InputState::Queued | InputState::Incorporated)
                || preference == Some(ContextPreference::Excluded)
            {
                return Err(ControlError::InvalidInput);
            }
        }
        ContextTarget::PublicReply(invocation) => {
            if !record.replies().iter().any(|reply| reply.after_invocation() == invocation) {
                return Err(ControlError::NotFound);
            }
        }
        ContextTarget::Image(operation) => {
            if !record.images().entries().iter().any(|entry| entry.image().operation() == operation)
            {
                return Err(ControlError::NotFound);
            }
        }
        ContextTarget::File(operation) => {
            if !record.files().entries().iter().any(|entry| entry.file().operation() == operation) {
                return Err(ControlError::NotFound);
            }
        }
    }
    Ok(())
}
