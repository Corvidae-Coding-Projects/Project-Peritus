//! Deterministic endpoint addressing, also usable by cross-platform qualification plans.

use sha2::{Digest as _, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
};

/// Maximum filesystem pathname bytes in the native standard socket address, excluding its NUL.
/// Non-Linux platforms use the conservative BSD/macOS bound; Windows uses named pipes instead.
#[cfg(target_os = "linux")]
pub const NATIVE_MAX_PATH_BYTES: usize = 107;

/// Conservative BSD/macOS pathname limit; Windows uses named pipes instead.
#[cfg(not(target_os = "linux"))]
pub const NATIVE_MAX_PATH_BYTES: usize = 103;

/// Preserves a usable Unix endpoint path or derives a short runtime path from its exact bytes.
///
/// The supplied limit belongs to the target platform, so qualification can also plan a foreign
/// Unix installation. The digest includes the complete original path, separating stores and
/// state roots even when two installations use the same store identity. No filesystem is changed.
///
/// # Errors
/// Rejects non-absolute Unix paths, NUL bytes, non-file final components, and limits that cannot
/// carry the fixed-size runtime address.
pub fn bounded_path(original: &Path, maximum_bytes: usize) -> io::Result<PathBuf> {
    let bytes = original.as_os_str().as_encoded_bytes();
    let name = bytes.rsplit(|byte| *byte == b'/').next().unwrap_or_default();
    if !bytes.starts_with(b"/") || bytes.contains(&0) || matches!(name, b"" | b"." | b"..") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid absolute Unix socket path",
        ));
    }
    if crate::verified::path_fits(bytes.len(), maximum_bytes) {
        return Ok(original.to_path_buf());
    }
    let mut hash = Sha256::new();
    hash.update(b"peritus/unix-runtime-endpoint/v1\0");
    hash.update(bytes);
    let digest: [u8; 32] = hash.finalize().into();
    let mut directory = String::from("/tmp/peritus-");
    for byte in &digest[..16] {
        use std::fmt::Write as _;
        write!(&mut directory, "{byte:02x}").expect("writing into String cannot fail");
    }
    directory.push_str("/daemon.sock");
    if !crate::verified::path_fits(directory.len(), maximum_bytes) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "platform socket path limit is too small for the private runtime endpoint",
        ));
    }
    Ok(PathBuf::from(directory))
}

#[cfg(test)]
mod tests;
