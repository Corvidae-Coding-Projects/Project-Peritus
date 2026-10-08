//! Final backend-owned Windows teardown evidence.

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

const MAX_CLEANUP_CAUSE_BYTES: usize = 512;

/// Explicit state of one backend cleanup dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupState {
    /// Cleanup has not yet completed or failed.
    Pending,
    /// Absence or successful teardown has been established.
    Complete,
    /// The last attempt failed and retained evidence requires retry or reconciliation.
    RetryRequired,
    /// The retained owner or durable receipt requires explicit reconciliation before cleanup can advance.
    ReconciliationRequired,
}

/// Durable, nonsecret cause for one cleanup dimension's first failed attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupFailure {
    kind: WindowsErrorKind,
    operation: WindowsOperation,
    recovery: WindowsRecovery,
    detail: String,
    source: Option<String>,
}

impl CleanupFailure {
    pub(crate) fn from_windows_error(error: &WindowsError) -> Self {
        Self {
            kind: error.kind(),
            operation: error.operation(),
            recovery: error.recovery(),
            detail: error.detail().to_owned(),
            source: error.cause().map(|source| source.to_string()),
        }
    }

    pub(crate) fn from_process_error(error: &peritus_process::ProcessError) -> Self {
        if let Some(windows) = error
            .cause()
            .and_then(|source| source.downcast_ref::<WindowsError>())
        {
            return Self::from_windows_error(windows);
        }
        let recovery = match error.recovery() {
            peritus_process::RecoveryClass::Quarantine => WindowsRecovery::Quarantine,
            peritus_process::RecoveryClass::ReopenAndReconcile => {
                WindowsRecovery::ReconcileCleanup
            }
            _ => WindowsRecovery::RetryCleanup,
        };
        Self {
            kind: WindowsErrorKind::RecoveryIndeterminate,
            operation: WindowsOperation::Release,
            recovery,
            detail: error.detail().to_owned(),
            source: Some(format!(
                "process {:?} during {:?}; recovery {:?}",
                error.code(),
                error.operation(),
                error.recovery()
            )),
        }
    }

    pub(crate) fn from_parts(
        kind: WindowsErrorKind,
        operation: WindowsOperation,
        recovery: WindowsRecovery,
        detail: String,
        source: Option<String>,
    ) -> Self {
        Self { kind, operation, recovery, detail, source }
    }

    pub(crate) fn text_is_bounded(detail: &str, source: Option<&str>) -> bool {
        detail.len() <= MAX_CLEANUP_CAUSE_BYTES
            && source.is_none_or(|value| value.len() <= MAX_CLEANUP_CAUSE_BYTES)
    }

    /// Returns the stable Windows failure category.
    #[must_use]
    pub const fn kind(&self) -> WindowsErrorKind {
        self.kind
    }

    /// Returns the Windows operation active when cleanup failed.
    #[must_use]
    pub const fn operation(&self) -> WindowsOperation {
        self.operation
    }

    /// Returns retry or reconciliation guidance.
    #[must_use]
    pub const fn recovery(&self) -> WindowsRecovery {
        self.recovery
    }

    /// Returns bounded nonsecret failure detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Returns a bounded nonsecret rendering of the typed underlying source.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Reports whether another ordinary retry is insufficient.
    #[must_use]
    pub const fn requires_reconciliation(&self) -> bool {
        matches!(self.recovery, WindowsRecovery::ReconcileCleanup | WindowsRecovery::Quarantine)
    }
}

/// Durable first-failure cause for every independently progressing cleanup dimension.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CleanupFailures {
    job: Option<CleanupFailure>,
    helper: Option<CleanupFailure>,
    acl: Option<CleanupFailure>,
    secret_files: Option<CleanupFailure>,
    secret_delivery: Option<CleanupFailure>,
    handles: Option<CleanupFailure>,
    proxy: Option<CleanupFailure>,
    filter: Option<CleanupFailure>,
}

impl CleanupFailures {
    #[allow(clippy::too_many_arguments, reason = "every cleanup dimension is explicit")]
    pub(crate) fn new(
        job: Option<CleanupFailure>,
        helper: Option<CleanupFailure>,
        acl: Option<CleanupFailure>,
        secret_files: Option<CleanupFailure>,
        secret_delivery: Option<CleanupFailure>,
        handles: Option<CleanupFailure>,
        proxy: Option<CleanupFailure>,
        filter: Option<CleanupFailure>,
    ) -> Self {
        Self { job, helper, acl, secret_files, secret_delivery, handles, proxy, filter }
    }

    pub(crate) fn record(
        &mut self,
        dimension: crate::recovery::RecoveryCleanupDimension,
        failure: CleanupFailure,
    ) -> bool {
        let slot = match dimension {
            crate::recovery::RecoveryCleanupDimension::Job => &mut self.job,
            crate::recovery::RecoveryCleanupDimension::Helper => &mut self.helper,
            crate::recovery::RecoveryCleanupDimension::Acl => &mut self.acl,
            crate::recovery::RecoveryCleanupDimension::SecretFiles => &mut self.secret_files,
            crate::recovery::RecoveryCleanupDimension::SecretDelivery => &mut self.secret_delivery,
            crate::recovery::RecoveryCleanupDimension::Handles => &mut self.handles,
            crate::recovery::RecoveryCleanupDimension::Proxy => &mut self.proxy,
            crate::recovery::RecoveryCleanupDimension::NetworkFilter => &mut self.filter,
        };
        match slot {
            None => {
                *slot = Some(failure);
                true
            }
            Some(current)
                if !current.requires_reconciliation() && failure.requires_reconciliation() =>
            {
                *slot = Some(failure);
                true
            }
            Some(_) => false,
        }
    }

    /// Returns the Job Object cleanup cause.
    #[must_use]
    pub const fn job(&self) -> Option<&CleanupFailure> { self.job.as_ref() }
    /// Returns the helper cleanup cause.
    #[must_use]
    pub const fn helper(&self) -> Option<&CleanupFailure> { self.helper.as_ref() }
    /// Returns the ACL cleanup cause.
    #[must_use]
    pub const fn acl(&self) -> Option<&CleanupFailure> { self.acl.as_ref() }
    /// Returns the private secret-file cleanup cause.
    #[must_use]
    pub const fn secret_files(&self) -> Option<&CleanupFailure> { self.secret_files.as_ref() }
    /// Returns the secret-delivery cleanup cause.
    #[must_use]
    pub const fn secret_delivery(&self) -> Option<&CleanupFailure> {
        self.secret_delivery.as_ref()
    }
    /// Returns the protected-handle cleanup cause.
    #[must_use]
    pub const fn handles(&self) -> Option<&CleanupFailure> { self.handles.as_ref() }
    /// Returns the managed-proxy cleanup cause.
    #[must_use]
    pub const fn proxy(&self) -> Option<&CleanupFailure> { self.proxy.as_ref() }
    /// Returns the WFP policy cleanup cause.
    #[must_use]
    pub const fn filter(&self) -> Option<&CleanupFailure> { self.filter.as_ref() }
}

/// Partial cleanup evidence retained even when release returns an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseProgress {
    acl: CleanupState,
    job: CleanupState,
    helper: CleanupState,
    secret_files: CleanupState,
    secret_delivery: CleanupState,
    handles: CleanupState,
    proxy: CleanupState,
    filter: CleanupState,
    failures: CleanupFailures,
}

impl ReleaseProgress {
    #[allow(
        clippy::too_many_arguments,
        reason = "each independently progressing cleanup dimension remains explicit"
    )]
    pub(crate) fn new(
        acl: CleanupState,
        job: CleanupState,
        helper: CleanupState,
        secret_files: CleanupState,
        secret_delivery: CleanupState,
        handles: CleanupState,
        proxy: CleanupState,
        filter: CleanupState,
        failures: CleanupFailures,
    ) -> Self {
        Self { acl, job, helper, secret_files, secret_delivery, handles, proxy, filter, failures }
    }

    /// Returns exact ACL reversal progress.
    #[must_use]
    pub const fn acl(&self) -> CleanupState {
        self.acl
    }

    /// Returns exact Job Object handle cleanup progress.
    #[must_use]
    pub const fn job(&self) -> CleanupState {
        self.job
    }

    /// Returns helper reap progress.
    #[must_use]
    pub const fn helper(&self) -> CleanupState {
        self.helper
    }

    /// Returns exact private secret-file cleanup progress.
    #[must_use]
    pub const fn secret_files(&self) -> CleanupState {
        self.secret_files
    }

    /// Returns exact secret-delivery cleanup progress.
    #[must_use]
    pub const fn secret_delivery(&self) -> CleanupState {
        self.secret_delivery
    }

    /// Returns protected inherited-handle cleanup progress.
    #[must_use]
    pub const fn handles(&self) -> CleanupState {
        self.handles
    }

    /// Returns managed-proxy teardown progress.
    #[must_use]
    pub const fn proxy(&self) -> CleanupState {
        self.proxy
    }

    /// Returns dynamic WFP policy teardown progress.
    #[must_use]
    pub const fn filter(&self) -> CleanupState {
        self.filter
    }

    /// Returns exact secret-delivery teardown progress.
    #[must_use]
    pub const fn secrets(&self) -> CleanupState {
        self.secret_delivery
    }

    /// Returns the durable first cause retained for each failed cleanup dimension.
    #[must_use]
    pub const fn failures(&self) -> &CleanupFailures {
        &self.failures
    }
}

/// Final backend-local release evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent teardown evidence remains explicit and inspectable"
)]
pub struct ReleaseReport {
    pub(crate) process_quiescent: bool,
    pub(crate) job_quiescent: bool,
    pub(crate) job_closed: bool,
    pub(crate) helper_reaped: bool,
    pub(crate) acl_restored: bool,
    pub(crate) secret_files_removed: bool,
    pub(crate) secret_delivery_released: bool,
    pub(crate) handles_closed: bool,
    pub(crate) proxy_joined: bool,
    pub(crate) network_filter_removed: bool,
}

impl ReleaseReport {
    /// Reports complete backend-owned cleanup.
    #[must_use]
    pub const fn complete(self) -> bool {
        crate::verified::teardown_complete(
            self.process_quiescent,
            self.job_quiescent,
            self.job_closed,
            self.helper_reaped,
            self.acl_restored,
            self.secret_files_removed,
            self.secret_delivery_released,
            self.handles_closed,
            self.proxy_joined,
            self.network_filter_removed,
        )
    }

    /// Reports observed helper and adopted-target process absence.
    #[must_use]
    pub const fn process_quiescent(self) -> bool {
        self.process_quiescent
    }

    /// Reports that the retained Job Object was observed empty before closure.
    #[must_use]
    pub const fn job_quiescent(self) -> bool {
        self.job_quiescent
    }

    /// Reports exact Job Object handle closure.
    #[must_use]
    pub const fn job_closed(self) -> bool {
        self.job_closed
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
    /// Reports secret lease, material, and staging release.
    #[must_use]
    pub const fn secret_delivery_released(self) -> bool {
        self.secret_delivery_released
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
