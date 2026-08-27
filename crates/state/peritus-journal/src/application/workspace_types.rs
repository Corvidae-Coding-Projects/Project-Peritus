//! Checked application-workspace catalog values.

use peritus_types::{Sha256Digest, WorkspaceId};

use crate::{JournalError, JournalErrorKind};

/// Maximum number of workspace registrations returned by one recovery page.
pub const MAX_APPLICATION_WORKSPACE_PAGE: usize = 4_096;

/// Maximum canonical bytes retained for one application workspace registration.
pub const MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES: usize = 1_048_576;

/// Application workspace registration state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationWorkspaceState {
    /// The workspace is registered and may admit work.
    Registered,
    /// Registration is retained but the workspace is temporarily unavailable.
    Unavailable,
    /// Registration history is retained but new work is forbidden.
    Removed,
}

impl ApplicationWorkspaceState {
    pub(super) const fn tag(self) -> i64 {
        match self {
            Self::Registered => 1,
            Self::Unavailable => 2,
            Self::Removed => 3,
        }
    }

    pub(super) const fn from_tag(tag: i64) -> Option<Self> {
        match tag {
            1 => Some(Self::Registered),
            2 => Some(Self::Unavailable),
            3 => Some(Self::Removed),
            _ => None,
        }
    }
}

/// New exact workspace registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewApplicationWorkspace {
    pub(super) workspace_id: WorkspaceId,
    pub(super) registration_bytes: Vec<u8>,
    pub(super) registration_digest: Sha256Digest,
}

impl NewApplicationWorkspace {
    /// Creates a bounded exact registration.
    ///
    /// # Errors
    ///
    /// Returns invalid input unless the registration contains 1 byte through 1 MiB.
    pub fn new(
        workspace_id: WorkspaceId,
        registration_bytes: Vec<u8>,
        registration_digest: Sha256Digest,
    ) -> Result<Self, JournalError> {
        if registration_bytes.is_empty()
            || registration_bytes.len() > MAX_APPLICATION_WORKSPACE_REGISTRATION_BYTES
        {
            return Err(invalid(
                "application workspace registration is outside the production bound",
            ));
        }
        if peritus_codec::sha256(&registration_bytes) != registration_digest {
            return Err(invalid(
                "application workspace registration digest differs from its bytes",
            ));
        }
        Ok(Self { workspace_id, registration_bytes, registration_digest })
    }
}

/// One durable exact workspace registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationWorkspace {
    pub(super) workspace_id: WorkspaceId,
    pub(super) registration_bytes: Vec<u8>,
    pub(super) registration_digest: Sha256Digest,
    pub(super) state: ApplicationWorkspaceState,
}

impl ApplicationWorkspace {
    /// Returns the workspace identity.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Borrows exact retained registration bytes.
    #[must_use]
    pub fn registration_bytes(&self) -> &[u8] {
        &self.registration_bytes
    }

    /// Returns the exact registration digest.
    #[must_use]
    pub const fn registration_digest(&self) -> Sha256Digest {
        self.registration_digest
    }

    /// Returns workspace lifecycle state.
    #[must_use]
    pub const fn state(&self) -> ApplicationWorkspaceState {
        self.state
    }
}

/// One bounded deterministic page of durable workspace registrations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationWorkspacePage {
    pub(super) workspaces: Vec<ApplicationWorkspace>,
    pub(super) next_after: Option<WorkspaceId>,
}

impl ApplicationWorkspacePage {
    /// Borrows workspace registrations in strict identity order.
    #[must_use]
    pub fn workspaces(&self) -> &[ApplicationWorkspace] {
        &self.workspaces
    }

    /// Returns the exclusive identity cursor for the next page, when more rows exist.
    #[must_use]
    pub const fn next_after(&self) -> Option<WorkspaceId> {
        self.next_after
    }

    /// Consumes the page and returns its ordered registrations.
    #[must_use]
    pub fn into_workspaces(self) -> Vec<ApplicationWorkspace> {
        self.workspaces
    }
}

const fn invalid(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "validate application ledger value", detail)
}
