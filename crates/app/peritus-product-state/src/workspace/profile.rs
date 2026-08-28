//! One remembered repository and its optional managed-worktree registration.

use serde::Deserialize;
use serde::Serialize;

use crate::ProductStateError;

const MAX_PATH_BYTES: usize = 32_768;

/// User-granted capability level for one remembered repository identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceTrust {
    /// Inert repository browsing only; no repository-controlled execution or mutation.
    Restricted,
    /// Agent effects may target the exact application-managed C1 worktree.
    Trusted,
}

/// Durable non-secret facts for one recently selected repository.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceProfile {
    repository_root: String,
    repository_identity: String,
    project_id: String,
    workspace_id: String,
    resource_id: String,
    environment_id: String,
    trust: WorkspaceTrust,
    registration_file: Option<String>,
    registration_digest: Option<String>,
    managed_root: Option<String>,
    transaction_root: Option<String>,
}

impl WorkspaceProfile {
    /// Creates a restricted recent-workspace record from canonical observed identity facts.
    ///
    /// # Errors
    ///
    /// Rejects malformed paths, digests, or nominal identifiers.
    pub fn restricted(
        repository_root: String,
        repository_identity: String,
        project_id: String,
        workspace_id: String,
        resource_id: String,
        environment_id: String,
    ) -> Result<Self, ProductStateError> {
        let profile = Self {
            repository_root,
            repository_identity,
            project_id,
            workspace_id,
            resource_id,
            environment_id,
            trust: WorkspaceTrust::Restricted,
            registration_file: None,
            registration_digest: None,
            managed_root: None,
            transaction_root: None,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Adds exact C1 registration references after the managed worktree is durably published.
    ///
    /// # Errors
    ///
    /// Rejects incomplete or malformed registration facts.
    pub fn trust(
        mut self,
        registration_file: String,
        registration_digest: String,
        managed_root: String,
        transaction_root: String,
    ) -> Result<Self, ProductStateError> {
        self.trust = WorkspaceTrust::Trusted;
        self.registration_file = Some(registration_file);
        self.registration_digest = Some(registration_digest);
        self.managed_root = Some(managed_root);
        self.transaction_root = Some(transaction_root);
        self.validate()?;
        Ok(self)
    }

    /// Returns this workspace to inert browse-only mode and forgets executable registration facts.
    #[must_use]
    pub fn restrict(mut self) -> Self {
        self.trust = WorkspaceTrust::Restricted;
        self.registration_file = None;
        self.registration_digest = None;
        self.managed_root = None;
        self.transaction_root = None;
        self
    }

    /// Borrows the canonical source repository root.
    #[must_use]
    pub fn repository_root(&self) -> &str {
        &self.repository_root
    }

    /// Borrows the exact observed repository identity digest.
    #[must_use]
    pub fn repository_identity(&self) -> &str {
        &self.repository_identity
    }

    /// Borrows the stable project identity.
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Borrows the stable workspace lineage identity.
    #[must_use]
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Borrows the stable workspace resource identity.
    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    /// Borrows the stable local environment identity.
    #[must_use]
    pub fn environment_id(&self) -> &str {
        &self.environment_id
    }

    /// Returns the remembered trust level.
    #[must_use]
    pub const fn trust_level(&self) -> WorkspaceTrust {
        self.trust
    }

    /// Borrows the canonical C1 registration file when trusted.
    #[must_use]
    pub fn registration_file(&self) -> Option<&str> {
        self.registration_file.as_deref()
    }

    /// Borrows the expected C1 registration digest when trusted.
    #[must_use]
    pub fn registration_digest(&self) -> Option<&str> {
        self.registration_digest.as_deref()
    }

    /// Borrows the managed writable worktree root when trusted.
    #[must_use]
    pub fn managed_root(&self) -> Option<&str> {
        self.managed_root.as_deref()
    }

    /// Borrows the isolated transaction root when trusted.
    #[must_use]
    pub fn transaction_root(&self) -> Option<&str> {
        self.transaction_root.as_deref()
    }

    pub(crate) fn validate(&self) -> Result<(), ProductStateError> {
        let registration_count = u8::from(self.registration_file.is_some())
            + u8::from(self.registration_digest.is_some())
            + u8::from(self.managed_root.is_some())
            + u8::from(self.transaction_root.is_some());
        let registration_shape =
            crate::verified::workspace_registration_shape_exec(self.trust, registration_count);
        let valid_registration = self.registration_file.as_deref().is_none_or(valid_path)
            && self.registration_digest.as_deref().is_none_or(valid_digest)
            && self.managed_root.as_deref().is_none_or(valid_path)
            && self.transaction_root.as_deref().is_none_or(valid_path);
        if !valid_path(&self.repository_root)
            || !valid_digest(&self.repository_identity)
            || !valid_identifier(&self.project_id)
            || !valid_identifier(&self.workspace_id)
            || !valid_identifier(&self.resource_id)
            || !valid_identifier(&self.environment_id)
            || !registration_shape
            || !valid_registration
        {
            return Err(invalid("workspace profile is malformed or exceeds its bounds"));
        }
        Ok(())
    }
}

fn valid_identifier(value: &str) -> bool {
    value != "00000000000000000000000000000000" && valid_hex(value, 32)
}

fn valid_digest(value: &str) -> bool {
    valid_hex(value, 64)
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PATH_BYTES
        && value.bytes().all(|byte| !byte.is_ascii_control())
}

fn invalid(detail: &'static str) -> ProductStateError {
    ProductStateError::InvalidPayload(detail.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_requires_all_registration_facts() {
        let restricted = WorkspaceProfile::restricted(
            "/repo".to_owned(),
            "01".repeat(32),
            "02".repeat(16),
            "03".repeat(16),
            "04".repeat(16),
            "05".repeat(16),
        )
        .expect("profile");
        assert_eq!(restricted.trust_level(), WorkspaceTrust::Restricted);
        let trusted = restricted
            .trust(
                "/state/registration.bin".to_owned(),
                "06".repeat(32),
                "/state/worktree".to_owned(),
                "/state/transactions".to_owned(),
            )
            .expect("trusted");
        assert_eq!(trusted.trust_level(), WorkspaceTrust::Trusted);
        assert!(trusted.restrict().registration_file().is_none());
    }
}
