//! Versioned durable product-state document.

use serde::Deserialize;
use serde::Serialize;

use crate::{
    BootstrapPhase, InstallIdentity, ProductStateError, ProviderSelection, WorkspaceProfile,
    WorkspaceSelection,
};

/// Product-state schema understood by this executable.
pub const PRODUCT_STATE_SCHEMA_VERSION: u16 = 1;

/// Canonical durable state needed to resume local bootstrap.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductState {
    schema_version: u16,
    generation: u64,
    identity: InstallIdentity,
    bootstrap_phase: BootstrapPhase,
    #[serde(default)]
    providers: ProviderSelection,
    #[serde(default)]
    provider_setup_complete: bool,
    #[serde(default)]
    workspaces: WorkspaceSelection,
    #[serde(default)]
    workspace_setup_complete: bool,
}

impl ProductState {
    /// Begins a new installation after identities have been durably selected.
    #[must_use]
    pub fn new(identity: InstallIdentity) -> Self {
        Self {
            schema_version: PRODUCT_STATE_SCHEMA_VERSION,
            generation: 1,
            identity,
            bootstrap_phase: BootstrapPhase::IdentityReady,
            providers: <ProviderSelection as Default>::default(),
            provider_setup_complete: false,
            workspaces: <WorkspaceSelection as Default>::default(),
            workspace_setup_complete: false,
        }
    }

    /// Parses and validates an exact JSON payload.
    ///
    /// # Errors
    ///
    /// Returns a typed schema, identity, or JSON failure.
    pub fn parse_json(bytes: &[u8]) -> Result<Self, ProductStateError> {
        let state: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProductStateError::InvalidPayload(error.to_string()))?;
        state.validate()?;
        Ok(state)
    }

    /// Serializes a deterministic compact JSON payload terminated by one newline.
    ///
    /// # Errors
    ///
    /// Returns a typed serialization failure if the in-memory value cannot be encoded.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ProductStateError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self)
            .map_err(|error| ProductStateError::InvalidPayload(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Returns the product-state schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Returns the positive generation incremented by every durable phase transition.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Borrows stable non-secret installation identities.
    #[must_use]
    pub const fn identity(&self) -> &InstallIdentity {
        &self.identity
    }

    /// Returns the last durably completed bootstrap phase.
    #[must_use]
    pub const fn bootstrap_phase(&self) -> BootstrapPhase {
        self.bootstrap_phase
    }

    /// Borrows durable non-secret provider choices.
    #[must_use]
    pub const fn providers(&self) -> &ProviderSelection {
        &self.providers
    }

    /// Returns whether the user completed provider setup, including explicit offline mode.
    #[must_use]
    pub const fn provider_setup_complete(&self) -> bool {
        self.provider_setup_complete
    }

    /// Borrows durable recent and active workspace choices.
    #[must_use]
    pub const fn workspaces(&self) -> &WorkspaceSelection {
        &self.workspaces
    }

    /// Returns whether workspace selection completed at least once.
    #[must_use]
    pub const fn workspace_setup_complete(&self) -> bool {
        self.workspace_setup_complete
    }

    /// Replaces durable provider choices and advances the immutable generation when changed.
    pub fn configure_providers(&mut self, providers: ProviderSelection) -> bool {
        if self.providers == providers && self.provider_setup_complete {
            return false;
        }
        self.providers = providers;
        self.provider_setup_complete = true;
        self.generation = self.generation.saturating_add(1);
        true
    }

    /// Inserts or updates the active workspace and advances the immutable generation.
    ///
    /// # Errors
    ///
    /// Rejects invalid or inconsistent workspace facts.
    pub fn configure_workspace(
        &mut self,
        profile: WorkspaceProfile,
    ) -> Result<bool, ProductStateError> {
        let mut workspaces = self.workspaces.clone();
        workspaces.activate(profile)?;
        if self.workspaces == workspaces && self.workspace_setup_complete {
            return Ok(false);
        }
        self.workspaces = workspaces;
        self.workspace_setup_complete = true;
        self.generation = self.generation.saturating_add(1);
        Ok(true)
    }

    /// Selects a remembered workspace and advances the immutable generation when changed.
    ///
    /// # Errors
    ///
    /// Rejects an unknown workspace identity.
    pub fn select_workspace(&mut self, workspace_id: &str) -> Result<bool, ProductStateError> {
        if self.workspaces.active().map(WorkspaceProfile::workspace_id) == Some(workspace_id) {
            return Ok(false);
        }
        self.workspaces.select(workspace_id)?;
        self.workspace_setup_complete = true;
        self.generation = self.generation.saturating_add(1);
        Ok(true)
    }

    /// Forgets one workspace and advances the immutable generation when found.
    pub fn remove_workspace(&mut self, workspace_id: &str) -> bool {
        if !self.workspaces.remove(workspace_id) {
            return false;
        }
        self.generation = self.generation.saturating_add(1);
        true
    }

    /// Advances to the same phase or its exact successor.
    ///
    /// Repeating the current phase is idempotent and does not change the generation.
    ///
    /// # Errors
    ///
    /// Returns [`ProductStateError::InvalidTransition`] for a skip or reversal.
    pub fn advance(&mut self, next: BootstrapPhase) -> Result<bool, ProductStateError> {
        if !crate::verified::bootstrap_transition_exec(self.bootstrap_phase, next) {
            return Err(ProductStateError::InvalidTransition {
                from: self.bootstrap_phase,
                to: next,
            });
        }
        if self.bootstrap_phase == next {
            return Ok(false);
        }
        self.bootstrap_phase = next;
        self.generation = self.generation.saturating_add(1);
        Ok(true)
    }

    fn validate(&self) -> Result<(), ProductStateError> {
        if self.schema_version != PRODUCT_STATE_SCHEMA_VERSION {
            return Err(ProductStateError::UnsupportedSchema(self.schema_version));
        }
        if self.generation == 0 {
            return Err(ProductStateError::InvalidPayload(
                "product-state generation must be positive".to_owned(),
            ));
        }
        self.identity.validate()?;
        self.providers.validate()?;
        self.workspaces.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> InstallIdentity {
        InstallIdentity::new([1; 16], [2; 16]).expect("valid identity")
    }

    #[test]
    fn canonical_round_trip_preserves_resume_phase() {
        let mut state = ProductState::new(identity());
        assert!(state.advance(BootstrapPhase::RegistryReady).expect("advance"));
        let bytes = state.canonical_json().expect("encode");
        assert_eq!(ProductState::parse_json(&bytes).expect("decode"), state);
        assert_eq!(state.generation(), 2);
    }

    #[test]
    fn skip_and_reversal_are_rejected() {
        let mut state = ProductState::new(identity());
        assert!(state.advance(BootstrapPhase::ConfigurationReady).is_err());
        state.advance(BootstrapPhase::RegistryReady).expect("exact successor");
        assert!(state.advance(BootstrapPhase::IdentityReady).is_err());
    }

    #[test]
    fn repeating_completed_effect_is_idempotent() {
        let mut state = ProductState::new(identity());
        assert!(!state.advance(BootstrapPhase::IdentityReady).expect("same phase"));
        assert_eq!(state.generation(), 1);
    }

    #[test]
    fn zero_identity_is_rejected() {
        assert!(InstallIdentity::new([0; 16], [2; 16]).is_err());
        assert!(InstallIdentity::new([1; 16], [0; 16]).is_err());
    }

    #[test]
    fn provider_selection_is_durable_and_idempotent() {
        let mut state = ProductState::new(identity());
        let selection = ProviderSelection::new(
            vec![crate::ProviderKind::ClaudeAccount, crate::ProviderKind::CodexAccount],
            Some(crate::ProviderKind::CodexAccount),
        )
        .expect("selection");
        assert!(state.configure_providers(selection.clone()));
        assert!(!state.configure_providers(selection));
        assert_eq!(state.generation(), 2);
        assert_eq!(state.providers().enabled().len(), 2);
        assert!(state.provider_setup_complete());
    }

    #[test]
    fn workspace_selection_round_trips_and_advances_once() {
        let mut state = ProductState::new(identity());
        let profile = WorkspaceProfile::restricted(
            "/repo".to_owned(),
            "01".repeat(32),
            "02".repeat(16),
            "03".repeat(16),
            "04".repeat(16),
            "05".repeat(16),
        )
        .expect("workspace");
        assert!(state.configure_workspace(profile.clone()).expect("configure"));
        assert!(!state.configure_workspace(profile).expect("idempotent"));
        let bytes = state.canonical_json().expect("json");
        let decoded = ProductState::parse_json(&bytes).expect("decode");
        assert_eq!(decoded.workspaces().active().expect("active").repository_root(), "/repo");
        assert!(decoded.workspace_setup_complete());
    }
}
