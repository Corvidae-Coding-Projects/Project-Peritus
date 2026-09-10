//! Pure revision-fenced project-guidance lifecycle transitions.

use super::{
    WorkbenchGuidanceContent, WorkbenchGuidanceIdentity, WorkbenchGuidanceLifecycle,
    WorkbenchGuidancePrior, WorkbenchGuidanceRecord, WorkbenchGuidanceScope,
    WorkbenchGuidanceTombstone, WorkbenchGuidanceValidation, WorkbenchGuidanceVersion, capacity,
    invalid, stale,
};
use crate::{AppProtocolError, ControlOperationId};
use peritus_types::WorkspaceId;

impl WorkbenchGuidanceRecord {
    /// Reconstructs a checked active record from an exact durable projection.
    ///
    /// # Errors
    /// Rejects provenance or validation that does not bind the exact content.
    pub fn new(
        identity: WorkbenchGuidanceIdentity,
        version: WorkbenchGuidanceVersion,
        content: WorkbenchGuidanceContent,
        pinned: bool,
        last_validation: WorkbenchGuidanceValidation,
    ) -> Result<Self, AppProtocolError> {
        if last_validation.content_digest() != content.digest() {
            return Err(invalid());
        }
        Ok(Self { identity, version, content, pinned, last_validation })
    }

    /// Creates revision one from an explicit save operation.
    ///
    /// # Errors
    /// Rejects dependency revision overflow or invalid content provenance.
    pub fn save(
        operation: ControlOperationId,
        workspace: WorkspaceId,
        save: WorkbenchGuidanceSave,
    ) -> Result<Self, AppProtocolError> {
        let dependency = save.expected_dependency_revision().checked_add(1).ok_or_else(capacity)?;
        let digest = save.content().digest();
        let pinned = save.pinned();
        Self::new(
            WorkbenchGuidanceIdentity::new(operation, workspace),
            WorkbenchGuidanceVersion::new(1, dependency)?,
            save.into_content(),
            pinned,
            WorkbenchGuidanceValidation::new(operation, digest),
        )
    }

    /// Returns stable guidance identity and workspace binding.
    #[must_use]
    pub const fn identity(&self) -> WorkbenchGuidanceIdentity {
        self.identity
    }

    /// Returns record and dependent-view revisions.
    #[must_use]
    pub const fn version(&self) -> WorkbenchGuidanceVersion {
        self.version
    }

    /// Borrows exact text, source, and scope.
    #[must_use]
    pub const fn content(&self) -> &WorkbenchGuidanceContent {
        &self.content
    }

    /// Reports explicit retrieval priority; pinning never grants authority.
    #[must_use]
    pub const fn pinned(&self) -> bool {
        self.pinned
    }

    /// Returns the most recent exact content validation.
    #[must_use]
    pub const fn last_validation(&self) -> WorkbenchGuidanceValidation {
        self.last_validation
    }

    /// Returns the active lifecycle.
    #[must_use]
    pub const fn lifecycle(&self) -> WorkbenchGuidanceLifecycle {
        WorkbenchGuidanceLifecycle::Active
    }

    /// Produces a checked replacement content revision.
    ///
    /// # Errors
    /// Rejects stale identities/revisions, dependency races, or revision overflow.
    pub fn revise(
        &self,
        operation: ControlOperationId,
        change: WorkbenchGuidanceRevision,
    ) -> Result<Self, AppProtocolError> {
        self.check_selection(change.selection(), change.expected_dependency_revision())?;
        let version = self.next_version(change.expected_dependency_revision())?;
        let digest = change.content().digest();
        Self::new(
            self.identity,
            version,
            change.into_content(),
            self.pinned,
            WorkbenchGuidanceValidation::new(operation, digest),
        )
    }

    /// Produces a checked pin-state revision without revalidating unchanged text.
    ///
    /// # Errors
    /// Rejects stale revisions, dependency races, no-op changes, or overflow.
    pub fn set_pinned(&self, change: WorkbenchGuidancePin) -> Result<Self, AppProtocolError> {
        self.check_selection(change.selection(), change.expected_dependency_revision())?;
        if self.pinned == change.pinned() {
            return Err(invalid());
        }
        Ok(Self {
            identity: self.identity,
            version: self.next_version(change.expected_dependency_revision())?,
            content: self.content.clone(),
            pinned: change.pinned(),
            last_validation: self.last_validation,
        })
    }

    /// Produces a checked project/conversation scope revision.
    ///
    /// # Errors
    /// Rejects stale revisions, dependency races, no-op changes, or overflow.
    pub fn set_scope(
        &self,
        change: WorkbenchGuidanceScopeChange,
    ) -> Result<Self, AppProtocolError> {
        self.check_selection(change.selection(), change.expected_dependency_revision())?;
        if self.content.scope() == change.scope() {
            return Err(invalid());
        }
        let content = WorkbenchGuidanceContent::new(
            self.content.text().clone(),
            self.content.source(),
            change.scope(),
        )?;
        Ok(Self {
            identity: self.identity,
            version: self.next_version(change.expected_dependency_revision())?,
            content,
            pinned: self.pinned,
            last_validation: self.last_validation,
        })
    }

    /// Creates a content-free tombstone that dominates this exact revision.
    ///
    /// # Errors
    /// Rejects stale revisions, dependency races, or revision overflow.
    pub fn forget(
        &self,
        operation: ControlOperationId,
        change: WorkbenchGuidanceForget,
    ) -> Result<WorkbenchGuidanceTombstone, AppProtocolError> {
        self.check_selection(change.selection(), change.expected_dependency_revision())?;
        let dependency_revision =
            change.expected_dependency_revision().checked_add(1).ok_or_else(capacity)?;
        Ok(WorkbenchGuidanceTombstone {
            identity: self.identity,
            prior: WorkbenchGuidancePrior::new(
                self.version.record(),
                self.content.digest(),
                self.content.scope(),
                self.pinned,
            )?,
            forgotten_by: operation,
            reason: change.into_reason(),
            dependency_revision,
        })
    }

    fn check_selection(
        &self,
        selection: WorkbenchGuidanceSelection,
        expected_dependency_revision: u64,
    ) -> Result<(), AppProtocolError> {
        if selection.id() != self.identity.id()
            || selection.expected_revision() != self.version.record()
            || expected_dependency_revision < self.version.dependency()
        {
            return Err(stale());
        }
        Ok(())
    }

    fn next_version(
        &self,
        expected_dependency_revision: u64,
    ) -> Result<WorkbenchGuidanceVersion, AppProtocolError> {
        WorkbenchGuidanceVersion::new(
            self.version.record().checked_add(1).ok_or_else(capacity)?,
            expected_dependency_revision.checked_add(1).ok_or_else(capacity)?,
        )
    }
}

/// Selected guidance identity and exact expected record revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceSelection {
    id: ControlOperationId,
    expected_revision: u64,
}

impl WorkbenchGuidanceSelection {
    /// Validates a positive selected revision.
    ///
    /// # Errors
    /// Rejects revision zero.
    pub const fn new(
        id: ControlOperationId,
        expected_revision: u64,
    ) -> Result<Self, AppProtocolError> {
        if expected_revision == 0 {
            return Err(invalid());
        }
        Ok(Self { id, expected_revision })
    }

    /// Returns stable guidance identity.
    #[must_use]
    pub const fn id(self) -> ControlOperationId {
        self.id
    }

    /// Returns the exact inspected record revision.
    #[must_use]
    pub const fn expected_revision(self) -> u64 {
        self.expected_revision
    }
}

/// Explicit initial guidance save.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceSave {
    expected_dependency_revision: u64,
    content: WorkbenchGuidanceContent,
    pinned: bool,
}

impl WorkbenchGuidanceSave {
    /// Binds a save to the inspected workspace dependency revision.
    #[must_use]
    pub const fn new(
        expected_dependency_revision: u64,
        content: WorkbenchGuidanceContent,
        pinned: bool,
    ) -> Self {
        Self { expected_dependency_revision, content, pinned }
    }

    /// Returns the exact inspected dependent-view revision.
    #[must_use]
    pub const fn expected_dependency_revision(&self) -> u64 {
        self.expected_dependency_revision
    }

    /// Borrows exact content and provenance.
    #[must_use]
    pub const fn content(&self) -> &WorkbenchGuidanceContent {
        &self.content
    }

    /// Reports initial explicit pin state.
    #[must_use]
    pub const fn pinned(&self) -> bool {
        self.pinned
    }

    fn into_content(self) -> WorkbenchGuidanceContent {
        self.content
    }
}

/// Explicit replacement of an exact active guidance revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceRevision {
    selection: WorkbenchGuidanceSelection,
    expected_dependency_revision: u64,
    content: WorkbenchGuidanceContent,
}

impl WorkbenchGuidanceRevision {
    /// Creates a revision-fenced replacement.
    #[must_use]
    pub const fn new(
        selection: WorkbenchGuidanceSelection,
        expected_dependency_revision: u64,
        content: WorkbenchGuidanceContent,
    ) -> Self {
        Self { selection, expected_dependency_revision, content }
    }

    /// Returns exact selected identity and record revision.
    #[must_use]
    pub const fn selection(&self) -> WorkbenchGuidanceSelection {
        self.selection
    }

    /// Returns the exact inspected dependent-view revision.
    #[must_use]
    pub const fn expected_dependency_revision(&self) -> u64 {
        self.expected_dependency_revision
    }

    /// Borrows replacement text, source, and scope.
    #[must_use]
    pub const fn content(&self) -> &WorkbenchGuidanceContent {
        &self.content
    }

    fn into_content(self) -> WorkbenchGuidanceContent {
        self.content
    }
}

/// Explicit pin-state change for one exact record revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidancePin {
    selection: WorkbenchGuidanceSelection,
    expected_dependency_revision: u64,
    pinned: bool,
}

impl WorkbenchGuidancePin {
    /// Creates a revision-fenced pin change.
    #[must_use]
    pub const fn new(
        selection: WorkbenchGuidanceSelection,
        expected_dependency_revision: u64,
        pinned: bool,
    ) -> Self {
        Self { selection, expected_dependency_revision, pinned }
    }

    /// Returns exact selected identity and record revision.
    #[must_use]
    pub const fn selection(self) -> WorkbenchGuidanceSelection {
        self.selection
    }

    /// Returns the exact inspected dependent-view revision.
    #[must_use]
    pub const fn expected_dependency_revision(self) -> u64 {
        self.expected_dependency_revision
    }

    /// Returns desired future-retrieval priority.
    #[must_use]
    pub const fn pinned(self) -> bool {
        self.pinned
    }
}

/// Explicit scope change within the already-bound project.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchGuidanceScopeChange {
    selection: WorkbenchGuidanceSelection,
    expected_dependency_revision: u64,
    scope: WorkbenchGuidanceScope,
}

impl WorkbenchGuidanceScopeChange {
    /// Creates a revision-fenced scope change; no cross-workspace value exists.
    #[must_use]
    pub const fn new(
        selection: WorkbenchGuidanceSelection,
        expected_dependency_revision: u64,
        scope: WorkbenchGuidanceScope,
    ) -> Self {
        Self { selection, expected_dependency_revision, scope }
    }

    /// Returns exact selected identity and record revision.
    #[must_use]
    pub const fn selection(self) -> WorkbenchGuidanceSelection {
        self.selection
    }

    /// Returns the exact inspected dependent-view revision.
    #[must_use]
    pub const fn expected_dependency_revision(self) -> u64 {
        self.expected_dependency_revision
    }

    /// Returns the new within-project scope.
    #[must_use]
    pub const fn scope(self) -> WorkbenchGuidanceScope {
        self.scope
    }
}

mod forget;
pub use forget::WorkbenchGuidanceForget;
