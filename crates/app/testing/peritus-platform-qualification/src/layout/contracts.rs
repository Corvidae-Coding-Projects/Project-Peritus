//! Ownership, entry-kind, and permission contracts for release-layout paths.

/// Owner responsible for a release-layout entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PathOwnership {
    /// Installed, replaced, rolled back, and removed by the package.
    Package,
    /// Provisioned by the operator and never overwritten or removed by ordinary package actions.
    Operator,
    /// Created and reconciled by `peritusd`; removed only by explicit purge.
    Runtime,
}

/// Filesystem shape of an installed entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EntryKind {
    /// Regular executable file.
    Executable,
    /// Regular non-executable file.
    File,
    /// Directory.
    Directory,
    /// Ephemeral Unix socket or Windows named-pipe expectation.
    Endpoint,
}

/// Platform-aware permissions required for an installed entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PermissionContract {
    /// Exact Unix permission mode.
    UnixMode(u16),
    /// Owner-only Windows DACL with inherited broad write access removed.
    WindowsOwnerOnly,
    /// Windows executable readable/executable by the owning user and not writable by other users.
    WindowsExecutable,
}
