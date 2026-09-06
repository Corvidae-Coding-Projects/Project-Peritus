//! Explicit direct-folder declarations, distinct from Git-backed C1 registrations.

use super::{decode_identifier, invalid};
use crate::DaemonError;
use peritus_types::WorkspaceId;
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

/// One directory available for conversational work, with explicit in-place effect authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FolderDeclaration {
    workspace_id: String,
    root: PathBuf,
    identity: String,
    writable: bool,
    protected_paths: Vec<PathBuf>,
}

impl FolderDeclaration {
    /// Returns the validated workspace identity.
    pub fn workspace_id(&self) -> Result<WorkspaceId, DaemonError> {
        WorkspaceId::new(decode_identifier(&self.workspace_id, "folder workspace identity")?)
            .map_err(|_| invalid("folder workspace identity must be nonzero"))
    }
    /// Borrows the selected directory; it is not a managed copy.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Whether the user explicitly trusted requested in-place effects.
    #[must_use]
    pub const fn writable(&self) -> bool {
        self.writable
    }
    /// Private Peritus directories excluded from ordinary file-tool access.
    #[must_use]
    pub fn protected_paths(&self) -> &[PathBuf] {
        &self.protected_paths
    }
    /// Revalidates directory identity without inspecting its contents or invoking Git.
    pub(crate) fn verify(&self) -> Result<(), DaemonError> {
        let observed = peritus_workspace::FolderIdentity::observe(&self.root)
            .map_err(|_| invalid("direct folder is unavailable; select or trust it again"))?;
        let actual = observed.digest().as_bytes().iter().fold(String::new(), |mut value, byte| {
            use std::fmt::Write as _;
            let _ = write!(value, "{byte:02x}");
            value
        });
        if observed.root() != self.root || actual != self.identity {
            return Err(invalid("direct folder identity changed; select or trust it again"));
        }
        Ok(())
    }
}

pub(super) fn validate(folders: &[FolderDeclaration]) -> Result<(), DaemonError> {
    if folders.len() > 32 {
        return Err(invalid("direct folder inventory exceeds its bound"));
    }
    let mut ids = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for folder in folders {
        if !ids.insert(folder.workspace_id()?)
            || !roots.insert(&folder.root)
            || !normal_absolute(&folder.root)
            || folder.identity.len() != 64
            || !folder.identity.bytes().all(|byte| byte.is_ascii_hexdigit())
            || folder.protected_paths.len() > 16
            || folder.protected_paths.iter().any(|path| !normal_absolute(path))
        {
            return Err(invalid("direct folder declaration is malformed or duplicated"));
        }
    }
    Ok(())
}

fn normal_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|component| !matches!(component, Component::ParentDir | Component::CurDir))
}
