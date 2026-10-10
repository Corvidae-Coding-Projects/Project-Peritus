//! Keep native filesystem paths distinct from Git tree names and local command operands.

use super::failure;
use crate::ProductRunnerError;
use std::path::{Component, Path};

pub(super) fn tree_name(path: &Path) -> Result<String, ProductRunnerError> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => {
                value.to_str().ok_or_else(|| failure("Git tree path is not UTF-8"))
            }
            _ => Err(failure("Git tree path is not relative")),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
}

pub(super) fn local_path(path: &Path) -> Result<String, ProductRunnerError> {
    let absolute = path.canonicalize().map_err(failure)?;
    let text = absolute.to_str().ok_or_else(|| failure("repository path is not UTF-8"))?;
    #[cfg(windows)]
    {
        // Git's transport parser mistakes verbatim drive paths for host:path SSH operands.
        let text = text.strip_prefix(r"\\?\").unwrap_or(text);
        let text =
            text.strip_prefix(r"UNC\").map_or_else(|| text.to_owned(), |unc| format!(r"\\{unc}"));
        Ok(text.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    Ok(text.to_owned())
}
