//! Bounded immutable version history and explicit future-request selection.

use super::{ControlError, ControlText, FileAttachment, FileMode, FileVersion, OperationId};
use crate::{
    attachment::{MAX_FILE_COUNT, MAX_FILE_SELECTION_BYTES},
    control::{InputLedger, InputSelection, QueueIntent},
};
use peritus_types::ActorId;
use serde::Deserialize;
use serde::Serialize;

/// Original reference, retained refresh history, and current inclusion preference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileSelection {
    file: FileAttachment,
    selected: bool,
    refreshes: Vec<FileVersion>,
    #[serde(default)]
    frozen: bool,
}
impl FileSelection {
    /// Returns effective read behavior; historical branches retain snapshots, never refresh grants.
    #[must_use]
    pub const fn mode(&self) -> FileMode {
        if self.frozen { FileMode::Snapshot } else { self.file.source().mode() }
    }
    /// Borrows the original source and consent binding.
    #[must_use]
    pub const fn file(&self) -> &FileAttachment {
        &self.file
    }
    /// Returns explicit future inclusion preference, separate from queue eligibility.
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }
    /// Borrows the current immutable version; prior versions remain available.
    #[must_use]
    pub fn current(&self) -> &FileVersion {
        self.refreshes.last().unwrap_or_else(|| self.file.initial())
    }
    /// Borrows every refresh observation in publication order.
    #[must_use]
    pub fn refreshes(&self) -> &[FileVersion] {
        &self.refreshes
    }
}

/// Bounded file history; mutations are proposals until atomically committed by the host.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileAttachments {
    entries: Vec<FileSelection>,
}
impl FileAttachments {
    /// Retains exact source/version history while disabling future reads in a historical branch.
    #[must_use]
    pub fn historical_snapshot(&self) -> Self {
        let mut snapshot = self.clone();
        for entry in &mut snapshot.entries {
            entry.frozen = true;
        }
        snapshot
    }
    /// Reports absence of all retained file references, including deselected history.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Borrows retained references and their immutable version histories.
    #[must_use]
    pub fn entries(&self) -> &[FileSelection] {
        &self.entries
    }
    /// Selects only references whose captions participate in the exact input capture.
    #[must_use]
    pub fn eligible(&self, included: &[InputSelection]) -> Vec<&FileSelection> {
        self.entries
            .iter()
            .filter(|entry| {
                entry.selected && included.iter().any(|input| input.id() == entry.file.input())
            })
            .collect()
    }
    pub(in crate::control) fn attach(
        &mut self,
        inputs: &mut InputLedger,
        actor: ActorId,
        file: &FileAttachment,
        text: &ControlText<8192>,
    ) -> Result<(), ControlError> {
        file.validate()?;
        if self.entries.len() >= 256 {
            return Err(ControlError::Capacity);
        }
        *inputs = inputs.apply(
            actor,
            &QueueIntent::Enqueue {
                id: file.input(),
                text: text.clone(),
                dependencies: Vec::new(),
            },
        )?;
        self.entries.push(FileSelection {
            file: file.clone(),
            selected: true,
            refreshes: Vec::new(),
            frozen: false,
        });
        self.validate(inputs)
    }
    pub(in crate::control) fn select(
        &mut self,
        inputs: &mut InputLedger,
        attachment: OperationId,
        selected: bool,
    ) -> Result<(), ControlError> {
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.file.operation() == attachment)
            .ok_or(ControlError::NotFound)?;
        entry.selected = selected;
        *inputs = inputs.context_changed()?;
        self.validate(inputs)
    }
    pub(in crate::control) fn refresh(
        &mut self,
        inputs: &mut InputLedger,
        attachment: OperationId,
        previous: OperationId,
        version: &FileVersion,
    ) -> Result<(), ControlError> {
        let capture = inputs.capture()?;
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.file.operation() == attachment)
            .ok_or(ControlError::NotFound)?;
        if entry.current().operation() != previous {
            return Err(ControlError::StaleRevision);
        }
        if !entry.selected
            || entry.mode() != FileMode::RefreshOnRequest
            || !capture.included().iter().any(|input| input.id() == entry.file.input())
        {
            return Err(ControlError::InvalidInput);
        }
        version.validate(entry.file.source())?;
        entry.refreshes.push(version.clone());
        *inputs = inputs.context_changed()?;
        self.validate(inputs)
    }
    pub(in crate::control) fn validate(&self, inputs: &InputLedger) -> Result<(), ControlError> {
        let mut operations = std::collections::BTreeSet::new();
        let mut versions = 0_usize;
        for entry in &self.entries {
            entry.file.validate()?;
            if inputs.latest(entry.file.input()).is_none()
                || !operations.insert(entry.file.operation())
                || (!entry.refreshes.is_empty()
                    && entry.file.source().mode() != FileMode::RefreshOnRequest)
            {
                return Err(ControlError::InvalidInput);
            }
            versions = versions.saturating_add(1 + entry.refreshes.len());
            for version in &entry.refreshes {
                version.validate(entry.file.source())?;
                if !operations.insert(version.operation()) {
                    return Err(ControlError::InvalidInput);
                }
            }
        }
        if self.entries.len() > 256 || versions > 1024 {
            return Err(ControlError::Capacity);
        }
        let capture = inputs.capture()?;
        let eligible = self.eligible(capture.included());
        let bytes = eligible.iter().try_fold(0_u64, |bytes, entry| {
            bytes.checked_add(entry.current().observation().bytes()).ok_or(ControlError::Capacity)
        })?;
        if eligible.len() > MAX_FILE_COUNT || bytes > MAX_FILE_SELECTION_BYTES {
            return Err(ControlError::Capacity);
        }
        Ok(())
    }
}
