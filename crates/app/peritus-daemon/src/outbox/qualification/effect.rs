//! Exact identity-bearing filesystem destination for outbox crash qualification.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::fs::File;

use peritus_journal::OutboxId;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

const DIRECTORY: &str = "outbox-crash-qualification-v1";

pub(super) struct QualificationDestination {
    root: PathBuf,
    effect: PathBuf,
}

impl QualificationDestination {
    pub(super) fn prepare(
        state_root: &Path,
        outbox_id: OutboxId,
        payload: &[u8],
    ) -> Result<Self, DaemonError> {
        validate_directory(state_root, "validate qualification state root")?;
        let root = state_root.join(DIRECTORY);
        create_private_directory(&root)?;
        let effect = root.join(effect_name(outbox_id));
        let destination = Self { root, effect };
        match fs::symlink_metadata(&destination.effect) {
            Ok(_) => destination.require_exact_effect(payload)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(filesystem_error("inspect qualification effect", error));
            }
        }
        Ok(destination)
    }

    pub(super) fn effect_path(&self) -> &Path {
        &self.effect
    }

    pub(super) fn apply_once(&self, payload: &[u8]) -> Result<(), DaemonError> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&self.effect) {
            Ok(mut file) => {
                file.write_all(payload)
                    .and_then(|()| file.sync_all())
                    .map_err(|error| filesystem_error("persist qualification effect", error))?;
                sync_directory(&self.root)?;
                Ok(())
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                self.require_exact_effect(payload)
            }
            Err(error) => Err(filesystem_error("create qualification effect", error)),
        }
    }

    pub(super) fn reconcile(&self, payload: &[u8]) -> Result<EffectObservation, DaemonError> {
        self.require_exact_effect(payload)?;
        let mut external_effects = 0_u64;
        let mut duplicate_effects = 0_u64;
        for entry in fs::read_dir(&self.root)
            .map_err(|error| filesystem_error("enumerate qualification effects", error))?
        {
            let entry = entry
                .map_err(|error| filesystem_error("inspect qualification effect entry", error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| filesystem_error("classify qualification effect entry", error))?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(corrupt("qualification effect directory contains a non-regular entry"));
            }
            if entry.path() == self.effect {
                external_effects = external_effects.checked_add(1).ok_or_else(|| {
                    corrupt("qualification external-effect count exceeded its numeric bound")
                })?;
            } else {
                duplicate_effects = duplicate_effects.checked_add(1).ok_or_else(|| {
                    corrupt("qualification duplicate-effect count exceeded its numeric bound")
                })?;
            }
        }
        Ok(EffectObservation { external_effects, duplicate_effects })
    }

    fn require_exact_effect(&self, payload: &[u8]) -> Result<(), DaemonError> {
        let metadata = fs::symlink_metadata(&self.effect)
            .map_err(|error| filesystem_error("inspect qualification effect", error))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(corrupt("qualification effect is not a regular identity-bearing file"));
        }
        let bytes = fs::read(&self.effect)
            .map_err(|error| filesystem_error("read qualification effect", error))?;
        if bytes != payload {
            return Err(corrupt("qualification effect identity differs from the C0 delivery"));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), DaemonError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| filesystem_error("persist qualification effect directory", error))
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), DaemonError> {
    // Windows does not provide a portable directory handle that `File::sync_all` accepts. The
    // effect file itself is already flushed above before the checkpoint is published.
    Ok(())
}

pub(super) struct EffectObservation {
    external_effects: u64,
    duplicate_effects: u64,
}

impl EffectObservation {
    pub(super) const fn external_effects(&self) -> u64 {
        self.external_effects
    }

    pub(super) const fn duplicate_effects(&self) -> u64 {
        self.duplicate_effects
    }
}

fn create_private_directory(path: &Path) -> Result<(), DaemonError> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(filesystem_error("create qualification effect directory", error)),
    }
    validate_directory(path, "validate qualification effect directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| filesystem_error("protect qualification effect directory", error))?;
    }
    Ok(())
}

fn validate_directory(path: &Path, operation: &'static str) -> Result<(), DaemonError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| filesystem_error(operation, error))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(DaemonError::new(
            DaemonErrorCode::InvalidInput,
            DaemonRecovery::CorrectRequest,
            operation,
            "qualification path is not a real directory",
        ));
    }
    let canonical = fs::canonicalize(path).map_err(|error| filesystem_error(operation, error))?;
    if canonical != path {
        return Err(DaemonError::new(
            DaemonErrorCode::InvalidInput,
            DaemonRecovery::CorrectRequest,
            operation,
            "qualification path contains an alias or symbolic-link component",
        ));
    }
    Ok(())
}

fn effect_name(outbox_id: OutboxId) -> String {
    let mut name = String::with_capacity(48);
    name.push_str("delivery-");
    for byte in outbox_id.as_bytes() {
        name.push(hex_digit(byte >> 4));
        name.push(hex_digit(byte & 0x0f));
    }
    name.push_str(".effect");
    name
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + value - 10) as char,
        _ => '?',
    }
}

fn filesystem_error(operation: &'static str, error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        operation,
        "outbox crash qualification filesystem operation failed",
        error,
    )
}

fn corrupt(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "reconcile qualification effect",
        detail,
    )
}
