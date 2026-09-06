//! Minimal bootstrap mounts for the explicit private filesystem mode.

use super::{LinuxError, MountAction, filesystem_error};
use peritus_sandbox::{CheckedSandboxPlan, FileDecision, FileOperation, SandboxPath};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn bootstrap(
    actions: &mut Vec<MountAction>,
    plan: &CheckedSandboxPlan,
    helper: &Path,
) -> Result<(), LinuxError> {
    actions.push(MountAction::ReadOnlyBind {
        source: helper.to_path_buf(),
        target: helper.to_path_buf(),
    });
    // ELF interpreter paths commonly use these fixed system aliases. Bind an alias only when
    // its canonical runtime tree is already admitted by the checked filesystem contract.
    for alias in ["/lib", "/lib64"] {
        let alias = PathBuf::from(alias);
        let Ok(source) = fs::canonicalize(&alias) else {
            continue;
        };
        if source == alias {
            continue;
        }
        let logical = SandboxPath::new(source.to_string_lossy().into_owned())
            .map_err(|_| filesystem_error("invalid runtime alias source"))?;
        if plan.contract().filesystem().decide(&logical, FileOperation::Read)
            == FileDecision::Allowed
        {
            actions.push(MountAction::ReadOnlyBind { source, target: alias });
        }
    }
    Ok(())
}
