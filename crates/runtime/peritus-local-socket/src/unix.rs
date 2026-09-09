//! Dispatch between standard and long-path Unix socket addressing.

use std::{
    io,
    os::unix::{
        ffi::OsStrExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
};

#[cfg(target_os = "linux")]
mod directory;
#[cfg(target_os = "macos")]
mod extended;
#[cfg(test)]
mod tests;

#[cfg(target_os = "linux")]
use directory as long;
#[cfg(target_os = "macos")]
use extended as long;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod long {
    //! Platforms without a long-path form refuse paths the standard structure cannot carry.

    use std::{
        io,
        os::unix::net::{UnixListener, UnixStream},
        path::Path,
    };

    pub(super) const MAX_PATH_BYTES: usize = super::STANDARD_PATH_BYTES;

    pub(super) fn bind(path: &Path) -> io::Result<UnixListener> {
        Err(super::too_long(path, MAX_PATH_BYTES))
    }

    pub(super) fn connect(path: &Path) -> io::Result<UnixStream> {
        Err(super::too_long(path, MAX_PATH_BYTES))
    }
}

/// Longest path, in bytes, that the standard `sockaddr_un` structure carries with its NUL.
const STANDARD_PATH_BYTES: usize =
    size_of::<libc::sockaddr_un>() - std::mem::offset_of!(libc::sockaddr_un, sun_path) - 1;

/// Longest socket path, in bytes, that this platform accepts.
pub const MAX_PATH_BYTES: usize = long::MAX_PATH_BYTES;

/// Binds a listening socket at `path`, however long the path is.
///
/// The socket is created at the real path with the process umask applied; callers that need a
/// particular mode restrict it afterwards exactly as they would for a standard bind.
///
/// # Errors
///
/// Returns the operating-system bind error, or an invalid-input error naming the path and this
/// platform's limit when the path is too long even for the long-path form.
pub fn bind(path: &Path) -> io::Result<UnixListener> {
    if fits_standard(path) {
        return UnixListener::bind(path);
    }
    long::bind(path)
}

/// Connects to the listening socket at `path`, however long the path is.
///
/// # Errors
///
/// Returns the operating-system connect error, or an invalid-input error naming the path and this
/// platform's limit when the path is too long even for the long-path form.
pub fn connect(path: &Path) -> io::Result<UnixStream> {
    if fits_standard(path) {
        return UnixStream::connect(path);
    }
    long::connect(path)
}

fn fits_standard(path: &Path) -> bool {
    path.as_os_str().as_bytes().len() <= STANDARD_PATH_BYTES
}

fn too_long(path: &Path, limit: usize) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "Unix socket path is {} bytes; this platform accepts at most {limit}: {}",
            path.as_os_str().as_bytes().len(),
            path.display()
        ),
    )
}
