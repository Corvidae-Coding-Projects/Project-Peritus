//! Bounded, explicit CLI operation receipts.

use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::{
    error::CliError,
    id::{generated_id, hex},
};

// Receipts contain only fixed-size identities, fingerprints, and scalar frontiers. This bound
// protects local parsing without limiting the operation history or response represented by them.
const MAX_FIXED_RECEIPT_BYTES: usize = 64 * 1024;

pub(crate) struct ReceiptLease {
    file: std::fs::File,
}

impl Drop for ReceiptLease {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}

pub(crate) async fn lease(path: Option<&Path>) -> Result<Option<ReceiptLease>, CliError> {
    let Some(path) = path else {
        return Ok(None);
    };
    let lock_path = sidecar_path(path, ".lock");
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).read(true).write(true);
    let file = options
        .open(&lock_path)
        .await
        .map_err(|error| {
            CliError::local_io(
                "open operation receipt lease",
                Some(lock_path.clone()),
                error,
            )
        })?
        .into_std()
        .await;
    match fs4::FileExt::try_lock(&file) {
        Ok(()) => Ok(Some(ReceiptLease { file })),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Err(CliError::runtime(
            "acquire operation receipt lease",
            format!("receipt is already active: {}", path.display()),
        )),
        Err(error) => Err(CliError::local_io(
            "acquire operation receipt lease",
            Some(lock_path),
            error,
        )),
    }
}

pub(crate) async fn load<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, CliError> {
    let mut file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CliError::local_io(
                "open operation receipt",
                Some(path.to_path_buf()),
                error,
            ));
        }
    };
    let metadata = file.metadata().await.map_err(|error| {
        CliError::local_io("inspect operation receipt", Some(path.to_path_buf()), error)
    })?;
    if metadata.len() > MAX_FIXED_RECEIPT_BYTES as u64 {
        return Err(CliError::usage(format!(
            "fixed operation receipt exceeds {MAX_FIXED_RECEIPT_BYTES} bytes: {}",
            path.display(),
        )));
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(MAX_FIXED_RECEIPT_BYTES)
            .min(MAX_FIXED_RECEIPT_BYTES),
    );
    file.take((MAX_FIXED_RECEIPT_BYTES as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| {
            CliError::local_io("read operation receipt", Some(path.to_path_buf()), error)
        })?;
    if bytes.len() > MAX_FIXED_RECEIPT_BYTES {
        return Err(CliError::usage(format!(
            "fixed operation receipt exceeds {MAX_FIXED_RECEIPT_BYTES} bytes: {}",
            path.display(),
        )));
    }
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        CliError::runtime(
            "decode operation receipt",
            format!("{}: {error}", path.display()),
        )
    })
}

pub(crate) async fn create<T: Serialize>(path: &Path, value: &T) -> Result<(), CliError> {
    let bytes = encode(path, value)?;
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path).await.map_err(|error| {
        CliError::local_io("create operation receipt", Some(path.to_path_buf()), error)
    })?;
    let result = async {
        file.write_all(&bytes).await.map_err(|error| {
            CliError::local_io("write operation receipt", Some(path.to_path_buf()), error)
        })?;
        file.sync_all().await.map_err(|error| {
            CliError::local_io("sync operation receipt", Some(path.to_path_buf()), error)
        })?;
        drop(file);
        sync_parent(path).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(path).await;
    }
    result
}

pub(crate) async fn replace<T: Serialize>(path: &Path, value: &T) -> Result<(), CliError> {
    let bytes = encode(path, value)?;
    let temporary = temporary_path(path);
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(&temporary).await.map_err(|error| {
        CliError::local_io("create receipt checkpoint", Some(temporary.clone()), error)
    })?;
    let result = async {
        file.write_all(&bytes).await.map_err(|error| {
            CliError::local_io("write receipt checkpoint", Some(temporary.clone()), error)
        })?;
        file.sync_all().await.map_err(|error| {
            CliError::local_io("sync receipt checkpoint", Some(temporary.clone()), error)
        })?;
        drop(file);
        replace_path(&temporary, path).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

pub(crate) async fn publish_replace(candidate: &Path, destination: &Path) -> Result<(), CliError> {
    replace_file(candidate, destination)
        .await
        .map_err(|error| CliError::local_io("replace artifact output", Some(destination.to_path_buf()), error))
}

pub(crate) async fn publish_new(candidate: &Path, destination: &Path) -> Result<(), CliError> {
    publish_new_file(candidate, destination)
        .await
        .map_err(|error| CliError::local_io("publish artifact output", Some(destination.to_path_buf()), error))
}

pub(crate) async fn sync_directory(path: &Path) -> Result<(), CliError> {
    sync_parent(path).await
}

pub(crate) async fn remove_durable(path: &Path) -> Result<(), CliError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => sync_parent(path).await,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CliError::local_io(
            "remove published artifact temporary",
            Some(path.to_path_buf()),
            error,
        )),
    }
}

fn encode<T: Serialize>(path: &Path, value: &T) -> Result<Vec<u8>, CliError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        CliError::runtime(
            "encode operation receipt",
            format!("{}: {error}", path.display()),
        )
    })?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FIXED_RECEIPT_BYTES {
        return Err(CliError::runtime(
            "encode operation receipt",
            "fixed receipt schema exceeded its local parsing bound",
        ));
    }
    Ok(bytes)
}

fn temporary_path(path: &Path) -> PathBuf {
    sidecar_path(
        path,
        &format!(".{}.tmp", hex(&generated_id(b"receipt-checkpoint"))),
    )
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(not(windows))]
async fn replace_path(temporary: &Path, destination: &Path) -> Result<(), CliError> {
    replace_file(temporary, destination).await.map_err(|error| {
        CliError::local_io("publish receipt checkpoint", Some(destination.to_path_buf()), error)
    })
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "atomic receipt replacement uses the documented Windows move primitive"
)]
async fn replace_path(temporary: &Path, destination: &Path) -> Result<(), CliError> {
    replace_file(temporary, destination).await.map_err(|error| {
        CliError::local_io("publish receipt checkpoint", Some(destination.to_path_buf()), error)
    })
}

#[cfg(not(windows))]
async fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    tokio::fs::rename(temporary, destination).await?;
    sync_parent_io(destination).await
}

#[cfg(windows)]
async fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    atomic_move(temporary, destination, true)
}

#[cfg(not(windows))]
async fn publish_new_file(candidate: &Path, destination: &Path) -> std::io::Result<()> {
    tokio::fs::hard_link(candidate, destination).await?;
    sync_parent_io(destination).await?;
    tokio::fs::remove_file(candidate).await?;
    sync_parent_io(destination).await
}

#[cfg(windows)]
async fn publish_new_file(candidate: &Path, destination: &Path) -> std::io::Result<()> {
    atomic_move(candidate, destination, false)
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "atomic receipt and artifact publication use the documented Windows move primitive"
)]
fn atomic_move(temporary: &Path, destination: &Path, replace: bool) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both encoded paths are NUL-terminated and remain live for this synchronous call.
    let flags = MOVEFILE_WRITE_THROUGH | if replace { MOVEFILE_REPLACE_EXISTING } else { 0 };
    if unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            flags,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
async fn sync_parent(path: &Path) -> Result<(), CliError> {
    sync_parent_io(path).await.map_err(|error| {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        CliError::local_io("sync receipt directory", Some(parent.to_path_buf()), error)
    })
}

#[cfg(not(windows))]
async fn sync_parent_io(path: &Path) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let directory = tokio::fs::File::open(parent).await?;
    directory.sync_all().await
}

#[cfg(windows)]
async fn sync_parent(_path: &Path) -> Result<(), CliError> {
    Ok(())
}
