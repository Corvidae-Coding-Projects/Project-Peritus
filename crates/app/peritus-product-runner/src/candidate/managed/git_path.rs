//! Keep native filesystem paths distinct from Git tree names and local fetch operands.

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

pub(super) fn local_repository(path: &Path) -> Result<String, ProductRunnerError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_tree_names_use_git_separators() {
        assert_eq!(
            tree_name(&Path::new("nested").join("child").join("file.txt")).unwrap(),
            "nested/child/file.txt"
        );
        assert!(tree_name(Path::new("../outside")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn unix_backslash_is_a_literal_filename_character() {
        assert_eq!(tree_name(Path::new(r"name\part")).unwrap(), r"name\part");
    }

    #[test]
    fn canonical_repository_operand_remains_local() {
        let directory = tempfile::tempdir().unwrap();
        let operand = local_repository(directory.path()).unwrap();
        assert_eq!(
            Path::new(&operand).canonicalize().unwrap(),
            directory.path().canonicalize().unwrap()
        );
        #[cfg(windows)]
        assert!(!operand.contains('\\'));
    }
}
