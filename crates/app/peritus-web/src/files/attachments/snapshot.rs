//! Owned temporary snapshots are published only after full source verification.

use crate::error::{Result, problem};
use std::path::PathBuf;
use tempfile::NamedTempFile;

pub(super) async fn temporary(directory: PathBuf) -> Result<NamedTempFile> {
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&directory)?;
        NamedTempFile::new_in(directory).map_err(Into::into)
    }).await.map_err(problem)?
}

pub(super) async fn publish(staging: NamedTempFile, destination: PathBuf) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let directory = destination.parent()
            .ok_or_else(|| problem("The attachment snapshot has no storage directory"))?
            .to_owned();
        let published = staging.persist_noclobber(destination).map_err(problem)?;
        published.sync_all()?;
        #[cfg(unix)]
        for path in std::iter::once(directory.as_path())
            .chain(directory.parent())
            .chain(directory.parent().and_then(std::path::Path::parent))
        {
            std::fs::File::open(path)?.sync_all()?;
        }
        Ok(())
    }).await.map_err(problem)?
}
