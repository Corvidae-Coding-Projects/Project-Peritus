//! Nonblocking bounded listener acceptance.

use std::net::{TcpListener, TcpStream};

use crate::NetworkError;

pub(super) fn next(listener: &TcpListener) -> Result<Option<TcpStream>, NetworkError> {
    match listener.accept() {
        Ok((stream, _peer)) => Ok(Some(stream)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(_) => Err(super::owner::proxy_error("managed proxy accept failed")),
    }
}
