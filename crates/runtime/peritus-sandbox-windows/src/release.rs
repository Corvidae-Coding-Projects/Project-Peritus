//! Final backend-owned Windows teardown evidence.

/// Explicit state of one backend cleanup dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupState {
    /// Cleanup has not yet completed or failed.
    Pending,
    /// Absence or successful teardown has been established.
    Complete,
    /// The last attempt failed and retained evidence requires retry or reconciliation.
    RetryRequired,
}

/// Partial cleanup evidence retained even when release returns an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseProgress {
    acl: CleanupState,
    proxy: CleanupState,
}

impl ReleaseProgress {
    pub(crate) const fn new(acl: CleanupState, proxy: CleanupState) -> Self {
        Self { acl, proxy }
    }

    /// Returns exact ACL reversal progress.
    #[must_use]
    pub const fn acl(self) -> CleanupState {
        self.acl
    }

    /// Returns managed-proxy teardown progress.
    #[must_use]
    pub const fn proxy(self) -> CleanupState {
        self.proxy
    }
}

/// Final backend-local release evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent teardown evidence remains explicit and inspectable"
)]
pub struct ReleaseReport {
    pub(crate) acl_restored: bool,
    pub(crate) secret_files_removed: bool,
    pub(crate) helper_reaped: bool,
    pub(crate) handles_closed: bool,
    pub(crate) proxy_joined: bool,
    pub(crate) network_filter_removed: bool,
}

impl ReleaseReport {
    /// Reports complete backend-owned cleanup.
    #[must_use]
    pub const fn complete(self) -> bool {
        crate::verified::teardown_complete(
            true,
            self.helper_reaped,
            self.acl_restored,
            self.secret_files_removed,
            self.handles_closed,
            self.proxy_joined && self.network_filter_removed,
        )
    }

    /// Reports exact ACL reversal completion.
    #[must_use]
    pub const fn acl_restored(self) -> bool {
        self.acl_restored
    }
    /// Reports private secret-file removal.
    #[must_use]
    pub const fn secret_files_removed(self) -> bool {
        self.secret_files_removed
    }
    /// Reports helper reap completion.
    #[must_use]
    pub const fn helper_reaped(self) -> bool {
        self.helper_reaped
    }
    /// Reports protected inherited-handle closure.
    #[must_use]
    pub const fn handles_closed(self) -> bool {
        self.handles_closed
    }
    /// Reports managed proxy worker joins.
    #[must_use]
    pub const fn proxy_joined(self) -> bool {
        self.proxy_joined
    }

    /// Reports removal of the session-owned dynamic WFP filters.
    #[must_use]
    pub const fn network_filter_removed(self) -> bool {
        self.network_filter_removed
    }
}
