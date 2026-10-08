//! Explicit-path-only media for direct folders: no recursive discovery or private-state reads.

use super::{
    MediaContext, ProductRunnerError, ProviderProfile, WorkspaceImages, attach,
    supported_extension,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

pub fn discover_explicit(
    root: &Path,
    task: &str,
    profile: &ProviderProfile,
    protected: &[PathBuf],
) -> Result<WorkspaceImages, ProductRunnerError> {
    discover_explicit_with_context(
        root,
        task,
        profile,
        protected,
        MediaContext::compatibility(),
    )
}

pub(crate) fn discover_explicit_retained(
    root: &Path,
    task: &str,
    profile: &ProviderProfile,
    protected: &[PathBuf],
    retention_root: &Path,
    cancelled: &AtomicBool,
) -> Result<WorkspaceImages, ProductRunnerError> {
    discover_explicit_with_context(
        root,
        task,
        profile,
        protected,
        MediaContext { retention_root: Some(retention_root), cancelled: Some(cancelled) },
    )
}

fn discover_explicit_with_context(
    root: &Path,
    task: &str,
    profile: &ProviderProfile,
    protected: &[PathBuf],
    context: MediaContext<'_>,
) -> Result<WorkspaceImages, ProductRunnerError> {
    let paths = explicit_paths(root, task, protected);
    attach(root, paths, profile, super::requests_visual_inspection(task), context)
}

pub(super) fn explicit_paths(root: &Path, task: &str, protected: &[PathBuf]) -> Vec<PathBuf> {
    task
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
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_visual_explicit_image_file_work_does_not_require_image_input() {
        let root = tempfile::tempdir().expect("folder");
        std::fs::write(root.path().join("photo.jpg"), b"image bytes to checksum").expect("file");
        let task = "Compute the SHA-256 checksum of photo.jpg with the workspace tools.";

        let images =
            discover_explicit(root.path(), task, &super::super::tests::profile(false), &[])
                .expect("file work needs no vision");
        let (prompt, attachments) = images.into_parts(task.to_owned());

        assert!(prompt.starts_with(task));
        assert!(prompt.contains("cannot inspect image pixels"));
        assert!(prompt.contains("scoped file and command tools"));
        assert!(prompt.contains("photo.jpg"));
        assert!(attachments.is_empty());
        assert_eq!(
            std::fs::read(root.path().join("photo.jpg")).unwrap(),
            b"image bytes to checksum"
        );
    }

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
