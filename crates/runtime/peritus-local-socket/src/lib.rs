//! Bounded, owner-protected paths for ordinary Unix-domain sockets.
//!
//! Paths that fit the platform's standard socket address remain unchanged. Longer paths map to
//! a private, identity-derived directory beneath `/tmp`; durable state is never relocated.
//! All clients continue to use standard socket APIs, including Tokio's cancellable connector.

mod path;
#[cfg(unix)]
mod unix;

pub use path::{NATIVE_MAX_PATH_BYTES, bounded_path};
#[cfg(unix)]
pub use unix::PreparedSocketPath;

mod verified;
