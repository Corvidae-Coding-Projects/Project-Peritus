//! Bounded inert preview launch profiles and exact source identities.

use super::{
    MAX_LAUNCH_TEXT_BYTES, MAX_LAUNCH_WALL_MILLIS, MAX_PREVIEW_INPUT_BYTES,
    MAX_WORKBENCH_LAUNCH_ARGUMENTS, MAX_WORKBENCH_LAUNCH_ENVIRONMENT, invalid,
    valid_environment_name,
};
use crate::AppProtocolError;
use peritus_types::{RunId, Sha256Digest};

/// Bounded inert profile text; it is never parsed as a shell command.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkbenchLaunchText(String);

impl WorkbenchLaunchText {
    /// Creates nonempty literal text without NUL or terminal control characters.
    ///
    /// # Errors
    /// Rejects empty, oversized, NUL-containing, or display-control-containing text.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        if value.is_empty()
            || value.len() > MAX_LAUNCH_TEXT_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self(value))
    }
    /// Borrows exact literal text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Debug for WorkbenchLaunchText {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WorkbenchLaunchText")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Exact bytes accepted only by an interactive preview's bounded input channel.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchPreviewInput(Vec<u8>);

impl WorkbenchPreviewInput {
    /// Creates one nonempty bounded input write.
    ///
    /// # Errors
    /// Rejects empty or oversized input.
    pub fn new(bytes: Vec<u8>) -> Result<Self, AppProtocolError> {
        if bytes.is_empty() || bytes.len() > MAX_PREVIEW_INPUT_BYTES {
            return Err(invalid());
        }
        Ok(Self(bytes))
    }
    /// Borrows exact input bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl core::fmt::Debug for WorkbenchPreviewInput {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WorkbenchPreviewInput")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Source identity semantics for one launch.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchLaunchSourceKind {
    /// Current complete managed candidate identity.
    ManagedCandidate,
    /// Exact explicitly named file in a plain folder; never represented as a Git candidate.
    PlainFolderFile,
}

impl WorkbenchLaunchSourceKind {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::ManagedCandidate => 1,
            Self::PlainFolderFile => 2,
        }
    }
    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::ManagedCandidate),
            2 => Some(Self::PlainFolderFile),
            _ => None,
        }
    }
}

/// Exact source snapshot named by a profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchLaunchSource {
    kind: WorkbenchLaunchSourceKind,
    path: WorkbenchLaunchText,
    digest: Sha256Digest,
}

impl WorkbenchLaunchSource {
    /// Creates a source identity; the daemon rechecks the digest before launch.
    #[must_use]
    pub const fn new(
        kind: WorkbenchLaunchSourceKind,
        path: WorkbenchLaunchText,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, path, digest }
    }
    /// Returns managed-candidate or plain-folder-file semantics.
    #[must_use]
    pub const fn kind(&self) -> WorkbenchLaunchSourceKind {
        self.kind
    }
    /// Borrows the explicit source label/path.
    #[must_use]
    pub const fn path(&self) -> &WorkbenchLaunchText {
        &self.path
    }
    /// Returns the exact expected source digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Optional exact build output identity, independent from source identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchBuildIdentity {
    path: WorkbenchLaunchText,
    digest: Sha256Digest,
}

impl WorkbenchBuildIdentity {
    /// Creates an exact build artifact identity; the daemon rehashes the file before launch.
    #[must_use]
    pub const fn new(path: WorkbenchLaunchText, digest: Sha256Digest) -> Self {
        Self { path, digest }
    }
    /// Borrows the workspace-relative build path.
    #[must_use]
    pub const fn path(&self) -> &WorkbenchLaunchText {
        &self.path
    }
    /// Returns the exact build digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Explicit network posture of the currently supported raw local preview path.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchPreviewNetwork {
    /// Retains host networking; this is visible and never mislabeled as isolation.
    InheritedHost,
}

/// Explicit lifecycle cleanup policy of every preview.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchPreviewStopPolicy {
    /// Cancellation terminates and joins the complete C2-owned process tree.
    TerminateOwnedTree,
}

/// Bounded direct-execution profile. Discovery may construct this DTO but never executes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchLaunchProfile {
    run: RunId,
    executable: WorkbenchLaunchText,
    arguments: Vec<WorkbenchLaunchText>,
    working_directory: WorkbenchLaunchText,
    environment: Vec<WorkbenchLaunchText>,
    source: WorkbenchLaunchSource,
    build: Option<WorkbenchBuildIdentity>,
    readiness_millis: u64,
    wall_millis: u64,
    interactive: bool,
    network: WorkbenchPreviewNetwork,
    stop_policy: WorkbenchPreviewStopPolicy,
}

impl WorkbenchLaunchProfile {
    /// Validates collection and finite time bounds without performing discovery or execution.
    ///
    /// # Errors
    /// Rejects excessive/duplicate environment references or invalid finite limits.
    #[allow(clippy::too_many_arguments, reason = "independent launch bindings remain explicit")]
    pub fn new(
        run: RunId,
        executable: WorkbenchLaunchText,
        arguments: Vec<WorkbenchLaunchText>,
        working_directory: WorkbenchLaunchText,
        mut environment: Vec<WorkbenchLaunchText>,
        source: WorkbenchLaunchSource,
        build: Option<WorkbenchBuildIdentity>,
        readiness_millis: u64,
        wall_millis: u64,
        interactive: bool,
    ) -> Result<Self, AppProtocolError> {
        environment.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        if arguments.len() > MAX_WORKBENCH_LAUNCH_ARGUMENTS
            || environment.len() > MAX_WORKBENCH_LAUNCH_ENVIRONMENT
            || environment.windows(2).any(|pair| pair[0] == pair[1])
            || environment.iter().any(|name| !valid_environment_name(name.as_str()))
            || readiness_millis == 0
            || readiness_millis > wall_millis
            || wall_millis > MAX_LAUNCH_WALL_MILLIS
        {
            return Err(invalid());
        }
        Ok(Self {
            run,
            executable,
            arguments,
            working_directory,
            environment,
            source,
            build,
            readiness_millis,
            wall_millis,
            interactive,
            network: WorkbenchPreviewNetwork::InheritedHost,
            stop_policy: WorkbenchPreviewStopPolicy::TerminateOwnedTree,
        })
    }
    /// Returns the governing product-run lineage.
    #[must_use]
    pub const fn run(&self) -> RunId {
        self.run
    }
    /// Borrows the literal executable.
    #[must_use]
    pub const fn executable(&self) -> &WorkbenchLaunchText {
        &self.executable
    }
    /// Borrows literal arguments without shell parsing.
    #[must_use]
    pub fn arguments(&self) -> &[WorkbenchLaunchText] {
        &self.arguments
    }
    /// Borrows the workspace-relative working directory.
    #[must_use]
    pub const fn working_directory(&self) -> &WorkbenchLaunchText {
        &self.working_directory
    }
    /// Borrows sorted required environment-variable names; values never cross A3.
    #[must_use]
    pub fn environment(&self) -> &[WorkbenchLaunchText] {
        &self.environment
    }
    /// Borrows the rechecked source identity.
    #[must_use]
    pub const fn source(&self) -> &WorkbenchLaunchSource {
        &self.source
    }
    /// Borrows the optional independently rechecked build identity.
    #[must_use]
    pub const fn build(&self) -> Option<&WorkbenchBuildIdentity> {
        self.build.as_ref()
    }
    /// Returns the bounded readiness-observation wait.
    #[must_use]
    pub const fn readiness_millis(&self) -> u64 {
        self.readiness_millis
    }
    /// Returns the immutable process wall deadline.
    #[must_use]
    pub const fn wall_millis(&self) -> u64 {
        self.wall_millis
    }
    /// Returns whether bounded process input is enabled.
    #[must_use]
    pub const fn interactive(&self) -> bool {
        self.interactive
    }
    /// Returns the truthful host-network posture.
    #[must_use]
    pub const fn network(&self) -> WorkbenchPreviewNetwork {
        self.network
    }
    /// Returns the mandatory owned-tree cleanup policy.
    #[must_use]
    pub const fn stop_policy(&self) -> WorkbenchPreviewStopPolicy {
        self.stop_policy
    }
}
