//! Explicit-path-only media for direct folders: no recursive discovery or private-state reads.

use super::{
    MAX_IMAGES, ProductRunnerError, ProviderProfile, WorkspaceImages, attach, supported_extension,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub fn discover_explicit(
    root: &Path,
    task: &str,
    profile: &ProviderProfile,
    protected: &[PathBuf],
) -> Result<WorkspaceImages, ProductRunnerError> {
    let paths = task
        .split_whitespace()
        .chain(task.split(['`', '"', '\n']))
        .filter_map(|token| {
            let token = token.trim().trim_matches(['`', '"', '\'', '(', ')', ',', ';']);
            let path = Path::new(token);
            if !supported_extension(path) {
                return None;
            }
            let relative = if path.is_absolute() { path.strip_prefix(root).ok()? } else { path };
            crate::developer_tools::checked_protected_file(
                root,
                relative.to_str()?,
                task,
                protected,
            )
            .ok()
        })
        .filter(|path| path.is_file())
        .take(MAX_IMAGES)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    attach(root, paths, profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_never_discovers_or_attaches_unmentioned_images() {
        let root = tempfile::tempdir().expect("folder");
        std::fs::write(root.path().join("private.png"), b"not an image").expect("private file");
        let images =
            discover_explicit(root.path(), "hello", &super::super::tests::profile(false), &[]);
        assert!(images.is_ok());
    }

    #[test]
    fn only_explicit_non_private_paths_are_attached() {
        let root = tempfile::tempdir().expect("folder");
        let private = root.path().join("private");
        std::fs::create_dir(&private).expect("private directory");
        std::fs::write(private.join("secret.png"), b"invalid private image").expect("private file");
        std::fs::write(root.path().join("public photo.png"), b"\x89PNG\r\n\x1a\npixels")
            .expect("image");
        let images = discover_explicit(
            root.path(),
            "Inspect `public photo.png` and private/secret.png",
            &super::super::tests::profile(true),
            &[private],
        )
        .expect("explicit media");
        assert_eq!(images.attachments.len(), 1);
        assert!(images.manifest.contains("public photo.png"));
        assert!(!images.manifest.contains("secret"));
    }
}
