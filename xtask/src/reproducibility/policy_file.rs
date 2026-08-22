use crate::error::Diagnostic;
use std::fs;
use std::path::Path;

pub(super) fn read_regular(
    root: &Path,
    relative: &Path,
    shape_message: &str,
    read_description: &str,
    help: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    read_regular_with(root, relative, shape_message, read_description, help, diagnostics, |path| {
        fs::read_to_string(path)
    })
}

fn read_regular_with(
    root: &Path,
    relative: &Path,
    shape_message: &str,
    read_description: &str,
    help: &str,
    diagnostics: &mut Vec<Diagnostic>,
    reader: impl FnOnce(&Path) -> std::io::Result<String>,
) -> Option<String> {
    let path = root.join(relative);
    if !is_regular_without_symlinks(root, relative) {
        diagnostics.push(Diagnostic::at(relative, shape_message, help));
        return None;
    }
    match reader(&path) {
        Ok(contents) => Some(contents),
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("{read_description} cannot be read: {error}"),
                help,
            ));
            None
        }
    }
}

fn is_regular_without_symlinks(root: &Path, relative: &Path) -> bool {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let Ok(metadata) = current.symlink_metadata() else { return false };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    current.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_file())
}

#[cfg(test)]
mod tests {
    use super::{read_regular, read_regular_with};
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const RELATIVE: &str = "policy/reviewed.toml";
    const SHAPE: &str = "reviewed policy is missing, non-regular, or symbolic";
    const HELP: &str = "restore the reviewed regular policy file";

    #[test]
    fn missing_and_directory_policy_inputs_are_actionable_diagnostics() {
        let missing = Fixture::new();
        assert_shape_failure(&missing);

        let directory = Fixture::new();
        fs::create_dir_all(directory.root.join(RELATIVE))
            .expect("policy-shaped directory must be creatable");
        assert_shape_failure(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn readable_and_broken_policy_symlinks_are_actionable_diagnostics() {
        use std::os::unix::fs::symlink;

        let readable = Fixture::new();
        readable.write("alternate.toml", "reviewed");
        readable.create_parent();
        symlink(readable.root.join("alternate.toml"), readable.root.join(RELATIVE))
            .expect("readable policy symlink must be creatable");
        assert_shape_failure(&readable);

        let broken = Fixture::new();
        broken.create_parent();
        symlink(broken.root.join("absent.toml"), broken.root.join(RELATIVE))
            .expect("broken policy symlink must be creatable");
        assert_shape_failure(&broken);
    }

    #[test]
    fn unreadable_regular_policy_is_an_actionable_diagnostic() {
        let fixture = Fixture::new();
        fixture.write(RELATIVE, "reviewed");
        let mut diagnostics = Vec::new();

        let contents = read_regular_with(
            &fixture.root,
            Path::new(RELATIVE),
            SHAPE,
            "reviewed policy",
            HELP,
            &mut diagnostics,
            |_| Err(io::Error::new(io::ErrorKind::PermissionDenied, "fixture denied")),
        );

        assert!(contents.is_none());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message().contains("cannot be read"));
        assert!(diagnostics[0].message().contains("fixture denied"));
        assert_eq!(diagnostics[0].help(), HELP);
    }

    fn assert_shape_failure(fixture: &Fixture) {
        let mut diagnostics = Vec::new();
        let contents = read_regular(
            &fixture.root,
            Path::new(RELATIVE),
            SHAPE,
            "reviewed policy",
            HELP,
            &mut diagnostics,
        );
        assert!(contents.is_none());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message(), SHAPE);
        assert_eq!(diagnostics[0].help(), HELP);
    }

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "peritus-policy-file-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).expect("fixture root must be creatable");
            Self { root }
        }

        fn create_parent(&self) {
            fs::create_dir_all(
                self.root.join(RELATIVE).parent().expect("policy path must have a parent"),
            )
            .expect("policy directory must be creatable");
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().expect("fixture path must have a parent"))
                .expect("fixture directory must be creatable");
            fs::write(path, contents).expect("fixture file must be writable");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("fixture root must be removable");
        }
    }
}
