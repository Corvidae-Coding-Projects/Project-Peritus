//! Exact, private and no-clobber publication of retained native generations.

use crate::error::{Result, problem, uncertain};
use std::{io::Write, path::Path};

pub(super) fn publish_exact(path: &Path, bytes: &[u8]) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || std::fs::read(path)? != bytes {
                return Err(problem(
                    "A retained native target path already owns different content",
                ));
            }
            restrict_file(path)?;
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = path
        .parent()
        .ok_or_else(|| problem("Retained native target has no parent directory"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    restrict_file(temporary.path())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(file) => file.sync_all().map_err(Into::into),
        Err(error) => {
            if std::fs::symlink_metadata(path)
                .is_ok_and(|metadata| metadata.file_type().is_file())
                && std::fs::read(path).is_ok_and(|retained| retained == bytes)
            {
                restrict_file(path)
            } else {
                Err(problem(error.error))
            }
        }
    }
}

pub(super) fn secure_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    if !std::fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(problem(
            "Retained native target directory is not a directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn restrict_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(uncertain)?;
    let _ = path;
    Ok(())
}
