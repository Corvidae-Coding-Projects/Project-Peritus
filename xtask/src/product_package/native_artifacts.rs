//! Exact native binary inventory and permissions after same-run artifact download.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{debug_binary, host_os};
use crate::XtaskError;

fn paths(root: &Path) -> Result<[PathBuf; 7], XtaskError> {
    let helper = match host_os() {
        "linux" => "peritus-linux-sandbox-helper",
        "macos" => "peritus-macos-sandbox-helper",
        "windows" => "peritus-windows-sandbox-helper",
        _ => return Err(XtaskError::metadata("native product packaging is unsupported here")),
    };
    Ok([
        "peritus",
        "peritusd",
        "peritus-tui",
        helper,
        "peritus-package",
        "peritus-h2",
        "peritus-h2-controller",
    ]
    .map(|name| debug_binary(root, name)))
}

/// Checks the full input inventory before restoring permissions lost by artifact download.
pub(super) fn restore(root: &Path) -> Result<(), XtaskError> {
    restore_paths(&paths(root)?)
}

/// Restores only the two qualification tools, without requiring debug product binaries.
pub(crate) fn restore_controllers(root: &Path) -> Result<(), XtaskError> {
    restore_paths(&[debug_binary(root, "peritus-h2"), debug_binary(root, "peritus-h2-controller")])
}

fn restore_paths(paths: &[PathBuf]) -> Result<(), XtaskError> {
    for path in paths {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            XtaskError::io("inspect required native build artifact at", path, error)
        })?;
        if !metadata.file_type().is_file() {
            return Err(XtaskError::metadata(format!(
                "required native build artifact is not a regular file: {}",
                path.display()
            )));
        }
    }
    #[cfg(unix)]
    for path in paths {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| {
            XtaskError::io("restore native build artifact permissions at", path, error)
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_package::SmokeSubject;

    fn fixture(root: &Path) -> [PathBuf; 7] {
        let paths = paths(root).expect("native paths");
        fs::create_dir_all(root.join("target/debug")).expect("binary directory");
        for path in &paths {
            fs::write(path, b"artifact presence fixture").expect("binary fixture");
        }
        paths
    }

    #[test]
    fn every_binary_is_required_and_directories_are_rejected() {
        let subject = SmokeSubject::new().expect("subject");
        let paths = fixture(subject.path());
        restore(subject.path()).expect("complete inventory");
        for path in &paths {
            fs::remove_file(path).expect("remove exact fixture");
            assert!(restore(subject.path()).is_err());
            fs::create_dir(path).expect("directory impostor");
            assert!(restore(subject.path()).is_err());
            fs::remove_dir(path).expect("remove empty impostor");
            fs::write(path, b"artifact presence fixture").expect("restore fixture");
        }
    }

    #[cfg(unix)]
    #[test]
    fn downloaded_permissions_are_restored_only_after_all_inputs_are_validated() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let subject = SmokeSubject::new().expect("subject");
        let paths = fixture(subject.path());
        for path in &paths {
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).expect("download mode");
        }
        let last = paths.last().expect("last artifact");
        fs::remove_file(last).expect("remove exact fixture");
        symlink(&paths[0], last).expect("symlink impostor");
        assert!(restore(subject.path()).is_err());
        assert_eq!(
            fs::metadata(&paths[0]).expect("first artifact").permissions().mode() & 0o777,
            0o644
        );
        fs::remove_file(last).expect("remove fixture symlink");
        fs::write(last, b"artifact presence fixture").expect("restore fixture");
        restore(subject.path()).expect("restore executable permissions");
        for path in &paths {
            assert_eq!(fs::metadata(path).expect("artifact").permissions().mode() & 0o777, 0o755);
        }
    }
}
