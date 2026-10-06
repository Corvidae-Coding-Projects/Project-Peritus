//! Bounded checkpoint lineage and restore-journal records without retained file bodies.

use super::{CheckpointId, ControlError, ControlText, RestoreId};
use peritus_types::Sha256Digest;
use serde::Deserialize;
use serde::Serialize;

mod path;
mod version;
pub use path::{CheckpointCoverage, CheckpointPath, CheckpointRange};
pub use version::{CheckpointFileMode, CheckpointFileVersion, CheckpointVersion};

/// Stable historical references consumed by later conversation branching.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointReferences {
    source_conversation_revision: u64,
    context_generation: u64,
    brief_revision: u64,
    goal_revision: Option<u64>,
}
impl CheckpointReferences {
    /// Binds a checkpoint to inspected conversation, context, brief, and optional goal revisions.
    #[must_use]
    pub const fn new(
        source_conversation_revision: u64,
        context_generation: u64,
        brief_revision: u64,
        goal_revision: Option<u64>,
    ) -> Self {
        Self { source_conversation_revision, context_generation, brief_revision, goal_revision }
    }
    /// Returns the original conversation revision without rewriting its history.
    #[must_use]
    pub const fn source_conversation_revision(&self) -> u64 {
        self.source_conversation_revision
    }
    /// Returns the exact context generation referenced by the checkpoint.
    #[must_use]
    pub const fn context_generation(&self) -> u64 {
        self.context_generation
    }
    /// Returns the greatest user-confirmed brief input revision at capture time.
    #[must_use]
    pub const fn brief_revision(&self) -> u64 {
        self.brief_revision
    }
    /// Returns the governing goal revision when a goal existed; restore never applies it.
    #[must_use]
    pub const fn goal_revision(&self) -> Option<u64> {
        self.goal_revision
    }
}

/// A user checkpoint manifest. File bodies are content-addressed journal artifacts, not fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserCheckpoint {
    id: CheckpointId,
    name: ControlText<256>,
    references: CheckpointReferences,
    paths: Vec<CheckpointPath>,
    exclusions: Vec<ControlText<512>>,
    external_effects: Vec<ControlText<512>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    automatic_run: Option<[u8; 16]>,
    sealed_by_run: Option<[u8; 16]>,
}
impl UserCheckpoint {
    /// Constructs bounded visible coverage without credentials, authority, or process handles.
    ///
    /// # Errors
    /// Rejects empty names, invalid/duplicate paths, or manifest count bounds.
    pub fn new(
        id: CheckpointId,
        name: String,
        references: CheckpointReferences,
        mut paths: Vec<CheckpointPath>,
        exclusions: Vec<String>,
        external_effects: Vec<String>,
    ) -> Result<Self, ControlError> {
        if u16::try_from(paths.len()).is_err()
            || u16::try_from(exclusions.len()).is_err()
            || u16::try_from(external_effects.len()).is_err()
        {
            return Err(ControlError::Capacity);
        }
        paths.sort_by(|left, right| left.path().cmp(right.path()));
        if paths.windows(2).any(|pair| pair[0].path() == pair[1].path()) {
            return Err(ControlError::InvalidInput);
        }
        let value = Self {
            id,
            name: ControlText::new(name)?,
            references,
            paths,
            exclusions: exclusions.into_iter().map(ControlText::new).collect::<Result<_, _>>()?,
            external_effects: external_effects
                .into_iter()
                .map(ControlText::new)
                .collect::<Result<_, _>>()?,
            automatic_run: None,
            sealed_by_run: None,
        };
        value.validate()?;
        Ok(value)
    }
    /// Constructs a host-origin automatic checkpoint for one owned run.
    ///
    /// # Errors
    /// Applies the ordinary checkpoint bounds and rejects the reserved zero run identity.
    pub fn automatic(
        id: CheckpointId,
        name: String,
        references: CheckpointReferences,
        paths: Vec<CheckpointPath>,
        exclusions: Vec<String>,
        external_effects: Vec<String>,
        run: [u8; 16],
    ) -> Result<Self, ControlError> {
        if run == [0; 16] {
            return Err(ControlError::InvalidInput);
        }
        let mut value = Self::new(id, name, references, paths, exclusions, external_effects)?;
        value.automatic_run = Some(run);
        Ok(value)
    }
    /// Returns checkpoint identity.
    #[must_use]
    pub const fn id(&self) -> CheckpointId {
        self.id
    }
    /// Borrows the user-selected name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    /// Borrows stable historical references for conversation branching.
    #[must_use]
    pub const fn references(&self) -> &CheckpointReferences {
        &self.references
    }
    /// Borrows exact covered paths in canonical order.
    #[must_use]
    pub fn paths(&self) -> &[CheckpointPath] {
        &self.paths
    }
    /// Borrows visible exclusions.
    pub fn exclusions(&self) -> super::ControlTextIter<'_, 512> {
        self.exclusions.iter().map(ControlText::as_str)
    }
    /// Borrows external effects that rewind cannot undo.
    pub fn external_effects(&self) -> super::ControlTextIter<'_, 512> {
        self.external_effects.iter().map(ControlText::as_str)
    }
    /// Returns the completed owned execution boundary that sealed expected post-change bytes.
    #[must_use]
    pub const fn sealed_by_run(&self) -> Option<[u8; 16]> {
        self.sealed_by_run
    }
    /// Returns the host-bound run when this is an automatic pre-mutation checkpoint.
    #[must_use]
    pub const fn automatic_run(&self) -> Option<[u8; 16]> {
        self.automatic_run
    }
    /// Returns the immutable capture-time manifest used by the original journal operation.
    ///
    /// Later owned-run sealing is a separate operation and is deliberately removed here so an
    /// idempotency lookup can reproduce the original command digest exactly.
    #[must_use]
    pub fn capture_manifest(&self) -> Self {
        let mut captured = self.clone();
        captured.sealed_by_run = None;
        for path in &mut captured.paths {
            path.unseal();
        }
        captured
    }
    pub(super) fn seal(
        &mut self,
        run: [u8; 16],
        versions: &[(String, CheckpointFileVersion)],
    ) -> Result<(), ControlError> {
        if run == [0; 16] || versions.len() != self.paths.len() {
            return Err(ControlError::InvalidInput);
        }
        if self.sealed_by_run.is_some_and(|existing| existing != run) {
            return Err(ControlError::IdempotencyConflict);
        }
        for (path, (name, version)) in self.paths.iter_mut().zip(versions) {
            if path.path() != name {
                return Err(ControlError::InvalidInput);
            }
            path.seal(*version)?;
        }
        self.sealed_by_run = Some(run);
        Ok(())
    }
    pub(in crate::control) fn seal_restoration(
        &mut self,
        versions: &[(String, CheckpointFileVersion)],
    ) -> Result<(), ControlError> {
        if self.automatic_run.is_some()
            || self.sealed_by_run.is_some()
            || self.paths.len() != versions.len()
        {
            return Err(ControlError::InvalidInput);
        }
        for (path, (name, version)) in self.paths.iter_mut().zip(versions) {
            if path.path() != name {
                return Err(ControlError::InvalidInput);
            }
            path.seal(*version)?;
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), ControlError> {
        if u16::try_from(self.paths.len()).is_err()
            || u16::try_from(self.exclusions.len()).is_err()
            || u16::try_from(self.external_effects.len()).is_err()
            || self.paths.windows(2).any(|pair| pair[0].path() >= pair[1].path())
        {
            return Err(ControlError::Capacity);
        }
        if self.automatic_run == Some([0; 16]) {
            return Err(ControlError::InvalidInput);
        }
        for path in &self.paths {
            path.validate()?;
        }
        Ok(())
    }
}

mod restore;
pub use restore::{RestoreOperation, RestoreStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_run_can_advance_the_exact_owned_postimage_but_another_run_cannot() {
        let mut checkpoint = UserCheckpoint::new(
            CheckpointId::new([1; 16]).expect("checkpoint ID"),
            "before mutation".to_owned(),
            CheckpointReferences::new(1, 1, 1, None),
            vec![
                CheckpointPath::new("artifact.txt".to_owned(), CheckpointFileVersion::Absent)
                    .expect("path"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("checkpoint");
        let first = CheckpointFileVersion::present(
            Sha256Digest::new([2; 32]),
            5,
            CheckpointFileMode::Regular,
        );
        let second = CheckpointFileVersion::present(
            Sha256Digest::new([3; 32]),
            6,
            CheckpointFileMode::Regular,
        );

        checkpoint.seal([4; 16], &[("artifact.txt".to_owned(), first)]).expect("first seal");
        checkpoint.seal([4; 16], &[("artifact.txt".to_owned(), second)]).expect("same run");

        assert_eq!(checkpoint.paths()[0].owned_postchange(), Some(second));
        assert_eq!(
            checkpoint.seal([5; 16], &[("artifact.txt".to_owned(), first)]),
            Err(ControlError::IdempotencyConflict)
        );
    }
}
