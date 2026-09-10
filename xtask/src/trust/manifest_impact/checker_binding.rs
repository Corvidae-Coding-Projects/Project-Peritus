//! Bind the executable authorization checker to the independently reviewed candidate.

use super::candidate_tree;
use crate::error::{Diagnostic, XtaskError};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn validate(
    root: &Path,
    tree: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let candidate = candidate_tree::entries(root, tree)?;
    let tracked = candidate_tree::git(
        root,
        &["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    )?;
    let mut paths: BTreeSet<PathBuf> =
        candidate.keys().filter(|path| is_checker(path)).cloned().collect();
    for path in tracked.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        let text = std::str::from_utf8(path)
            .map_err(|_| XtaskError::metadata("authorization checker path is not UTF-8"))?;
        if is_checker(Path::new(text)) {
            paths.insert(PathBuf::from(text));
        }
    }
    for path in paths {
        let expected = candidate
            .get(&path)
            .map(|object| candidate_tree::git(root, &["cat-file", "blob", object]))
            .transpose()?;
        let actual = match fs::symlink_metadata(root.join(&path)) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Some(
                fs::read(root.join(&path))
                    .map_err(|error| XtaskError::io("read checker binding", &path, error))?,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Ok(_) => {
                diagnostics.push(mismatch(&path));
                continue;
            }
            Err(error) => return Err(XtaskError::io("inspect checker binding", &path, error)),
        };
        if actual != expected {
            diagnostics.push(mismatch(&path));
        }
    }
    Ok(())
}

fn is_checker(path: &Path) -> bool {
    path.starts_with("xtask/src") || path.starts_with("xtask/tests")
}

fn mismatch(path: &Path) -> Diagnostic {
    Diagnostic::at(
        path,
        "authorization checker bytes differ from the independently reviewed candidate",
        "use the exact candidate checker and tests, including additions and deletions, before authorizing its formal source",
    )
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::trust::manifest_impact::candidate_tree::{CandidateTree, git};
    use std::fs;

    #[test]
    fn rejects_authorization_only_checker_edits_omissions_and_additions() {
        let directory = CandidateTree::create().expect("temporary fixture");
        let root = directory.root();
        git(root, &["init", "--quiet"]).expect("initialize fixture");
        fs::create_dir_all(root.join("xtask/src")).expect("checker directory");
        fs::write(root.join("xtask/src/lib.rs"), "pub fn check() -> bool { true }\n")
            .expect("candidate checker");
        git(root, &["add", "xtask/src/lib.rs"]).expect("stage fixture");
        let tree = String::from_utf8(git(root, &["write-tree"]).expect("candidate tree"))
            .expect("tree ID");
        let mut diagnostics = Vec::new();
        validate(root, tree.trim(), &mut diagnostics).expect("matching checker binding");
        assert!(diagnostics.is_empty());
        fs::write(root.join("xtask/src/lib.rs"), "pub fn check() -> bool { false }\n")
            .expect("different checker");
        validate(root, tree.trim(), &mut diagnostics).expect("mismatch check");
        assert_eq!(diagnostics.len(), 1);
        fs::remove_file(root.join("xtask/src/lib.rs")).expect("fixture checker omission");
        diagnostics.clear();
        validate(root, tree.trim(), &mut diagnostics).expect("omission check");
        assert_eq!(diagnostics.len(), 1);
        fs::write(root.join("xtask/src/lib.rs"), "pub fn check() -> bool { true }\n")
            .expect("restore fixture bytes");
        fs::write(root.join("xtask/src/extra.rs"), "pub const VALUE: u8 = 1;\n")
            .expect("unreviewed checker addition");
        diagnostics.clear();
        validate(root, tree.trim(), &mut diagnostics).expect("addition check");
        assert_eq!(diagnostics.len(), 1);
    }
}
