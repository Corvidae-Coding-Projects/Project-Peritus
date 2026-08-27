//! Authenticated local IPC and bounded A3 frame transport.

mod endpoint;
mod frame;
mod peer;
mod server;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use endpoint::{AuthenticatedConnection, LocalEndpoint, LocalEndpointAddress};
pub use frame::AppFrameStream;
pub use peer::PeerIdentity;
pub(crate) use server::serve;

use tokio::io::{AsyncRead, AsyncWrite};

pub(super) trait LocalIo: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T> LocalIo for T where T: AsyncRead + AsyncWrite + Send + Unpin {}
pub(super) type BoxedLocalIo = Box<dyn LocalIo>;
