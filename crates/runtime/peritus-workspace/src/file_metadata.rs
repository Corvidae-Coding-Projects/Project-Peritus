//! Portable workspace metadata used by grounding and progress evidence.

use std::fs::Metadata;

/// Returns a stable permission fingerprint for progress comparisons.
#[must_use]
pub fn permission_fingerprint(metadata: &Metadata) -> u32 {
    permission_bits(metadata)
}

/// Renders the current workspace permissions without pretending Git stores every permission bit.
#[must_use]
pub fn permissions(metadata: &Metadata) -> String {
    #[cfg(unix)]
    {
        format!("{:04o}", permission_bits(metadata))
    }
    #[cfg(not(unix))]
    {
        if metadata.permissions().readonly() {
            "read-only".to_owned()
        } else {
            "read-write".to_owned()
        }
    }
}

/// Returns the executable-bit representation supported by Git tree entries.
#[cfg(unix)]
#[must_use]
pub fn git_file_mode(metadata: &Metadata) -> &'static str {
    if permission_bits(metadata) & 0o111 == 0 { "100644" } else { "100755" }
}

/// Returns Git's regular-file mode on platforms without Unix executable bits.
#[cfg(not(unix))]
#[must_use]
pub const fn git_file_mode(_metadata: &Metadata) -> &'static str {
    "100644"
}

#[cfg(unix)]
fn permission_bits(metadata: &Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;

    metadata.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn permission_bits(metadata: &Metadata) -> u32 {
    u32::from(metadata.permissions().readonly())
}
