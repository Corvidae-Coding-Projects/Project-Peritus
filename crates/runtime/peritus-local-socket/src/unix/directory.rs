//! Directory-relative addressing for Linux.
//!
//! Linux has no extended `sockaddr_un` form, but `/proc/self/fd/<fd>/<name>` names a file inside
//! an open directory descriptor. Binding or connecting through that address keeps the address
//! short however long the real path is, while the socket file itself is created at, and visible
//! through, the real path.

use std::{
    fs::{File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        unix::{
            fs::OpenOptionsExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
};

/// `PATH_MAX` less the terminating NUL: the real path must still be a valid filesystem path.
pub(super) const MAX_PATH_BYTES: usize = 4095;

pub(super) fn bind(path: &Path) -> io::Result<UnixListener> {
    through_directory(path, UnixListener::bind)
}

pub(super) fn connect(path: &Path) -> io::Result<UnixStream> {
    through_directory(path, UnixStream::connect)
}

fn through_directory<T>(
    path: &Path,
    operation: impl FnOnce(&Path) -> io::Result<T>,
) -> io::Result<T> {
    if path.as_os_str().len() > MAX_PATH_BYTES {
        return Err(super::too_long(path, MAX_PATH_BYTES));
    }
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "socket path has no file name")
    })?;
    let parent =
        path.parent().filter(|parent| !parent.as_os_str().is_empty()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "socket path has no directory")
        })?;
    let directory: File = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(parent)?;
    let short = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd())).join(name);
    if !super::fits_standard(&short) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Unix socket file name is too long for a directory-relative address: {}",
                path.display()
            ),
        ));
    }
    operation(&short)
}
