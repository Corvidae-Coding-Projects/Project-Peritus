//! Bounded, approval-first project initialization contracts.
//!
//! Discovery observations and proposed commands are inert data. Accepting an initialization
//! proposal authorizes only its exact instruction-file patch; command execution remains separate.

use crate::{AppErrorCode, AppProtocolError, WorkbenchQuery};
use peritus_types::Sha256Digest;

mod render;
use render::{managed_content, render_exact_diff};
mod validation;
use validation::{valid_command_part, valid_relative_path};

#[cfg(test)]
mod tests;

/// The sole instruction file managed by the first initialization flow.
pub const INIT_INSTRUCTION_PATH: &str = "AGENTS.md";
/// Maximum bytes read from any one selected initialization source.
pub const MAX_INIT_SOURCE_BYTES: usize = 256 * 1024;
const MAX_INIT_SOURCE_BYTES_U64: u64 = 256 * 1024;
/// Maximum exact instruction-file bytes retained in a proposal.
pub const MAX_INIT_INSTRUCTION_BYTES: usize = 256 * 1024;
/// Maximum exact rendered diff bytes retained in a proposal.
pub const MAX_INIT_DIFF_BYTES: usize = 768 * 1024;
/// Maximum selected sources recorded by discovery.
pub const MAX_INIT_SOURCES: usize = 16;
/// Maximum structured command candidates recorded by discovery.
pub const MAX_INIT_COMMANDS: usize = 32;
/// Maximum arguments in one command candidate.
pub const MAX_INIT_COMMAND_ARGUMENTS: usize = 16;

/// Opening marker for the replaceable Peritus-owned instruction section.
pub const INIT_SECTION_START: &str = "<!-- peritus:init:v1:start -->";
/// Closing marker for the replaceable Peritus-owned instruction section.
pub const INIT_SECTION_END: &str = "<!-- peritus:init:v1:end -->";

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

/// Read-only discovery request bound to an inspected conversation aggregate revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitDiscoveryRequest {
    query: WorkbenchQuery,
    revision: u64,
}

impl InitDiscoveryRequest {
    /// Creates an inert discovery request.
    ///
    /// # Errors
    /// Rejects the absent aggregate revision zero.
    pub const fn new(query: WorkbenchQuery, revision: u64) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(invalid());
        }
        Ok(Self { query, revision })
    }

    /// Returns the exact conversation and workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }

    /// Returns the inspected conversation aggregate revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// Closed category for a bounded locally observed discovery source.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InitSourceKind {
    /// A language or package manifest.
    Manifest,
    /// Root-local project documentation.
    Documentation,
    /// Existing project instructions.
    Instructions,
    /// Root-local command configuration.
    CommandConfig,
}

/// Content-free exact observation of one selected local source.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InitSourceObservation {
    path: String,
    kind: InitSourceKind,
    digest: Sha256Digest,
    bytes: u64,
}

impl InitSourceObservation {
    /// Constructs one bounded source observation without granting future read authority.
    ///
    /// # Errors
    /// Rejects a noncanonical path or a source over the discovery ceiling.
    pub fn new(
        path: String,
        kind: InitSourceKind,
        digest: Sha256Digest,
        bytes: u64,
    ) -> Result<Self, AppProtocolError> {
        if !valid_relative_path(&path) || bytes > MAX_INIT_SOURCE_BYTES_U64 {
            return Err(invalid());
        }
        Ok(Self { path, kind, digest, bytes })
    }

    /// Borrows the canonical workspace-relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the host-classified source kind.
    #[must_use]
    pub const fn kind(&self) -> InitSourceKind {
        self.kind
    }

    /// Returns the complete exact source digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the complete exact source byte count.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Portable regular-file mode retained by the reviewed instruction patch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitFileMode {
    /// Non-executable regular file.
    Regular,
    /// Executable regular file; retained only when the existing instruction file has this mode.
    Executable,
}

/// Closed command purpose shown by initialization discovery.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InitCommandKind {
    /// Build or compile the project.
    Build,
    /// Run project tests.
    Test,
    /// Run static checks or linting.
    Lint,
    /// Launch a project-local executable or development entry point.
    Launch,
}

impl InitCommandKind {
    /// Returns the stable human-readable category name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Build => "Build",
            Self::Test => "Test",
            Self::Lint => "Lint",
            Self::Launch => "Launch",
        }
    }
}

/// Verification state for discovered command data.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InitCommandVerification {
    /// The command was inferred from local configuration but has not been run.
    Unverified,
}

/// One structured command candidate; it is not an execution request.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InitCommand {
    kind: InitCommandKind,
    source: String,
    executable: String,
    arguments: Vec<String>,
    verification: InitCommandVerification,
}

impl InitCommand {
    /// Constructs one inert, unverified command candidate.
    ///
    /// # Errors
    /// Rejects malformed sources, unsafe display text, or excessive arguments.
    pub fn new(
        kind: InitCommandKind,
        source: String,
        executable: String,
        arguments: Vec<String>,
        verification: InitCommandVerification,
    ) -> Result<Self, AppProtocolError> {
        if !valid_relative_path(&source)
            || !valid_command_part(&executable, 128)
            || arguments.len() > MAX_INIT_COMMAND_ARGUMENTS
            || arguments.iter().any(|argument| !valid_command_part(argument, 1024))
        {
            return Err(invalid());
        }
        Ok(Self { kind, source, executable, arguments, verification })
    }

    /// Returns the command purpose.
    #[must_use]
    pub const fn kind(&self) -> InitCommandKind {
        self.kind
    }

    /// Borrows the exact local source path from which the candidate was derived.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Borrows the executable token.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// Borrows the exact argument vector; it is never interpreted by discovery.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    /// Returns the explicit verification label.
    #[must_use]
    pub const fn verification(&self) -> InitCommandVerification {
        self.verification
    }
}

/// Exact reviewed instruction-file replacement and its complete precondition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitInstructionPatch {
    path: String,
    original_content: Option<String>,
    precondition_digest: Option<Sha256Digest>,
    precondition_bytes: u64,
    mode: InitFileMode,
    proposed_content: String,
    diff: String,
}

impl InitInstructionPatch {
    /// Validates an exact whole-file proposal and its deterministic review diff.
    ///
    /// # Errors
    /// Rejects a non-`AGENTS.md` target, inconsistent precondition, oversized content, no-op,
    /// or a diff that does not exactly describe the retained original and proposed bytes.
    #[allow(clippy::too_many_arguments, reason = "all exact patch bindings are independent")]
    pub fn new(
        path: String,
        original_content: Option<String>,
        precondition_digest: Option<Sha256Digest>,
        precondition_bytes: u64,
        mode: InitFileMode,
        proposed_content: String,
        diff: String,
    ) -> Result<Self, AppProtocolError> {
        let original_valid = match (&original_content, precondition_digest) {
            (None, None) => precondition_bytes == 0 && mode == InitFileMode::Regular,
            (Some(original), Some(digest)) => {
                original.len() <= MAX_INIT_INSTRUCTION_BYTES
                    && u64::try_from(original.len()).ok() == Some(precondition_bytes)
                    && peritus_codec::sha256(original.as_bytes()) == digest
            }
            _ => false,
        };
        if path != INIT_INSTRUCTION_PATH
            || !original_valid
            || proposed_content.len() > MAX_INIT_INSTRUCTION_BYTES
            || diff.len() > MAX_INIT_DIFF_BYTES
            || original_content.as_deref() == Some(proposed_content.as_str())
            || diff != render_exact_diff(&path, original_content.as_deref(), &proposed_content)
        {
            return Err(invalid());
        }
        Ok(Self {
            path,
            original_content,
            precondition_digest,
            precondition_bytes,
            mode,
            proposed_content,
            diff,
        })
    }

    /// Borrows the sole exact target path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Borrows the complete original UTF-8 content, or `None` when the target must be absent.
    #[must_use]
    pub fn original_content(&self) -> Option<&str> {
        self.original_content.as_deref()
    }

    /// Returns the exact original digest, or `None` when absence is required.
    #[must_use]
    pub const fn precondition_digest(&self) -> Option<Sha256Digest> {
        self.precondition_digest
    }

    /// Returns the exact original byte count; zero also permits a present empty file.
    #[must_use]
    pub const fn precondition_bytes(&self) -> u64 {
        self.precondition_bytes
    }

    /// Returns the retained portable file mode.
    #[must_use]
    pub const fn mode(&self) -> InitFileMode {
        self.mode
    }

    /// Borrows the complete exact proposed UTF-8 file content.
    #[must_use]
    pub fn proposed_content(&self) -> &str {
        &self.proposed_content
    }

    /// Borrows the deterministic whole-file review diff.
    #[must_use]
    pub fn diff(&self) -> &str {
        &self.diff
    }
}

mod proposal;
pub use proposal::InitProposal;
