//! Complete durable workspace registry with bounded presentation recency.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};

use super::{WorkspaceProfile, WorkspaceTrust};
use crate::ProductStateError;

const MAX_RECENT_WORKSPACES: usize = 32;

/// Complete durable workspace registry, most-recent presentation, and active selection.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct WorkspaceSelection {
    #[serde(rename = "registry")]
    retained_registrations: Vec<WorkspaceProfile>,
    recent: Vec<String>,
    #[serde(skip)]
    workspace_index: BTreeMap<String, usize>,
    repository_index: BTreeMap<String, BTreeMap<String, RepositoryLineages>>,
    active_workspace_id: Option<String>,
    #[serde(skip)]
    legacy_storage: bool,
}

impl WorkspaceSelection {
    /// Borrows recent workspaces in most-recent-first order.
    #[must_use]
    pub fn recent(&self) -> Vec<&WorkspaceProfile> {
        self.recent.iter().filter_map(|workspace| self.find(workspace)).collect()
    }

    /// Borrows one recent presentation entry by zero-based position.
    #[must_use]
    pub fn recent_at(&self, index: usize) -> Option<&WorkspaceProfile> {
        self.recent.get(index).and_then(|workspace| self.find(workspace))
    }

    /// Returns every durable workspace lineage, including entries outside presentation recency.
    #[must_use]
    pub fn profiles(&self) -> Vec<&WorkspaceProfile> {
        let recent = self.recent.iter().map(String::as_str).collect::<BTreeSet<_>>();
        self.recent()
            .into_iter()
            .chain(
                self.retained_registrations
                    .iter()
                    .filter(|profile| !recent.contains(profile.workspace_id())),
            )
            .collect()
    }

    /// Returns every trusted registration retained for exact daemon-catalog recovery.
    #[must_use]
    pub fn registered(&self) -> Vec<&WorkspaceProfile> {
        self.retained_registrations
            .iter()
            .filter(|profile| {
                !profile.is_direct_folder() && profile.trust_level() == WorkspaceTrust::Trusted
            })
            .collect()
    }

    /// Returns the active workspace profile, when one is selected.
    #[must_use]
    pub fn active(&self) -> Option<&WorkspaceProfile> {
        let active = self.active_workspace_id.as_deref()?;
        self.find(active)
    }

    /// Finds a durable workspace by its exact lineage identity.
    #[must_use]
    pub fn find(&self, workspace_id: &str) -> Option<&WorkspaceProfile> {
        self.workspace_index
            .get(workspace_id)
            .and_then(|index| self.retained_registrations.get(*index))
    }

    /// Finds a remembered workspace by exact repository root and identity.
    #[must_use]
    pub fn find_repository(&self, root: &str, identity: &str) -> Option<&WorkspaceProfile> {
        self.repository_index
            .get(root)?
            .get(identity)
            .and_then(|lineages| self.find(&lineages.selected))
    }

    /// Inserts or replaces a workspace and makes it most recent and active.
    ///
    /// # Errors
    ///
    /// Rejects an invalid workspace profile.
    pub fn activate(&mut self, profile: WorkspaceProfile) -> Result<(), ProductStateError> {
        profile.validate()?;
        let workspace_id = profile.workspace_id().to_owned();
        self.insert_registry(profile)?;
        self.promote(&workspace_id)?;
        self.active_workspace_id = Some(workspace_id);
        self.validate()
    }

    /// Selects one remembered workspace and moves it to the front of the recent list.
    ///
    /// # Errors
    ///
    /// Rejects an unknown workspace identity.
    pub fn select(&mut self, workspace_id: &str) -> Result<(), ProductStateError> {
        if self.find(workspace_id).is_none() {
            return Err(invalid("selected workspace is not remembered"));
        }
        self.promote(workspace_id)?;
        self.active_workspace_id = Some(workspace_id.to_owned());
        self.validate()
    }

    /// Removes one remembered workspace and selects the next recent entry if necessary.
    pub fn remove(&mut self, workspace_id: &str) -> bool {
        let Some(index) = self.recent.iter().position(|item| item == workspace_id) else {
            return false;
        };
        self.recent.remove(index);
        if self.active_workspace_id.as_deref() == Some(workspace_id) {
            self.active_workspace_id = self.recent.first().cloned();
            if let Some(active) = self.active_workspace_id.clone() {
                self.point_repository_to(&active);
            }
        }
        true
    }

    fn retain_registration(&mut self, profile: WorkspaceProfile) {
        if profile.is_direct_folder() || profile.trust_level() != WorkspaceTrust::Trusted {
            return;
        }
        let _ = self.insert_registry(profile);
    }

    fn insert_registry(&mut self, profile: WorkspaceProfile) -> Result<(), ProductStateError> {
        profile.validate()?;
        let workspace_id = profile.workspace_id().to_owned();
        if let Some(index) = self.workspace_index.get(&workspace_id).copied() {
            let previous = std::mem::replace(&mut self.retained_registrations[index], profile);
            self.remove_repository_pointer(&previous);
        } else {
            let index = self.retained_registrations.len();
            self.retained_registrations.push(profile);
            self.workspace_index.insert(workspace_id.clone(), index);
        }
        self.point_repository_to(&workspace_id);
        Ok(())
    }

    fn promote(&mut self, workspace_id: &str) -> Result<(), ProductStateError> {
        let root = self
            .find(workspace_id)
            .ok_or_else(|| invalid("selected workspace is not registered"))?
            .repository_root()
            .to_owned();
        let profiles = &self.retained_registrations;
        let index = &self.workspace_index;
        self.recent.retain(|existing| {
            existing != workspace_id
                && index
                    .get(existing)
                    .and_then(|profile| profiles.get(*profile))
                    .is_some_and(|profile| profile.repository_root() != root)
        });
        self.recent.insert(0, workspace_id.to_owned());
        self.recent.truncate(MAX_RECENT_WORKSPACES);
        self.point_repository_to(workspace_id);
        Ok(())
    }

    fn point_repository_to(&mut self, workspace_id: &str) {
        let Some(profile) = self.find(workspace_id) else {
            return;
        };
        let root = profile.repository_root().to_owned();
        let identity = profile.repository_identity().to_owned();
        self.repository_index
            .entry(root)
            .or_default()
            .entry(identity)
            .and_modify(|lineages| {
                lineages.workspaces.insert(workspace_id.to_owned());
                lineages.selected = workspace_id.to_owned();
            })
            .or_insert_with(|| RepositoryLineages {
                selected: workspace_id.to_owned(),
                workspaces: BTreeSet::from([workspace_id.to_owned()]),
            });
    }

    fn remove_repository_pointer(&mut self, profile: &WorkspaceProfile) {
        let root = profile.repository_root();
        let remove_root = self.repository_index.get_mut(root).is_some_and(|identities| {
            let remove_identity = identities
                .get_mut(profile.repository_identity())
                .is_some_and(|lineages| {
                    lineages.workspaces.remove(profile.workspace_id());
                    if lineages.selected == profile.workspace_id() {
                        lineages.selected =
                            lineages.workspaces.first().cloned().unwrap_or_default();
                    }
                    lineages.workspaces.is_empty()
                });
            if remove_identity {
                identities.remove(profile.repository_identity());
            }
            identities.is_empty()
        });
        if remove_root {
            self.repository_index.remove(root);
        }
    }

    fn rebuild_workspace_index(&mut self) -> Result<(), ProductStateError> {
        self.workspace_index.clear();
        for (index, profile) in self.retained_registrations.iter().enumerate() {
            profile.validate()?;
            if self
                .workspace_index
                .insert(profile.workspace_id().to_owned(), index)
                .is_some()
            {
                return Err(invalid("workspace registry contains a duplicate lineage identity"));
            }
        }
        Ok(())
    }

    fn rebuild_repository_index(&mut self) {
        self.repository_index.clear();
        for profile in &self.retained_registrations {
            self.repository_index
                .entry(profile.repository_root().to_owned())
                .or_default()
                .entry(profile.repository_identity().to_owned())
                .and_modify(|lineages: &mut RepositoryLineages| {
                    lineages.workspaces.insert(profile.workspace_id().to_owned());
                    lineages.selected = profile.workspace_id().to_owned();
                })
                .or_insert_with(|| RepositoryLineages {
                    selected: profile.workspace_id().to_owned(),
                    workspaces: BTreeSet::from([profile.workspace_id().to_owned()]),
                });
        }
        let recent = self.recent.clone();
        for workspace in recent.into_iter().rev() {
            self.point_repository_to(&workspace);
        }
        if let Some(active) = self.active_workspace_id.clone() {
            self.point_repository_to(&active);
        }
    }

    fn migrate_legacy(
        legacy: LegacyWorkspaceSelection,
    ) -> Result<Self, ProductStateError> {
        if legacy.recent.len() > MAX_RECENT_WORKSPACES {
            return Err(invalid("legacy workspace recency exceeds its presentation bound"));
        }
        let mut recent_identities = BTreeSet::new();
        let mut recent_roots = BTreeSet::new();
        for profile in &legacy.recent {
            profile.validate()?;
            if !recent_identities.insert(profile.workspace_id())
                || !recent_roots.insert(profile.repository_root())
            {
                return Err(invalid("legacy workspace recency is ambiguous"));
            }
        }
        if legacy
            .active_workspace_id
            .as_deref()
            .is_some_and(|active| !recent_identities.contains(active))
        {
            return Err(invalid("legacy active workspace is absent from recency"));
        }
        let mut registered_identities = BTreeSet::new();
        let mut registered_roots = BTreeSet::new();
        for profile in &legacy.recent {
            if !profile.is_direct_folder() && profile.trust_level() == WorkspaceTrust::Trusted {
                if !registered_identities.insert(profile.workspace_id())
                    || !registered_roots.insert(profile.repository_root())
                {
                    return Err(invalid("legacy managed workspace registrations are ambiguous"));
                }
            }
        }
        for profile in &legacy.retained_registrations {
            profile.validate()?;
            if profile.trust_level() != WorkspaceTrust::Trusted {
                return Err(invalid("legacy retained registration is not trusted"));
            }
            if !profile.is_direct_folder()
                && (!registered_identities.insert(profile.workspace_id())
                    || !registered_roots.insert(profile.repository_root()))
            {
                return Err(invalid("legacy managed workspace registrations are ambiguous"));
            }
        }
        let recent = legacy
            .recent
            .iter()
            .map(|profile| profile.workspace_id().to_owned())
            .collect();
        let mut selection = Self {
            retained_registrations: Vec::new(),
            recent,
            workspace_index: BTreeMap::new(),
            repository_index: BTreeMap::new(),
            active_workspace_id: legacy.active_workspace_id,
            legacy_storage: true,
        };
        for profile in legacy.retained_registrations.into_iter().chain(legacy.recent) {
            if let Some(index) = selection
                .retained_registrations
                .iter()
                .position(|existing| existing.workspace_id() == profile.workspace_id())
            {
                selection.retained_registrations[index] = profile;
            } else {
                selection.retained_registrations.push(profile);
            }
        }
        Ok(selection)
    }

    pub(crate) const fn storage_migration_required(&self) -> bool {
        self.legacy_storage
    }

    pub(crate) fn finish_storage_migration(&mut self) {
        self.legacy_storage = false;
    }

    pub(crate) fn validate(&self) -> Result<(), ProductStateError> {
        if self.recent.len() > MAX_RECENT_WORKSPACES
            || self.retained_registrations.iter().any(|profile| profile.validate().is_err())
            || self.workspace_index.len() != self.retained_registrations.len()
            || self.retained_registrations.iter().enumerate().any(|(index, profile)| {
                self.workspace_index.get(profile.workspace_id()) != Some(&index)
            })
            || self.active_workspace_id.as_deref().is_some_and(|active| {
                !self.workspace_index.contains_key(active)
            })
            || !self.valid_recent_index()
            || !self.valid_repository_index()
        {
            return Err(invalid("workspace selection is noncanonical or inconsistent"));
        }
        Ok(())
    }

    fn valid_recent_index(&self) -> bool {
        let mut identities = BTreeSet::new();
        let mut roots = BTreeSet::new();
        self.recent.iter().all(|workspace| {
            let Some(profile) = self.find(workspace) else {
                return false;
            };
            identities.insert(workspace.as_str()) && roots.insert(profile.repository_root())
        })
    }

    fn valid_repository_index(&self) -> bool {
        let mut required = BTreeSet::new();
        for profile in &self.retained_registrations {
            required.insert((profile.repository_root(), profile.repository_identity()));
            if !self
                .repository_index
                .get(profile.repository_root())
                .and_then(|identities| identities.get(profile.repository_identity()))
                .is_some_and(|lineages| lineages.workspaces.contains(profile.workspace_id()))
            {
                return false;
            }
        }
        let mut indexed = BTreeSet::new();
        for (root, identities) in &self.repository_index {
            if identities.is_empty() {
                return false;
            }
            for (identity, lineages) in identities {
                if lineages.workspaces.is_empty()
                    || !lineages.workspaces.contains(&lineages.selected)
                {
                    return false;
                }
                for workspace in &lineages.workspaces {
                    let Some(profile) = self.find(workspace) else {
                        return false;
                    };
                    if profile.repository_root() != root.as_str()
                        || profile.repository_identity() != identity.as_str()
                    {
                        return false;
                    }
                }
                indexed.insert((root.as_str(), identity.as_str()));
            }
        }
        indexed == required
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RepositoryLineages {
    selected: String,
    workspaces: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WorkspaceSelectionWire {
    Current(CurrentWorkspaceSelection),
    Legacy(LegacyWorkspaceSelection),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentWorkspaceSelection {
    #[serde(default)]
    registry: Vec<WorkspaceProfile>,
    #[serde(default)]
    recent: Vec<String>,
    #[serde(default)]
    repository_index: BTreeMap<String, BTreeMap<String, RepositoryLineages>>,
    active_workspace_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyWorkspaceSelection {
    #[serde(default)]
    recent: Vec<WorkspaceProfile>,
    #[serde(default)]
    retained_registrations: Vec<WorkspaceProfile>,
    active_workspace_id: Option<String>,
}

impl<'de> Deserialize<'de> for WorkspaceSelection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WorkspaceSelectionWire::deserialize(deserializer)?;
        let mut selection = match wire {
            WorkspaceSelectionWire::Current(current) => Self {
                retained_registrations: current.registry,
                recent: current.recent,
                workspace_index: BTreeMap::new(),
                repository_index: current.repository_index,
                active_workspace_id: current.active_workspace_id,
                legacy_storage: false,
            },
            WorkspaceSelectionWire::Legacy(legacy) =>
                Self::migrate_legacy(legacy).map_err(serde::de::Error::custom)?,
        };
        selection.rebuild_workspace_index().map_err(serde::de::Error::custom)?;
        if selection.legacy_storage {
            selection.rebuild_repository_index();
        }
        selection.validate().map_err(serde::de::Error::custom)?;
        Ok(selection)
    }
}

fn invalid(detail: &'static str) -> ProductStateError {
    ProductStateError::InvalidPayload(detail.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(root: &str, workspace: u8) -> WorkspaceProfile {
        WorkspaceProfile::restricted(
            root.to_owned(),
            "01".repeat(32),
            "02".repeat(16),
            format!("{workspace:02x}").repeat(16),
            "04".repeat(16),
            "05".repeat(16),
        )
        .expect("profile")
    }

    #[test]
    fn activation_is_recent_unique_and_removal_selects_the_next_entry() {
        let mut selection = WorkspaceSelection::default();
        selection.activate(profile("/one", 3)).expect("first");
        selection.activate(profile("/two", 6)).expect("second");
        assert_eq!(selection.active().expect("active").repository_root(), "/two");
        selection.select(&"03".repeat(16)).expect("switch");
        assert_eq!(selection.recent()[0].repository_root(), "/one");
        assert!(selection.remove(&"03".repeat(16)));
        assert_eq!(selection.active().expect("fallback").repository_root(), "/two");
    }

    #[test]
    fn replacing_a_trusted_repository_retains_its_daemon_registration() {
        let mut selection = WorkspaceSelection::default();
        let trusted = profile("/repo", 3)
            .trust(
                "/state/registration.bin".to_owned(),
                "06".repeat(32),
                "/state/worktree".to_owned(),
                "/state/transactions".to_owned(),
            )
            .expect("trusted");
        selection.activate(trusted).expect("first identity");
        selection.activate(profile("/repo", 6)).expect("replacement identity");
        assert_eq!(selection.recent().len(), 1);
        assert_eq!(selection.registered().len(), 1);
        assert_eq!(selection.active().expect("active").trust_level(), WorkspaceTrust::Restricted);
    }

    #[test]
    fn refreshing_a_trusted_workspace_replaces_its_old_registration() {
        let mut selection = WorkspaceSelection::default();
        let original = profile("/repo", 3)
            .trust(
                "/state/registration.bin".to_owned(),
                "06".repeat(32),
                "/state/worktree".to_owned(),
                "/state/transactions".to_owned(),
            )
            .expect("trusted");
        selection.activate(original.clone()).expect("original registration");
        let refreshed = original
            .trust(
                "/state/registration.bin".to_owned(),
                "07".repeat(32),
                "/state/worktree".to_owned(),
                "/state/transactions".to_owned(),
            )
            .expect("refreshed");

        selection.activate(refreshed).expect("refresh registration");

        assert_eq!(selection.recent().len(), 1);
        assert_eq!(selection.registered().len(), 1);
        assert_eq!(selection.recent()[0].registration_digest(), Some(&*"07".repeat(32)));
    }

    #[test]
    fn retained_registration_history_keeps_every_trusted_workspace() {
        let mut selection = WorkspaceSelection::default();
        for index in 1..=4_097 {
            let workspace_id = format!("{index:032x}");
            let trusted = WorkspaceProfile::restricted(
                format!("/repo/{index}"),
                "01".repeat(32),
                "02".repeat(16),
                workspace_id.clone(),
                "04".repeat(16),
                "05".repeat(16),
            )
            .expect("profile")
            .trust(
                format!("/state/{index}/registration.bin"),
                "06".repeat(32),
                format!("/state/{index}/worktree"),
                format!("/state/{index}/transactions"),
            )
            .expect("trusted");
            selection.retain_registration(trusted);
        }

        assert_eq!(selection.retained_registrations.len(), 4_097);
        assert!(
            selection
                .retained_registrations
                .iter()
                .any(|profile| profile.workspace_id() == "00000000000000000000000000000001")
        );
        assert_eq!(
            selection.retained_registrations.last().expect("newest").workspace_id(),
            format!("{:032x}", 4_097)
        );
    }
}
