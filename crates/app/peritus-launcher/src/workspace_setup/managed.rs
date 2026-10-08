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
    persistence::read_exact_or_publish,
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
    /// The same trusted detached worktree has a newer baseline to register.
    Advanced,
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
            Self::Advanced => "Ready — committed changes",
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
    let registration_path = publish_registration(layout, &profile, &registration)?;
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
    let trusted = profile
        .trust(
            path_text(&registration_path)?,
            hex(registration.digest().as_bytes()),
            path_text(worktree.root())?,
            path_text(&transaction_root)?,
        )
        .map_err(LauncherError::from)?;
    validate_registration_publication(&trusted)?;
    Ok(trusted)
}

/// Revalidates one recent workspace without changing its source checkout.
#[must_use]
pub fn observe_health(profile: &WorkspaceProfile) -> Result<WorkspaceHealth, LauncherError> {
    if profile.is_direct_folder() {
        let folder = DiscoveredRepository::folder(Path::new(profile.repository_root()))?;
        if folder.identity_text() != profile.repository_identity() {
            return Ok(WorkspaceHealth::NeedsRepair);
        }
        return Ok(if profile.trust_level() == WorkspaceTrust::Trusted {
            WorkspaceHealth::Ready
        } else {
            WorkspaceHealth::Restricted
        });
    }
    let repository =
        peritus_git::GitRepository::open(RepositoryOptions::new(profile.repository_root()))?;
    if hex(repository.identity().digest().as_bytes()) != profile.repository_identity() {
        return Ok(WorkspaceHealth::NeedsRepair);
    }
    if profile.trust_level() == WorkspaceTrust::Restricted {
        return Ok(WorkspaceHealth::Restricted);
    }
    let Some(registration_file) = profile.registration_file() else {
        return Ok(WorkspaceHealth::NeedsRepair);
    };
    let bytes = match fs::read(registration_file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspaceHealth::NeedsRepair);
        }
        Err(error) => {
            return Err(LauncherError::filesystem(
                "read workspace registration during health inspection",
                registration_file,
                error,
            ));
        }
    };
    let Ok(registration) = WorkspaceRegistration::decode(&bytes) else {
        return Ok(WorkspaceHealth::NeedsRepair);
    };
    if !registration_matches_profile(&registration, profile)? {
        return Ok(WorkspaceHealth::NeedsRepair);
    }
    let worktree = match repository.reopen_worktree(registration.worktree_manifest()) {
        Ok(worktree) => worktree,
        Err(reopen_error) => {
        let name = WorktreeName::new(format!("workspace_{}", &profile.workspace_id()[..16]))?;
        let request = RecoverWorktree::new(
            name,
            registration.worktree_manifest().root(),
            WorktreeAccess::Writable,
        );
        return match repository.recover_current_worktree(request) {
            Ok(current)
                if current.repository_digest()
                    == registration.worktree_manifest().repository_digest()
                    && current.baseline() != registration.worktree_manifest().baseline()
                    && registration.worktree_manifest().access() == WorktreeAccess::Writable =>
            {
                Ok(WorkspaceHealth::Advanced)
            }
            Ok(_) => Ok(WorkspaceHealth::NeedsRepair),
            Err(_) => Err(reopen_error.into()),
        };
        }
    };
    let status = repository.status(&worktree)?;
    Ok(if status.is_clean() { WorkspaceHealth::Ready } else { WorkspaceHealth::Dirty })
}

#[cfg(test)]
#[must_use]
pub fn health(profile: &WorkspaceProfile) -> WorkspaceHealth {
    observe_health(profile).unwrap_or(WorkspaceHealth::NeedsRepair)
}

pub(crate) fn validate_registration_publication(
    profile: &WorkspaceProfile,
) -> Result<(), LauncherError> {
    if profile.is_direct_folder() || profile.trust_level() == WorkspaceTrust::Restricted {
        return Ok(());
    }
    let registration_file = profile.registration_file().ok_or_else(|| {
        LauncherError::WorkspaceSetup(
            "trusted managed workspace has no registration publication".to_owned(),
        )
    })?;
    let bytes = fs::read(registration_file).map_err(|error| {
        LauncherError::filesystem(
            "read workspace registration publication",
            registration_file,
            error,
        )
    })?;
    let registration = WorkspaceRegistration::decode(&bytes)?;
    if !registration_matches_profile(&registration, profile)? {
        return Err(LauncherError::WorkspaceSetup(
            "workspace registration publication differs from its exact profile binding"
                .to_owned(),
        ));
    }
    Ok(())
}

fn publish_registration(
    layout: &AppLayout,
    profile: &WorkspaceProfile,
    registration: &WorkspaceRegistration,
) -> Result<std::path::PathBuf, LauncherError> {
    let digest = hex(registration.digest().as_bytes());
    let mut recovery = 0_u64;
    loop {
        let path = layout.workspace_registration_publication_file(
            profile.workspace_id(),
            &digest,
            recovery,
        );
        let actual = read_exact_or_publish(&path, registration.canonical_bytes())?;
        if actual == registration.canonical_bytes() {
            return Ok(path);
        }
        recovery = recovery.checked_add(1).ok_or_else(|| {
            LauncherError::WorkspaceSetup(
                "workspace registration recovery identity is exhausted".to_owned(),
            )
        })?;
    }
}

fn registration_matches_profile(
    registration: &WorkspaceRegistration,
    profile: &WorkspaceProfile,
) -> Result<bool, LauncherError> {
    Ok(hex(registration.digest().as_bytes())
        == profile.registration_digest().unwrap_or_default()
        && registration.workspace_id() == workspace_id(profile)?
        && registration.resource_id() == resource_id(profile)?
        && registration.environment_id() == environment_id(profile)?
        && registration.repository_root() == Path::new(profile.repository_root())
        && hex(registration.worktree_manifest().repository_digest().as_bytes())
            == profile.repository_identity()
        && registration.worktree_manifest().root()
            == Path::new(profile.managed_root().unwrap_or_default())
        && registration.transaction_root()
            == Path::new(profile.transaction_root().unwrap_or_default()))
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
