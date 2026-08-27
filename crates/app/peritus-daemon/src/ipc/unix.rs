//! Protected Unix-domain endpoint and peer-credential admission.

use std::{
    fs, io,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use peritus_journal::ApplicationPrincipalKind;
use tokio::net::UnixListener;

use super::{AuthenticatedConnection, LocalEndpointAddress, PeerIdentity};
use crate::{DaemonError, DaemonErrorCode, DaemonIdentity, DaemonRecovery};

const ROOT_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;

pub(super) fn recover_stale(
    state_root: &Path,
    identity: &DaemonIdentity,
) -> Result<(), DaemonError> {
    let root = inspect_state_root(state_root)?;
    let path = state_root.join(format!("{}.sock", identity.endpoint_name()));
    match fs::symlink_metadata(&path) {
        Ok(metadata)
            if metadata.file_type().is_socket()
                && metadata.uid() == root.uid()
                && metadata.mode() & 0o777 == SOCKET_MODE =>
        {
            fs::remove_file(&path).map_err(|error| {
                DaemonError::with_source(
                    DaemonErrorCode::Transport,
                    DaemonRecovery::Retry,
                    "recover stale Unix endpoint",
                    "stale owned Unix endpoint could not be removed",
                    error,
                )
            })
        }
        Ok(_) => Err(security_error(
            "recover stale Unix endpoint",
            "endpoint path is not an owned mode-0600 Unix socket",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DaemonError::with_source(
            DaemonErrorCode::Storage,
            DaemonRecovery::Operator,
            "recover stale Unix endpoint",
            "stale Unix endpoint could not be inspected",
            error,
        )),
    }
}

/// One Unix listener anchored to the exact socket object created at bind time.
pub(super) struct PlatformEndpoint {
    listener: UnixListener,
    path: PathBuf,
    device: u64,
    inode: u64,
    owner_uid: u32,
}

impl PlatformEndpoint {
    pub(super) fn owner_peer(&self) -> PeerIdentity {
        PeerIdentity::from_os_identity(
            ApplicationPrincipalKind::UnixPeer,
            &self.owner_uid.to_be_bytes(),
        )
    }

    pub(super) async fn bind(
        state_root: &Path,
        identity: &DaemonIdentity,
    ) -> Result<(Self, LocalEndpointAddress), DaemonError> {
        let root = inspect_state_root(state_root)?;
        let owner_uid = root.uid();
        let path = state_root.join(format!("{}.sock", identity.endpoint_name()));
        refuse_existing_endpoint(&path)?;

        let listener = UnixListener::bind(&path).map_err(bind_error)?;
        let created = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_socket() && metadata.uid() == owner_uid => {
                metadata
            }
            Ok(_) => {
                drop(listener);
                return Err(security_error(
                    "reopen Unix endpoint identity",
                    "new Unix endpoint is not owned by the protected state-root owner",
                ));
            }
            Err(error) => {
                drop(listener);
                return Err(DaemonError::with_source(
                    DaemonErrorCode::Transport,
                    DaemonRecovery::Operator,
                    "reopen Unix endpoint identity",
                    "new Unix endpoint identity could not be inspected",
                    error,
                ));
            }
        };
        let device = created.dev();
        let inode = created.ino();
        if let Err(error) = fs::set_permissions(&path, fs::Permissions::from_mode(SOCKET_MODE)) {
            drop(listener);
            remove_owned_socket(&path, device, inode);
            return Err(DaemonError::with_source(
                DaemonErrorCode::Transport,
                DaemonRecovery::Operator,
                "protect Unix endpoint",
                "new Unix endpoint permissions could not be restricted",
                error,
            ));
        }

        let socket = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                drop(listener);
                return Err(DaemonError::with_source(
                    DaemonErrorCode::Transport,
                    DaemonRecovery::Operator,
                    "reopen Unix endpoint identity",
                    "new Unix endpoint identity could not be inspected",
                    error,
                ));
            }
        };
        if !socket.file_type().is_socket()
            || socket.uid() != owner_uid
            || socket.mode() & 0o777 != SOCKET_MODE
            || socket.dev() != device
            || socket.ino() != inode
        {
            drop(listener);
            remove_owned_socket(&path, device, inode);
            return Err(security_error(
                "validate Unix endpoint identity",
                "new Unix endpoint is not a user-owned mode-0600 socket",
            ));
        }

        let endpoint = Self { listener, path: path.clone(), device, inode, owner_uid };
        Ok((endpoint, LocalEndpointAddress::Unix(path)))
    }

    pub(super) async fn accept(&self) -> Result<AuthenticatedConnection, DaemonError> {
        let (stream, _) = self.listener.accept().await.map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::Transport,
                DaemonRecovery::Retry,
                "accept Unix connection",
                "Unix endpoint could not accept a local connection",
                error,
            )
        })?;
        let credentials = stream.peer_cred().map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::Transport,
                DaemonRecovery::Retry,
                "authenticate Unix peer",
                "accepted Unix peer credentials are unavailable",
                error,
            )
        })?;
        let uid = credentials.uid();
        if uid != self.owner_uid {
            return Err(security_error(
                "authenticate Unix peer",
                "accepted Unix peer does not own the protected state root",
            ));
        }

        // Big-endian u32 is the canonical cross-platform byte representation of a Unix UID.
        let peer = self.owner_peer();
        Ok(AuthenticatedConnection::new(Box::new(stream), peer))
    }
}

impl Drop for PlatformEndpoint {
    fn drop(&mut self) {
        remove_owned_socket(&self.path, self.device, self.inode);
    }
}

fn inspect_state_root(state_root: &Path) -> Result<fs::Metadata, DaemonError> {
    let metadata = fs::symlink_metadata(state_root).map_err(|error| {
        DaemonError::with_source(
            DaemonErrorCode::Storage,
            DaemonRecovery::Operator,
            "inspect Unix state root",
            "protected state root cannot be inspected",
            error,
        )
    })?;
    if !metadata.file_type().is_dir() || metadata.mode() & 0o777 != ROOT_MODE {
        return Err(security_error(
            "validate Unix state root",
            "state root must be a real mode-0700 directory",
        ));
    }
    Ok(metadata)
}

fn refuse_existing_endpoint(path: &Path) -> Result<(), DaemonError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => Err(DaemonError::new(
            DaemonErrorCode::AlreadyRunning,
            DaemonRecovery::Reconcile,
            "bind Unix endpoint",
            "Unix endpoint already exists and was not removed speculatively",
        )),
        Ok(_) => Err(DaemonError::new(
            DaemonErrorCode::CorruptState,
            DaemonRecovery::Operator,
            "bind Unix endpoint",
            "Unix endpoint path is occupied by a symlink or non-socket object",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DaemonError::with_source(
            DaemonErrorCode::Storage,
            DaemonRecovery::Operator,
            "inspect Unix endpoint",
            "Unix endpoint path cannot be inspected",
            error,
        )),
    }
}

fn bind_error(error: io::Error) -> DaemonError {
    if error.kind() == io::ErrorKind::AddrInUse {
        DaemonError::with_source(
            DaemonErrorCode::AlreadyRunning,
            DaemonRecovery::Reconcile,
            "bind Unix endpoint",
            "Unix endpoint became occupied before bind completed",
            error,
        )
    } else {
        DaemonError::with_source(
            DaemonErrorCode::Transport,
            DaemonRecovery::Operator,
            "bind Unix endpoint",
            "Unix endpoint could not be created",
            error,
        )
    }
}

fn remove_owned_socket(path: &Path, device: u64, inode: u64) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_socket() && metadata.dev() == device && metadata.ino() == inode {
        let _ = fs::remove_file(path);
    }
}

fn security_error(operation: &'static str, detail: &'static str) -> DaemonError {
    DaemonError::new(DaemonErrorCode::Unauthorized, DaemonRecovery::Operator, operation, detail)
}
