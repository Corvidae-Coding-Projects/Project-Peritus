//! Consistent `SQLite` backup, digest verification, synchronization, and restoration.

use std::{fs, fs::OpenOptions, io::Read, path::Path};

use peritus_types::Sha256Digest;
use rusqlite::{Connection, MAIN_DB};
use sha2::{Digest, Sha256};

use crate::{MigrationError, MigrationErrorCode, RecoveryClass};

pub fn create(connection: &Connection, final_path: &Path) -> Result<Sha256Digest, MigrationError> {
    remove_abandoned_file(final_path, "remove unacknowledged backup")?;
    let temporary = final_path.with_extension("sqlite3.partial");
    remove_abandoned_file(&temporary, "remove abandoned backup temporary")?;
    let temporary_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| MigrationError::io("create exclusive backup temporary file", error))?;
    drop(temporary_file);
    if let Err(error) = connection.backup(MAIN_DB, &temporary, None) {
        let _ = fs::remove_file(&temporary);
        return Err(MigrationError::backup(error));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| MigrationError::io("open completed backup", error))?;
    file.sync_all().map_err(|error| MigrationError::io("synchronize backup file", error))?;
    let digest = hash_file(&temporary)?;
    fs::rename(&temporary, final_path)
        .map_err(|error| MigrationError::io("publish backup", error))?;
    sync_directory(final_path.parent().ok_or_else(invalid_path)?)?;
    Ok(digest)
}

fn remove_abandoned_file(path: &Path, operation: &'static str) -> Result<(), MigrationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            fs::remove_file(path).map_err(|error| MigrationError::io(operation, error))
        }
        Ok(_) => Err(MigrationError::message(
            MigrationErrorCode::BackupFailed,
            RecoveryClass::Terminal,
            operation,
            "derived backup path is not a regular file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(MigrationError::io(operation, error)),
    }
}

pub fn verify(path: &Path, expected: Sha256Digest) -> Result<(), MigrationError> {
    if hash_file(path)? != expected {
        return Err(MigrationError::message(
            MigrationErrorCode::BackupFailed,
            RecoveryClass::Terminal,
            "verify migration backup",
            "backup digest does not match durable recovery metadata",
        ));
    }
    Ok(())
}

pub fn restore(connection: &mut Connection, path: &Path) -> Result<(), MigrationError> {
    connection
        .restore(MAIN_DB, path, None::<fn(rusqlite::backup::Progress)>)
        .map_err(MigrationError::restore)
}

fn hash_file(path: &Path) -> Result<Sha256Digest, MigrationError> {
    let mut file =
        fs::File::open(path).map_err(|error| MigrationError::io("open migration backup", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| MigrationError::io("hash migration backup", error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(Sha256Digest::new(hasher.finalize().into()))
}

fn sync_directory(path: &Path) -> Result<(), MigrationError> {
    #[cfg(unix)]
    {
        fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| MigrationError::io("synchronize backup directory", error))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

const fn invalid_path() -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::InvalidConfiguration,
        RecoveryClass::CorrectRequest,
        "derive backup path",
        "backup path has no parent directory",
    )
}
