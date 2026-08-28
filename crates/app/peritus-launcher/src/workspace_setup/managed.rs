//! Application-managed writable worktree publication and health inspection.

use std::{fs, path::Path};

use peritus_git::{CreateWorktree, RepositoryOptions, WorktreeAccess, WorktreeName};
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
    WorkspaceProfile::restricted(
        repository.root_text().to_owned(),
        repository.identity_text().to_owned(),
        hex(&bytes[0..16]),
        hex(&bytes[16..32]),
        hex(&bytes[32..48]),
        hex(&bytes[48..64]),
    )
    .map_err(LauncherError::from)
}

/// Creates or recovers a managed detached worktree, then publishes its exact C1 registration.
pub fn trust(
    layout: &AppLayout,
    repository: &DiscoveredRepository,
    profile: WorkspaceProfile,
) -> Result<WorkspaceProfile, LauncherError> {
    let repair = profile.trust_level() == WorkspaceTrust::Trusted;
    let baseline = repository.repository().resolve_baseline("HEAD")?;
    let leaf = format!("workspace_{}", &profile.workspace_id()[..16]);
    let name = WorktreeName::new(leaf.clone())?;
    let destination = layout.managed_workspaces_root().join(&leaf);
    let request = CreateWorktree::new(name, &destination, baseline, WorktreeAccess::Writable);
    let worktree = if destination.exists() {
        repository.repository().recover_existing_worktree(request)?
    } else {
        repository.repository().create_worktree(request)?
    };
    let transaction_root = layout.prepare_workspace_transaction(profile.workspace_id())?;
    let binding = WorkspaceBinding::new(
        workspace_id(&profile)?,
        resource_id(&profile)?,
        environment_id(&profile)?,
        worktree.root().to_owned(),
        baseline.commit(),
        baseline.tree(),
    )?;
    let registration = WorkspaceRegistration::new(
        &binding,
        repository.repository(),
        &worktree,
        transaction_root.clone(),
    )?;
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
mod tests {
    use std::{path::PathBuf, process::Command};

    use super::*;
    use crate::ProductBootstrap;

    #[test]
    fn trust_creates_registered_managed_copy_and_reports_dirty_state() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let source = initialized_repository(temporary.path());
        let nested = source.join("src/nested");
        fs::create_dir_all(&nested).expect("nested directory");
        let repository = DiscoveredRepository::open(&nested).expect("descendant discovery");
        assert_eq!(repository.repository().identity().repository_root(), source);
        let profile = new_profile(&repository).expect("restricted profile");
        assert_eq!(health(&profile), WorkspaceHealth::Restricted);

        let layout = AppLayout::for_test(&temporary.path().join("application"))
            .prepare()
            .expect("application layout");
        let trusted = trust(&layout, &repository, profile).expect("trusted workspace");
        assert_eq!(health(&trusted), WorkspaceHealth::Ready);
        assert!(Path::new(trusted.registration_file().expect("registration")).is_file());
        assert_ne!(trusted.managed_root(), Some(source.to_str().expect("source path")));

        let managed_file =
            Path::new(trusted.managed_root().expect("managed root")).join("file.txt");
        fs::write(managed_file, "changed\n").expect("modify managed copy");
        assert_eq!(health(&trusted), WorkspaceHealth::Dirty);
    }

    #[test]
    fn interrupted_registration_recovers_and_removed_recent_keeps_daemon_catalog_valid() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let source = initialized_repository(temporary.path());
        let repository = DiscoveredRepository::open(&source).expect("repository");
        let layout = AppLayout::for_test(&temporary.path().join("application"))
            .prepare()
            .expect("application layout");
        let trusted = trust(&layout, &repository, new_profile(&repository).expect("profile"))
            .expect("trusted workspace");
        fs::remove_file(trusted.registration_file().expect("registration"))
            .expect("simulate interrupted registration publication");
        let recovered = trust(&layout, &repository, trusted.restrict()).expect("recovery");
        assert_eq!(health(&recovered), WorkspaceHealth::Ready);
        fs::write(recovered.registration_file().expect("registration"), b"interrupted")
            .expect("simulate interrupted repair");
        let repaired = trust(&layout, &repository, recovered).expect("repair registration");
        assert_eq!(health(&repaired), WorkspaceHealth::Ready);

        let configured = ProductBootstrap::new(layout.clone())
            .configure_workspace(repaired.clone())
            .expect("configure workspace");
        assert_eq!(configured.daemon_config().projects().len(), 1);
        assert_eq!(configured.daemon_config().workspaces().len(), 1);
        assert!(!configured.daemon_config().tools().allowed().is_empty());

        let forgotten = ProductBootstrap::new(layout)
            .remove_workspace(repaired.workspace_id())
            .expect("forget recent workspace");
        assert!(forgotten.state().workspaces().recent().is_empty());
        assert_eq!(forgotten.daemon_config().workspaces().len(), 1);
        assert!(forgotten.daemon_config().tools().allowed().is_empty());
    }

    #[test]
    fn moved_source_repository_is_reported_as_repairable() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let source = initialized_repository(temporary.path());
        let repository = DiscoveredRepository::open(&source).expect("repository");
        let profile = new_profile(&repository).expect("profile");
        fs::rename(&source, temporary.path().join("moved")).expect("move source repository");
        assert_eq!(health(&profile), WorkspaceHealth::NeedsRepair);
    }

    fn initialized_repository(root: &Path) -> PathBuf {
        let source = root.join("source");
        fs::create_dir_all(&source).expect("source directory");
        git(&source, &["init", "--quiet"]);
        git(&source, &["config", "user.name", "Peritus Test"]);
        git(&source, &["config", "user.email", "peritus@example.invalid"]);
        fs::write(source.join("file.txt"), "initial\n").expect("source file");
        git(&source, &["add", "file.txt"]);
        git(&source, &["commit", "--quiet", "-m", "initial"]);
        fs::canonicalize(source).expect("canonical source")
    }

    fn git(root: &Path, arguments: &[&str]) {
        let status =
            Command::new("git").current_dir(root).args(arguments).status().expect("run git");
        assert!(status.success(), "git {arguments:?} failed with {status}");
    }
}
