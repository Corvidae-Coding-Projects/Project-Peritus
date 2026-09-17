//! Application-managed writable worktree publication and health inspection.

use std::{fs, path::Path};

use peritus_git::{
    CreateWorktree, RecoverWorktree, RepositoryOptions, WorktreeAccess, WorktreeName,
};
use peritus_product_state::{WorkspaceProfile, WorkspaceTrust};
use peritus_types::{EnvironmentId, ResourceId, WorkspaceId};
use peritus_workspace::{WorkspaceBinding, WorkspaceRegistration};

use super::discovery::{DiscoveredRepository, hex};
use crate::{
    AppLayout, LauncherError,
    persistence::{read_exact_or_publish, replace_recovery_file},
};

/// User-facing state of one recent workspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceHealth {
    /// Repository is known but no execution or mutation authority was granted.
    Restricted,
    /// Trusted managed worktree is available and clean.
    Ready,
    /// Trusted managed worktree is available and contains an unfinished change.
    Dirty,
    /// Retained paths or repository identity no longer match and setup can repair it.
    NeedsRepair,
}

impl WorkspaceHealth {
    /// Returns concise status text for menus.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Restricted => "Restricted",
            Self::Ready => "Ready",
            Self::Dirty => "Ready — changes in progress",
            Self::NeedsRepair => "Needs repair",
        }
    }
}

/// Creates stable nominal identities for a newly remembered repository.
pub fn new_profile(repository: &DiscoveredRepository) -> Result<WorkspaceProfile, LauncherError> {
    let mut bytes = [0_u8; 64];
    getrandom::fill(&mut bytes).map_err(|error| LauncherError::Random(error.to_string()))?;
    for chunk in bytes.chunks_exact_mut(16) {
        if chunk.iter().all(|byte| *byte == 0) {
            chunk[0] = 1;
        }
    }
    let profile = WorkspaceProfile::restricted(
        repository.root_text().to_owned(),
        repository.identity_text().to_owned(),
        hex(&bytes[0..16]),
        hex(&bytes[16..32]),
        hex(&bytes[32..48]),
        hex(&bytes[48..64]),
    )
    .map_err(LauncherError::from)?;
    if repository.repository().is_none() {
        profile.into_direct_folder().map_err(LauncherError::from)
    } else {
        Ok(profile)
    }
}

/// Creates or recovers a managed detached worktree, then publishes its exact C1 registration.
pub fn trust(
    layout: &AppLayout,
    repository: &DiscoveredRepository,
    profile: WorkspaceProfile,
) -> Result<WorkspaceProfile, LauncherError> {
    if profile.is_direct_folder() {
        if repository.repository().is_some()
            || repository.root_text() != profile.repository_root()
            || repository.identity_text() != profile.repository_identity()
        {
            return Err(LauncherError::WorkspaceSetup(
                "folder identity changed; add and trust the selected folder again".to_owned(),
            ));
        }
        return profile.trust_folder().map_err(LauncherError::from);
    }
    let repository = repository.repository().ok_or_else(|| {
        LauncherError::WorkspaceSetup("managed Git workspace has no repository adapter".to_owned())
    })?;
    let repair = profile.trust_level() == WorkspaceTrust::Trusted;
    let baseline = repository.resolve_baseline("HEAD")?;
    let leaf = format!("workspace_{}", &profile.workspace_id()[..16]);
    let name = WorktreeName::new(leaf.clone())?;
    let destination = layout.managed_workspaces_root().join(&leaf);
    let worktree = if destination.exists() {
        if repair {
            repository.recover_current_worktree(RecoverWorktree::new(
                name,
                &destination,
                WorktreeAccess::Writable,
            ))?
        } else {
            repository.recover_existing_worktree(CreateWorktree::new(
                name,
                &destination,
                baseline,
                WorktreeAccess::Writable,
            ))?
        }
    } else {
        repository.create_worktree(CreateWorktree::new(
            name,
            &destination,
            baseline,
            WorktreeAccess::Writable,
        ))?
    };
    let transaction_root = layout.prepare_workspace_transaction(profile.workspace_id())?;
    let managed_baseline = worktree.baseline();
    let binding = WorkspaceBinding::new(
        workspace_id(&profile)?,
        resource_id(&profile)?,
        environment_id(&profile)?,
        worktree.root().to_owned(),
        managed_baseline.commit(),
        managed_baseline.tree(),
    )?;
    let registration =
        WorkspaceRegistration::new(&binding, repository, &worktree, transaction_root.clone())?;
    let registration_path = layout.workspace_registration_file(profile.workspace_id());
    let actual = read_exact_or_publish(&registration_path, registration.canonical_bytes())?;
    if actual != registration.canonical_bytes() && repair {
        replace_recovery_file(&registration_path, registration.canonical_bytes())?;
    } else if actual != registration.canonical_bytes() {
        return Err(LauncherError::WorkspaceSetup(
            "a workspace registration already exists with different content".to_owned(),
        ));
    }
    let decoded =
        WorkspaceRegistration::decode(&fs::read(&registration_path).map_err(|error| {
            LauncherError::filesystem(
                "read published workspace registration",
                &registration_path,
                error,
            )
        })?)?;
    if decoded != registration {
        return Err(LauncherError::WorkspaceSetup(
            "the repaired workspace registration did not validate".to_owned(),
        ));
    }
    profile
        .trust(
            path_text(&registration_path)?,
            hex(registration.digest().as_bytes()),
            path_text(worktree.root())?,
            path_text(&transaction_root)?,
        )
        .map_err(LauncherError::from)
}

/// Revalidates one recent workspace without changing its source checkout.
#[must_use]
pub fn health(profile: &WorkspaceProfile) -> WorkspaceHealth {
    if profile.is_direct_folder() {
        return match DiscoveredRepository::folder(Path::new(profile.repository_root())) {
            Ok(folder) if folder.identity_text() == profile.repository_identity() => {
                if profile.trust_level() == WorkspaceTrust::Trusted {
                    WorkspaceHealth::Ready
                } else {
                    WorkspaceHealth::Restricted
                }
            }
            _ => WorkspaceHealth::NeedsRepair,
        };
    }
    let Ok(repository) =
        peritus_git::GitRepository::open(RepositoryOptions::new(profile.repository_root()))
    else {
        return WorkspaceHealth::NeedsRepair;
    };
    if hex(repository.identity().digest().as_bytes()) != profile.repository_identity() {
        return WorkspaceHealth::NeedsRepair;
    }
    if profile.trust_level() == WorkspaceTrust::Restricted {
        return WorkspaceHealth::Restricted;
    }
    let Some(registration_file) = profile.registration_file() else {
        return WorkspaceHealth::NeedsRepair;
    };
    let Ok(bytes) = fs::read(registration_file) else {
        return WorkspaceHealth::NeedsRepair;
    };
    let Ok(registration) = WorkspaceRegistration::decode(&bytes) else {
        return WorkspaceHealth::NeedsRepair;
    };
    if hex(registration.digest().as_bytes()) != profile.registration_digest().unwrap_or_default()
        || registration.repository_root() != Path::new(profile.repository_root())
        || registration.worktree_manifest().root()
            != Path::new(profile.managed_root().unwrap_or_default())
        || registration.transaction_root()
            != Path::new(profile.transaction_root().unwrap_or_default())
    {
        return WorkspaceHealth::NeedsRepair;
    }
    let Ok(worktree) = repository.reopen_worktree(registration.worktree_manifest()) else {
        return WorkspaceHealth::NeedsRepair;
    };
    match repository.status(&worktree) {
        Ok(status) if status.is_clean() => WorkspaceHealth::Ready,
        Ok(_) => WorkspaceHealth::Dirty,
        Err(_) => WorkspaceHealth::NeedsRepair,
    }
}

fn workspace_id(profile: &WorkspaceProfile) -> Result<WorkspaceId, LauncherError> {
    WorkspaceId::new(identifier(profile.workspace_id())?)
        .map_err(|_| LauncherError::WorkspaceSetup("workspace identity is zero".to_owned()))
}

fn resource_id(profile: &WorkspaceProfile) -> Result<ResourceId, LauncherError> {
    ResourceId::new(identifier(profile.resource_id())?)
        .map_err(|_| LauncherError::WorkspaceSetup("resource identity is zero".to_owned()))
}

fn environment_id(profile: &WorkspaceProfile) -> Result<EnvironmentId, LauncherError> {
    EnvironmentId::new(identifier(profile.environment_id())?)
        .map_err(|_| LauncherError::WorkspaceSetup("environment identity is zero".to_owned()))
}

fn identifier(value: &str) -> Result<[u8; 16], LauncherError> {
    let mut output = [0_u8; 16];
    if value.len() != 32 {
        return Err(LauncherError::WorkspaceSetup("workspace identity is malformed".to_owned()));
    }
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(output)
}

fn nibble(byte: u8) -> Result<u8, LauncherError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(LauncherError::WorkspaceSetup("workspace identity is malformed".to_owned())),
    }
}

fn path_text(path: &Path) -> Result<String, LauncherError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        LauncherError::WorkspaceSetup("a managed workspace path is not valid UTF-8".to_owned())
    })
}

#[cfg(test)]
mod tests;
