//! Extended `sockaddr_un` addressing for macOS.
//!
//! XNU reads `sun_len - offsetof(sockaddr_un, sun_path)` bytes of `sun_path` in `unp_bind` and
//! `unp_connect` (`bsd/kern/uipc_usrreq.c`), bounded by `SOCK_MAXADDRLEN` rather than by the
//! declared 104-byte array. Passing the real address length instead of `size_of::<sockaddr_un>()`
//! therefore lets a path of up to 252 bytes bind and connect.

use std::{
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{
            ffi::OsStrExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::Path,
};

/// `SOCK_MAXADDRLEN` (255) less the `sun_len` and `sun_family` bytes and the terminating NUL.
pub(super) const MAX_PATH_BYTES: usize = 252;
const HEADER_BYTES: usize = std::mem::offset_of!(libc::sockaddr_un, sun_path);
const STORAGE_BYTES: usize = 256;
const BACKLOG: libc::c_int = 128;

/// One complete `sockaddr_un` whose `sun_len` covers the whole path.
struct Address {
    storage: [u8; STORAGE_BYTES],
    length: libc::socklen_t,
}

impl Address {
    fn new(path: &Path) -> io::Result<Self> {
        let bytes = path.as_os_str().as_bytes();
        if bytes.len() > MAX_PATH_BYTES {
            return Err(super::too_long(path, MAX_PATH_BYTES));
        }
        if bytes.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unix socket path contains an interior NUL byte",
            ));
        }
        let length = HEADER_BYTES + bytes.len() + 1;
        let mut storage = [0_u8; STORAGE_BYTES];
        storage[0] = u8::try_from(length).map_err(|_| super::too_long(path, MAX_PATH_BYTES))?;
        storage[1] = u8::try_from(libc::AF_UNIX).expect("AF_UNIX is a small constant");
        storage[HEADER_BYTES..HEADER_BYTES + bytes.len()].copy_from_slice(bytes);
        let length = libc::socklen_t::try_from(length).expect("address length is below 256");
        Ok(Self { storage, length })
    }

    const fn as_sockaddr(&self) -> *const libc::sockaddr {
        self.storage.as_ptr().cast::<libc::sockaddr>()
    }
}

#[allow(
    unsafe_code,
    reason = "socket(2) and fcntl(2) take no pointers; the descriptor is owned at once"
)]
fn stream_socket() -> io::Result<OwnedFd> {
    // SAFETY: socket(2) takes only integer arguments.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a fresh descriptor that nothing else owns.
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    // SAFETY: fcntl(2) with F_SETFD takes only integer arguments on an owned descriptor.
    if unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(socket)
}

#[allow(
    unsafe_code,
    reason = "bind(2) and listen(2) on an owned descriptor with a complete, initialised address"
)]
pub(super) fn bind(path: &Path) -> io::Result<UnixListener> {
    let address = Address::new(path)?;
    let socket = stream_socket()?;
    // SAFETY: `address` outlives the call and its first `length` bytes are initialised.
    if unsafe { libc::bind(socket.as_raw_fd(), address.as_sockaddr(), address.length) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: listen(2) takes only integer arguments on an owned, bound descriptor.
    if unsafe { libc::listen(socket.as_raw_fd(), BACKLOG) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(UnixListener::from(socket))
}

#[allow(
    unsafe_code,
    reason = "connect(2) on an owned descriptor with a complete, initialised address"
)]
pub(super) fn connect(path: &Path) -> io::Result<UnixStream> {
    let address = Address::new(path)?;
    let socket = stream_socket()?;
    // SAFETY: `address` outlives the call and its first `length` bytes are initialised.
    if unsafe { libc::connect(socket.as_raw_fd(), address.as_sockaddr(), address.length) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(UnixStream::from(socket))
}
