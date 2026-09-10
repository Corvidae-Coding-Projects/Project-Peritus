//! Bounded non-mutating product diagnostics; observations never grant authority.

use crate::{AppErrorCode, AppProtocolError};
use peritus_types::{ProviderProfileId, WorkspaceId};

#[cfg(test)]
mod tests;

/// Maximum independently classified findings in one diagnostic report.
pub const MAX_DOCTOR_FINDINGS: usize = 32;
/// Maximum UTF-8 bytes in a finding's summary or suggested action.
pub const MAX_DOCTOR_TEXT_BYTES: usize = 1024;

/// Exact scope for local-only diagnostics. No network or repair option is implied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DoctorQuery {
    workspace: WorkspaceId,
    provider: Option<ProviderProfileId>,
}

impl DoctorQuery {
    /// Selects a workspace and optionally its current provider route.
    #[must_use]
    pub const fn new(workspace: WorkspaceId, provider: Option<ProviderProfileId>) -> Self {
        Self { workspace, provider }
    }
    /// Returns the selected workspace identity.
    #[must_use]
    pub const fn workspace(self) -> WorkspaceId {
        self.workspace
    }
    /// Returns the selected route, without containing credential material.
    #[must_use]
    pub const fn provider(self) -> Option<ProviderProfileId> {
        self.provider
    }
}

/// Closed evidence classification for one check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoctorStatus {
    /// The explicitly described local check passed; no stronger check is implied.
    Healthy,
    /// A nonfatal condition needs attention.
    Warning,
    /// The checked prerequisite is not satisfied.
    Blocked,
    /// The check cannot be performed by this backend or without further permission.
    Unsupported,
}

impl DoctorStatus {
    /// Human-readable classification, independent of the observation text.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Warning => "warning",
            Self::Blocked => "blocked",
            Self::Unsupported => "unsupported",
        }
    }
}

/// One bounded, inert observation and separately suggested next action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorFinding {
    check: String,
    status: DoctorStatus,
    observation: String,
    action: String,
}

impl DoctorFinding {
    /// Validates the check label and bounded terminal-safe text.
    ///
    /// # Errors
    /// Rejects empty labels/observations, control characters, and excessive text.
    pub fn new(
        check: String,
        status: DoctorStatus,
        observation: String,
        action: String,
    ) -> Result<Self, AppProtocolError> {
        if check.is_empty()
            || check.len() > 64
            || observation.is_empty()
            || [&check, &observation, &action].into_iter().any(|text| {
                text.len() > MAX_DOCTOR_TEXT_BYTES || text.chars().any(char::is_control)
            })
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { check, status, observation, action })
    }
    /// Returns the stable check label.
    #[must_use]
    pub fn check(&self) -> &str {
        &self.check
    }
    /// Returns the observed classification.
    #[must_use]
    pub const fn status(&self) -> DoctorStatus {
        self.status
    }
    /// Returns only the public bounded observation, never raw provider output.
    #[must_use]
    pub fn observation(&self) -> &str {
        &self.observation
    }
    /// Returns a suggested action, not an automatically executed repair.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }
}

/// A scoped point-in-time report, not a live readiness guarantee.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorReport {
    query: DoctorQuery,
    findings: Vec<DoctorFinding>,
}

impl DoctorReport {
    /// Validates report size and unique check identities.
    ///
    /// # Errors
    /// Rejects empty/excessive findings or duplicate labels.
    pub fn new(query: DoctorQuery, findings: Vec<DoctorFinding>) -> Result<Self, AppProtocolError> {
        if findings.is_empty()
            || findings.len() > MAX_DOCTOR_FINDINGS
            || findings.iter().enumerate().any(|(index, finding)| {
                findings[..index].iter().any(|other| other.check == finding.check)
            })
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { query, findings })
    }
    /// Returns the exact query scope.
    #[must_use]
    pub const fn query(&self) -> DoctorQuery {
        self.query
    }
    /// Borrows the bounded set of classified observations.
    #[must_use]
    pub fn findings(&self) -> &[DoctorFinding] {
        &self.findings
    }
}
