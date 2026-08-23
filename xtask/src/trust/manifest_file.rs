use crate::error::Diagnostic;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn read_toml<T: DeserializeOwned>(
    root: &Path,
    relative: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<T> {
    read_toml_with_bytes(root, relative, diagnostics).map(|(document, _)| document)
}

pub(super) fn read_toml_with_bytes<T: DeserializeOwned>(
    root: &Path,
    relative: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(T, Vec<u8>)> {
    let bytes = read_regular(root, relative, diagnostics)?;
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) => {
            parse_error(relative, "TOML", &error.to_string(), diagnostics);
            return None;
        }
    };
    match toml::from_str(text) {
        Ok(document) => Some((document, bytes)),
        Err(error) => {
            parse_error(relative, "TOML", &error.to_string(), diagnostics);
            None
        }
    }
}

pub(super) fn read_json<T: DeserializeOwned>(
    root: &Path,
    relative: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(T, Vec<u8>)> {
    let bytes = read_regular(root, relative, diagnostics)?;
    match serde_json::from_slice(&bytes) {
        Ok(document) => Some((document, bytes)),
        Err(error) => {
            parse_error(relative, "JSON", &error.to_string(), diagnostics);
            None
        }
    }
}

pub(super) fn read_bytes(
    root: &Path,
    relative: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<u8>> {
    read_regular(root, relative, diagnostics)
}

pub(super) fn is_regular_without_symlink(root: &Path, relative: &Path) -> bool {
    if !repository_relative(relative) {
        return false;
    }
    let mut current = PathBuf::from(root);
    for component in relative.components() {
        current.push(component);
        let Ok(metadata) = current.symlink_metadata() else { return false };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    current.symlink_metadata().is_ok_and(|metadata| metadata.file_type().is_file())
}

pub(super) fn repository_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn read_regular(
    root: &Path,
    relative: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<u8>> {
    read_regular_with(root, relative, diagnostics, |path| fs::read(path))
}

fn read_regular_with(
    root: &Path,
    relative: &Path,
    diagnostics: &mut Vec<Diagnostic>,
    read: impl FnOnce(&Path) -> std::io::Result<Vec<u8>>,
) -> Option<Vec<u8>> {
    if !is_regular_without_symlink(root, relative) {
        diagnostics.push(Diagnostic::at(
            relative,
            "verification policy is missing, non-regular, or reached through a symlink",
            "restore the exact checked-in policy as a readable regular file",
        ));
        return None;
    }
    let path = root.join(relative);
    match read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("verification policy cannot be read: {error}"),
                "restore readable repository-owned bytes; do not bypass the policy",
            ));
            None
        }
    }
}

fn parse_error(path: &Path, format: &str, detail: &str, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.push(Diagnostic::at(
        path,
        format!("verification policy does not match its {format} schema: {detail}"),
        "correct the policy document; unknown or malformed fields fail closed",
    ));
}

#[cfg(test)]
mod tests {
    use super::{read_regular_with, read_toml, repository_relative};
    use serde::Deserialize;
    use std::env;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let root =
                env::temp_dir().join(format!("peritus-manifest-file-{}-{id}", process::id()));
            fs::create_dir_all(&root).expect("fixture root must be created");
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _cleanup_result = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Deserialize)]
    struct Document {
        value: u64,
    }

    #[test]
    fn loader_reports_absent_directory_and_simulated_unreadable_policy() {
        let fixture = Fixture::new();
        let relative = Path::new("verification/policy.toml");
        fs::create_dir_all(fixture.0.join("verification"))
            .expect("verification directory must be created");
        let mut diagnostics = Vec::new();
        assert!(read_toml::<Document>(&fixture.0, relative, &mut diagnostics).is_none());
        assert!(diagnostics.iter().any(|item| item.message().contains("missing, non-regular")));

        fs::create_dir(fixture.0.join(relative)).expect("directory-valued policy must be created");
        diagnostics.clear();
        assert!(read_toml::<Document>(&fixture.0, relative, &mut diagnostics).is_none());
        assert!(diagnostics.iter().any(|item| item.message().contains("missing, non-regular")));

        fs::remove_dir(fixture.0.join(relative)).expect("directory-valued policy must be removed");
        fs::write(fixture.0.join(relative), b"value = 1\n")
            .expect("regular policy must be written");
        diagnostics.clear();
        let bytes = read_regular_with(&fixture.0, relative, &mut diagnostics, |_| {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "simulated unreadable policy"))
        });
        assert!(bytes.is_none());
        assert!(diagnostics.iter().any(|item| item.message().contains("cannot be read")));
    }

    #[cfg(unix)]
    #[test]
    fn loader_rejects_readable_and_broken_symlinks() {
        use std::os::unix::fs::symlink;

        for target in [Some("../real.toml"), None] {
            let fixture = Fixture::new();
            fs::create_dir_all(fixture.0.join("verification"))
                .expect("verification directory must be created");
            if target.is_some() {
                fs::write(fixture.0.join("real.toml"), b"value = 1\n")
                    .expect("symlink target must be written");
            }
            symlink(
                target.unwrap_or("../missing.toml"),
                fixture.0.join("verification/policy.toml"),
            )
            .expect("policy symlink must be created");
            let mut diagnostics = Vec::new();
            assert!(
                read_toml::<Document>(
                    &fixture.0,
                    Path::new("verification/policy.toml"),
                    &mut diagnostics,
                )
                .is_none()
            );
            assert!(diagnostics.iter().any(|item| item.message().contains("through a symlink")));
        }
    }

    #[test]
    fn regular_toml_still_loads() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("verification"))
            .expect("verification directory must be created");
        fs::write(fixture.0.join("verification/policy.toml"), b"value = 7\n")
            .expect("regular policy must be written");
        let mut diagnostics = Vec::new();
        let document = read_toml::<Document>(
            &fixture.0,
            Path::new("verification/policy.toml"),
            &mut diagnostics,
        )
        .expect("regular policy must parse");
        assert_eq!(document.value, 7);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn repository_paths_reject_absolute_parent_and_current_components() {
        assert!(repository_relative(Path::new("verification/policy.toml")));
        for path in
            ["", "/verification/policy.toml", "../policy.toml", "a/../policy.toml", "./policy.toml"]
        {
            assert!(!repository_relative(Path::new(path)), "accepted `{path}`");
        }
    }
}
