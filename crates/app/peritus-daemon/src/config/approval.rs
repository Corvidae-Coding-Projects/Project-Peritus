//! Public approval credential-registry configuration.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::DaemonError;

use super::invalid;

/// Required public credential-registry payload and monotonic lineage generation.
///
/// The payload contains only B1 public credential material. Private signing keys are deliberately
/// outside daemon configuration and persistence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRegistryDeclaration {
    payload_file: PathBuf,
    generation: u64,
}

impl ApprovalRegistryDeclaration {
    /// Creates a checked public registry declaration.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless the payload path is absolute and lexically normalized and the
    /// configured lineage generation is positive.
    pub fn new(payload_file: PathBuf, generation: u64) -> Result<Self, DaemonError> {
        let declaration = Self { payload_file, generation };
        declaration.validate()?;
        Ok(declaration)
    }

    /// Borrows the absolute canonical-payload file declaration.
    #[must_use]
    pub fn payload_file(&self) -> &Path {
        &self.payload_file
    }

    /// Returns the positive same-key lineage generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn validate(&self) -> Result<(), DaemonError> {
        let normalized = self.payload_file.components().collect::<PathBuf>();
        if !self.payload_file.is_absolute()
            || self
                .payload_file
                .components()
                .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
            || normalized.as_os_str() != self.payload_file.as_os_str()
            || self.generation == 0
        {
            return Err(invalid(
                "approval registry path must be absolute and lexically normalized and generation must be positive",
            ));
        }
        Ok(())
    }
}
