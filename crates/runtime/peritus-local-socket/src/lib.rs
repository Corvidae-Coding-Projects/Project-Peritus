//! Long-path-safe Unix-domain socket binding and connection for local Peritus endpoints.
//!
//! The standard `sockaddr_un` structure carries at most 104 path bytes on macOS and 108 on Linux,
//! and the standard library rejects longer paths before the kernel sees them. Peritus keeps its
//! daemon endpoint beneath a protected per-user state root whose length depends on the platform
//! layout and the account name, so the endpoint must not depend on that path being short.
//!
//! On macOS the kernel accepts a `sockaddr_un` whose `sun_len` covers up to 252 path bytes, and
//! this crate builds that address directly. On Linux it binds and connects through
//! `/proc/self/fd/<directory>/<name>`, which keeps the address short however long the real path
//! is. Paths that fit the standard structure use the standard library unchanged. In every case the
//! socket file is created at, and can be inspected through, its real path.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{MAX_PATH_BYTES, bind, connect};
