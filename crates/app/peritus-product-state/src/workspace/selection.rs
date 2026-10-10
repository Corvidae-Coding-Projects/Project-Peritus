//! Most-recent-first workspace inventory and active selection.

use serde::Deserialize;
use serde::Serialize;

use super::{WorkspaceProfile, WorkspaceTrust};
use crate::ProductStateError;

const MAX_RECENT_WORKSPACES: usize = 32;

/// Most-recent-first workspace inventory and active selection.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSelection {
    #[serde(default)]
    recent: Vec<WorkspaceProfile>,
    #[serde(default)]
    retained_registrations: Vec<WorkspaceProfile>,
    active_workspace_id: Option<String>,
}

impl WorkspaceSelection {
    /// Borrows recent workspaces in most-recent-first order.
    #[must_use]
    pub fn recent(&self) -> &[WorkspaceProfile] {
        &self.recent
    }

    /// Returns every trusted registration retained for exact daemon-catalog recovery.
    #[must_use]
    pub fn registered(&self) -> Vec<&WorkspaceProfile> {
        self.recent
            .iter()
            .chain(self.retained_registrations.iter())
            .filter(|profile| {
                !profile.is_direct_folder() && profile.trust_level() == WorkspaceTrust::Trusted
            })
            .collect()
    }

    /// Returns the active workspace profile, when one is selected.
    #[must_use]
    pub fn active(&self) -> Option<&WorkspaceProfile> {
        let active = self.active_workspace_id.as_deref()?;
        self.recent.iter().find(|profile| profile.workspace_id() == active)
    }

    /// Finds a remembered workspace by exact repository root and identity.
    #[must_use]
    pub fn find_repository(&self, root: &str, identity: &str) -> Option<&WorkspaceProfile> {
        self.recent.iter().find(|profile| {
            profile.repository_root() == root && profile.repository_identity() == identity
        })
    }

    /// Inserts or replaces a workspace and makes it most recent and active.
    ///
    /// # Errors
    ///
    /// Rejects an invalid workspace profile.
    pub fn activate(&mut self, profile: WorkspaceProfile) -> Result<(), ProductStateError> {
        profile.validate()?;
        self.retained_registrations.retain(|existing| {
            existing.workspace_id() != profile.workspace_id()
                && existing.repository_root() != profile.repository_root()
        });
        let previous = std::mem::take(&mut self.recent);
        for existing in previous {
            if existing.workspace_id() == profile.workspace_id() {
                // This is an in-place profile refresh for the same workspace lineage. Its new
                // registration supersedes the old one; retaining both would make the catalog
                // internally contradictory.
            } else if existing.repository_root() == profile.repository_root() {
                self.retain_registration(existing);
            } else {
                self.recent.push(existing);
            }
        }
        self.active_workspace_id = Some(profile.workspace_id().to_owned());
        self.recent.insert(0, profile);
        while self.recent.len() > MAX_RECENT_WORKSPACES {
            if let Some(overflow) = self.recent.pop() {
                self.retain_registration(overflow);
            }
        }
        self.validate()
    }

    /// Selects one remembered workspace and moves it to the front of the recent list.
    ///
    /// # Errors
    ///
    /// Rejects an unknown workspace identity.
    pub fn select(&mut self, workspace_id: &str) -> Result<(), ProductStateError> {
        let Some(index) = self.recent.iter().position(|item| item.workspace_id() == workspace_id)
        else {
            return Err(invalid("selected workspace is not remembered"));
        };
        let profile = self.recent.remove(index);
        self.active_workspace_id = Some(profile.workspace_id().to_owned());
        self.recent.insert(0, profile);
        Ok(())
    }

    /// Removes one remembered workspace and selects the next recent entry if necessary.
    pub fn remove(&mut self, workspace_id: &str) -> bool {
        let Some(index) = self.recent.iter().position(|item| item.workspace_id() == workspace_id)
        else {
            return false;
        };
        let removed = self.recent.remove(index);
        self.retain_registration(removed);
        if self.active_workspace_id.as_deref() == Some(workspace_id) {
            self.active_workspace_id =
                self.recent.first().map(|item| item.workspace_id().to_owned());
        }
        true
    }

    fn retain_registration(&mut self, profile: WorkspaceProfile) {
        if profile.is_direct_folder() || profile.trust_level() != WorkspaceTrust::Trusted {
            return;
        }
        self.retained_registrations.retain(|existing| {
            existing.workspace_id() != profile.workspace_id()
                && existing.repository_root() != profile.repository_root()
        });
        self.retained_registrations.push(profile);
    }

    pub(crate) fn validate(&self) -> Result<(), ProductStateError> {
        let registered = self.registered();
        if self.recent.len() > MAX_RECENT_WORKSPACES
            || self.recent.iter().any(|profile| profile.validate().is_err())
            || self.retained_registrations.iter().any(|profile| {
                profile.validate().is_err() || profile.trust_level() != WorkspaceTrust::Trusted
            })
            || self.recent.iter().enumerate().any(|(index, profile)| {
                self.recent[..index].iter().any(|previous| {
                    previous.workspace_id() == profile.workspace_id()
                        || previous.repository_root() == profile.repository_root()
                })
            })
            || self.active_workspace_id.as_deref().is_some_and(|active| {
                !self.recent.iter().any(|profile| profile.workspace_id() == active)
            })
            || registered.iter().enumerate().any(|(index, profile)| {
                registered.iter().take(index).any(|previous| {
                    previous.workspace_id() == profile.workspace_id()
                        || previous.repository_root() == profile.repository_root()
                })
            })
        {
            return Err(invalid("workspace selection is noncanonical or inconsistent"));
        }
        Ok(())
    }
}

fn invalid(detail: &'static str) -> ProductStateError {
    ProductStateError::InvalidPayload(detail.to_owned())
}
