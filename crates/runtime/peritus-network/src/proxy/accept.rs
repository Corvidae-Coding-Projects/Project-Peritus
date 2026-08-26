//! Nonblocking bounded listener acceptance.

use std::net::{TcpListener, TcpStream};

use crate::NetworkError;

pub(super) fn next(listener: &TcpListener) -> Result<Option<TcpStream>, NetworkError> {
    match listener.accept() {
        Ok((stream, _peer)) => {
            // Accepted sockets do not inherit listener status flags consistently across
            // platforms. Workers use bounded blocking I/O, so normalize that contract here.
            stream.set_nonblocking(false).map_err(|_| {
                super::owner::proxy_error("managed proxy client cannot be blocking")
            })?;
            Ok(Some(stream))
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(_) => Err(super::owner::proxy_error("managed proxy accept failed")),
    }
}
