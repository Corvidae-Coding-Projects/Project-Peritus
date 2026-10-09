//! Closed mapping from immutable tool descriptors to production adapter constructors.

use peritus_tools_fs::FsDispatchKind;
use peritus_tools_git::GitDispatchKind;

/// Production filesystem adapter selections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemDispatcherRoute {
    /// `fs.create` through an authorized C1 mutation gateway.
    Create,
    /// `fs.discover` over an immutable C1 workspace.
    Discover,
    /// `fs.metadata` over an immutable C1 workspace.
    Metadata,
    /// `fs.patch` through an authorized C1 mutation gateway.
    Patch,
    /// `fs.read` over an immutable C1 workspace.
    Read,
    /// `fs.remove` through an authorized C1 mutation gateway.
    Remove,
    /// `fs.replace` through an authorized C1 mutation gateway.
    Replace,
    /// `fs.search` over an immutable C1 workspace.
    Search,
    /// `fs.write` through an authorized C1 mutation gateway.
    Write,
}

impl FilesystemDispatcherRoute {
    /// Returns the exact C4 adapter constructor discriminant.
    #[must_use]
    pub const fn dispatch_kind(self) -> FsDispatchKind {
        match self {
            Self::Create => FsDispatchKind::Create,
            Self::Discover => FsDispatchKind::Discover,
            Self::Metadata => FsDispatchKind::Metadata,
            Self::Patch => FsDispatchKind::Patch,
            Self::Read => FsDispatchKind::Read,
            Self::Remove => FsDispatchKind::Remove,
            Self::Replace => FsDispatchKind::Replace,
            Self::Search => FsDispatchKind::Search,
            Self::Write => FsDispatchKind::Write,
        }
    }
}

/// Production Git adapter selections.
///
/// `git.merge` is absent because C1 does not publish a merge effect; the existing typed
/// unsupported adapter is not a production handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitDispatcherRoute {
    /// `git.candidate` through an authorized C1 candidate gateway.
    Candidate,
    /// `git.diff` over an immutable C1 workspace.
    Diff,
    /// `git.history` over an immutable C1 workspace.
    History,
    /// `git.rollback` through an authorized C1 rollback gateway.
    Rollback,
    /// `git.snapshot` over an immutable C1 workspace.
    Snapshot,
    /// `git.status` over an immutable C1 workspace.
    Status,
}

impl GitDispatcherRoute {
    /// Returns the exact C4 adapter constructor discriminant.
    #[must_use]
    pub const fn dispatch_kind(self) -> GitDispatchKind {
        match self {
            Self::Candidate => GitDispatchKind::Candidate,
            Self::Diff => GitDispatchKind::Diff,
            Self::History => GitDispatchKind::History,
            Self::Rollback => GitDispatchKind::Rollback,
            Self::Snapshot => GitDispatchKind::Snapshot,
            Self::Status => GitDispatchKind::Status,
        }
    }
}

/// Exact production dispatcher constructor selected for one descriptor.
///
/// The route is immutable startup metadata. The daemon uses it to construct the corresponding
/// scoped C4 dispatcher only after the required target and lower-layer authority are available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolDispatcherRoute {
    /// A descriptor-specific filesystem adapter.
    Filesystem(FilesystemDispatcherRoute),
    /// A descriptor-specific Git adapter.
    Git(GitDispatcherRoute),
    /// Explicit workspace quality discovery.
    QualityDiscover,
    /// One exact cataloged quality execution.
    QualityRun,
    /// Structured argv execution.
    ShellExec,
    /// Explicit interpreter and script execution.
    ShellScript,
}

impl ToolDispatcherRoute {
    /// Returns the one canonical capability name served by this route.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Filesystem(FilesystemDispatcherRoute::Create) => "fs.create",
            Self::Filesystem(FilesystemDispatcherRoute::Discover) => "fs.discover",
            Self::Filesystem(FilesystemDispatcherRoute::Metadata) => "fs.metadata",
            Self::Filesystem(FilesystemDispatcherRoute::Patch) => "fs.patch",
            Self::Filesystem(FilesystemDispatcherRoute::Read) => "fs.read",
            Self::Filesystem(FilesystemDispatcherRoute::Remove) => "fs.remove",
            Self::Filesystem(FilesystemDispatcherRoute::Replace) => "fs.replace",
            Self::Filesystem(FilesystemDispatcherRoute::Search) => "fs.search",
            Self::Filesystem(FilesystemDispatcherRoute::Write) => "fs.write",
            Self::Git(GitDispatcherRoute::Candidate) => "git.candidate",
            Self::Git(GitDispatcherRoute::Diff) => "git.diff",
            Self::Git(GitDispatcherRoute::History) => "git.history",
            Self::Git(GitDispatcherRoute::Rollback) => "git.rollback",
            Self::Git(GitDispatcherRoute::Snapshot) => "git.snapshot",
            Self::Git(GitDispatcherRoute::Status) => "git.status",
            Self::QualityDiscover => "quality.discover",
            Self::QualityRun => "quality.run",
            Self::ShellExec => "shell.exec",
            Self::ShellScript => "shell.script",
        }
    }
}
