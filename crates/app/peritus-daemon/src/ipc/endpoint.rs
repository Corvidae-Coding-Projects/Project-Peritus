//! Cross-platform local endpoint facade.

use std::{
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

#[cfg(unix)]
use std::path::PathBuf;

use super::{AppFrameStream, BoxedLocalIo, PeerIdentity};
use crate::{DaemonError, DaemonIdentity};
use peritus_app_protocol::AppProtocolLimits;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[cfg(unix)]
use super::unix as platform;
#[cfg(windows)]
use super::windows as platform;

/// Platform-specific stable local endpoint address.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalEndpointAddress {
    /// Protected Unix-domain socket path.
    #[cfg(unix)]
    Unix(PathBuf),
    /// Protected Windows named-pipe identity.
    #[cfg(windows)]
    Windows(String),
}

/// One accepted stream paired with authenticated OS peer identity.
pub struct AuthenticatedConnection {
    io: BoxedLocalIo,
    peer: PeerIdentity,
}

impl AuthenticatedConnection {
    pub(super) const fn new(io: BoxedLocalIo, peer: PeerIdentity) -> Self {
        Self { io, peer }
    }
    /// Returns the authenticated local peer identity.
    #[must_use]
    pub const fn peer(&self) -> PeerIdentity {
        self.peer
    }
    /// Consumes the accepted connection into bounded A3 framing.
    #[must_use]
    pub const fn into_framed(self, limits: AppProtocolLimits) -> AppFrameStream<Self> {
        AppFrameStream::new(self, limits)
    }
}

impl AsyncRead for AuthenticatedConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_read(context, buffer)
    }
}

impl AsyncWrite for AuthenticatedConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut self.io).poll_write(context, buffer)
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.io).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.io).poll_shutdown(context)
    }
}

/// Exclusive authenticated local listener.
pub struct LocalEndpoint {
    inner: platform::PlatformEndpoint,
    address: LocalEndpointAddress,
}

impl LocalEndpoint {
    /// Removes a stale platform endpoint only after the caller has acquired the instance lock.
    ///
    /// # Errors
    ///
    /// Returns a typed ownership or filesystem error when the endpoint path is not the prior
    /// daemon's recoverable object.
    #[cfg(unix)]
    pub(crate) fn recover_stale(
        state_root: &Path,
        identity: &DaemonIdentity,
    ) -> Result<(), DaemonError> {
        platform::recover_stale(state_root, identity)
    }

    #[cfg(windows)]
    pub(crate) const fn recover_stale(
        state_root: &Path,
        identity: &DaemonIdentity,
    ) -> Result<(), DaemonError> {
        platform::recover_stale(state_root, identity)
    }

    /// Binds the stable endpoint beneath the protected state root.
    ///
    /// The caller must already hold the daemon instance lock. Existing endpoint objects are not
    /// removed speculatively.
    ///
    /// # Errors
    ///
    /// Returns a typed transport, ownership, or existing-instance error.
    pub async fn bind(state_root: &Path, identity: &DaemonIdentity) -> Result<Self, DaemonError> {
        let (inner, address) = platform::PlatformEndpoint::bind(state_root, identity).await?;
        Ok(Self { inner, address })
    }

    /// Borrows the exact bound endpoint address.
    #[must_use]
    pub const fn address(&self) -> &LocalEndpointAddress {
        &self.address
    }

    /// Returns the authenticated principal identity required for local clients.
    #[must_use]
    pub fn owner_peer(&self) -> PeerIdentity {
        self.inner.owner_peer()
    }

    /// Accepts one connection only after platform peer authentication succeeds.
    ///
    /// # Errors
    ///
    /// Returns a typed transport or unauthorized-peer error.
    pub async fn accept(&self) -> Result<AuthenticatedConnection, DaemonError> {
        self.inner.accept().await
    }
}
