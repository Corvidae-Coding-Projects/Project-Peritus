//! Inspectable effective workspace policy and bounded user restriction changes.

use crate::{AppErrorCode, AppProtocolError, WorkbenchQuery};

/// Capability whose effective state can be inspected or narrowed.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WorkbenchPermissionCapability {
    /// Authorized workspace inspection.
    Read,
    /// Authorized workspace mutation.
    Write,
    /// Owned process execution and control.
    Process,
    /// Provider or process activity that may use the network.
    Network,
}

impl WorkbenchPermissionCapability {
    /// Canonical projection order.
    pub const ALL: [Self; 4] = [Self::Read, Self::Write, Self::Process, Self::Network];

    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::Read => 1,
            Self::Write => 2,
            Self::Process => 3,
            Self::Network => 4,
        }
    }

    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Read),
            2 => Some(Self::Write),
            3 => Some(Self::Process),
            4 => Some(Self::Network),
            _ => None,
        }
    }

    /// Stable user-facing label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Process => "process",
            Self::Network => "network",
        }
    }
}

/// Workspace trust origin resolved by the host, never by the client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchWorkspaceTrust {
    /// Registered managed workspace.
    Managed,
    /// Explicit direct folder without in-place mutation authority.
    DirectReadOnly,
    /// Explicit direct folder with in-place mutation authority.
    DirectWritable,
}

impl WorkbenchWorkspaceTrust {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::Managed => 1,
            Self::DirectReadOnly => 2,
            Self::DirectWritable => 3,
        }
    }
    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Managed),
            2 => Some(Self::DirectReadOnly),
            3 => Some(Self::DirectWritable),
            _ => None,
        }
    }
    /// Stable public label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed workspace",
            Self::DirectReadOnly => "trusted direct folder · read-only",
            Self::DirectWritable => "trusted direct folder · in-place writes",
        }
    }
}

/// Provenance of one displayed effective decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchPermissionProvenance {
    /// The configured workspace or folder trust ceiling controls this value.
    WorkspaceHostPolicy,
    /// The configured tool inventory controls this value.
    ToolHostPolicy,
    /// The configured provider inventory controls this value.
    ProviderHostPolicy,
    /// An explicit user restriction narrows an otherwise available capability.
    UserRestriction,
}

impl WorkbenchPermissionProvenance {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::WorkspaceHostPolicy => 1,
            Self::ToolHostPolicy => 2,
            Self::ProviderHostPolicy => 3,
            Self::UserRestriction => 4,
        }
    }
    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::WorkspaceHostPolicy),
            2 => Some(Self::ToolHostPolicy),
            3 => Some(Self::ProviderHostPolicy),
            4 => Some(Self::UserRestriction),
            _ => None,
        }
    }
    /// Stable explanatory label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceHostPolicy => "configured workspace trust",
            Self::ToolHostPolicy => "configured tool allowlist",
            Self::ProviderHostPolicy => "configured provider routes",
            Self::UserRestriction => "explicit user restriction",
        }
    }
}

/// One exact user-requested overlay change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchPermissionChange {
    expected_authority_revision: u64,
    capability: WorkbenchPermissionCapability,
    allowed: bool,
}

impl WorkbenchPermissionChange {
    /// Binds the mutation to the inspected workspace authority revision.
    #[must_use]
    pub const fn new(
        expected_authority_revision: u64,
        capability: WorkbenchPermissionCapability,
        allowed: bool,
    ) -> Self {
        Self { expected_authority_revision, capability, allowed }
    }
    /// Returns the exact inspected policy revision.
    #[must_use]
    pub const fn expected_authority_revision(self) -> u64 {
        self.expected_authority_revision
    }
    /// Returns the selected capability.
    #[must_use]
    pub const fn capability(self) -> WorkbenchPermissionCapability {
        self.capability
    }
    /// Returns the requested overlay value.
    #[must_use]
    pub const fn allowed(self) -> bool {
        self.allowed
    }
}

/// One inspectable host/effective capability row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchPermissionEntry {
    capability: WorkbenchPermissionCapability,
    host_allowed: bool,
    effective_allowed: bool,
    provenance: WorkbenchPermissionProvenance,
    explicit_approval_still_required: bool,
}

impl WorkbenchPermissionEntry {
    /// Constructs a row without granting authority.
    ///
    /// # Errors
    /// Rejects an effective grant above its host ceiling.
    pub const fn new(
        capability: WorkbenchPermissionCapability,
        host_allowed: bool,
        effective_allowed: bool,
        provenance: WorkbenchPermissionProvenance,
        explicit_approval_still_required: bool,
    ) -> Result<Self, AppProtocolError> {
        if effective_allowed && !host_allowed {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self {
            capability,
            host_allowed,
            effective_allowed,
            provenance,
            explicit_approval_still_required,
        })
    }
    /// Returns the capability.
    #[must_use]
    pub const fn capability(self) -> WorkbenchPermissionCapability {
        self.capability
    }
    /// Returns the immutable host ceiling.
    #[must_use]
    pub const fn host_allowed(self) -> bool {
        self.host_allowed
    }
    /// Returns the actually enforced intersection.
    #[must_use]
    pub const fn effective_allowed(self) -> bool {
        self.effective_allowed
    }
    /// Returns the source controlling the displayed decision.
    #[must_use]
    pub const fn provenance(self) -> WorkbenchPermissionProvenance {
        self.provenance
    }
    /// Reports that lower signed operation/revision approval remains mandatory where applicable.
    #[must_use]
    pub const fn explicit_approval_still_required(self) -> bool {
        self.explicit_approval_still_required
    }
}

/// Complete revisioned effective workspace permission inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchPermissions {
    query: WorkbenchQuery,
    conversation_revision: u64,
    authority_revision: u64,
    trust: WorkbenchWorkspaceTrust,
    entries: [WorkbenchPermissionEntry; 4],
}

impl WorkbenchPermissions {
    /// Constructs a canonical four-row projection.
    ///
    /// # Errors
    /// Rejects absent conversations or missing/reordered capability rows.
    pub fn new(
        query: WorkbenchQuery,
        conversation_revision: u64,
        authority_revision: u64,
        trust: WorkbenchWorkspaceTrust,
        entries: [WorkbenchPermissionEntry; 4],
    ) -> Result<Self, AppProtocolError> {
        if conversation_revision == 0
            || entries.iter().map(|entry| entry.capability()).ne(WorkbenchPermissionCapability::ALL)
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { query, conversation_revision, authority_revision, trust, entries })
    }
    /// Returns exact conversation/workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the selected conversation revision.
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 {
        self.conversation_revision
    }
    /// Returns the workspace restriction revision.
    #[must_use]
    pub const fn authority_revision(&self) -> u64 {
        self.authority_revision
    }
    /// Returns host workspace trust provenance.
    #[must_use]
    pub const fn trust(&self) -> WorkbenchWorkspaceTrust {
        self.trust
    }
    /// Borrows four canonical capability rows.
    #[must_use]
    pub const fn entries(&self) -> &[WorkbenchPermissionEntry; 4] {
        &self.entries
    }
}
