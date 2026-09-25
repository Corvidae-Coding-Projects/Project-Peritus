//! Evidence-backed suggestions. Only an explicit evaluation request may start inference.

use crate::{AppErrorCode, AppProtocolError, ProductRunRequest};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

/// Maximum suggestions returned for one workspace.
pub const MAX_IMPROVEMENTS: usize = 32;
/// Maximum retained distinct run references for one suggestion.
pub const MAX_IMPROVEMENT_EVIDENCE: usize = 4;
/// Maximum bytes in a proposal or evidence summary.
pub const MAX_IMPROVEMENT_TEXT: usize = 4096;

/// Checked inert text; evidence does not become an instruction or permission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementText(String);

impl ImprovementText {
    /// Checks nonempty bounded text, rejecting terminal controls.
    ///
    /// # Errors
    /// Rejects empty, oversized, or control-containing input.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        if value.trim().is_empty()
            || value.len() > MAX_IMPROVEMENT_TEXT
            || value.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self(value))
    }

    /// Borrows the exact text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Read, collect, dismiss, or explicitly evaluate one suggestion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImprovementRequest {
    /// Read the durable inbox; this never starts work.
    List(WorkspaceId),
    /// Retain a user suggestion backed by a real completed run in this workspace.
    Suggest {
        /// Source workspace.
        workspace: WorkspaceId,
        /// Completed source run.
        run: RunId,
        /// Inert untested suggestion.
        proposal: ImprovementText,
    },
    /// Hide a suggestion without erasing its evidence or evaluation history.
    Dismiss {
        /// Source workspace.
        workspace: WorkspaceId,
        /// Exact suggestion identity.
        candidate: Sha256Digest,
    },
    /// Generate and test a patch in an explicitly selected harness source workspace.
    /// The daemon derives the task and ignores the supplied task text.
    Evaluate {
        /// Source workspace.
        workspace: WorkspaceId,
        /// Exact suggestion identity.
        candidate: Sha256Digest,
        /// Proposed evaluation identity, target workspace and configured provider routes.
        run: ProductRunRequest,
    },
}

impl ImprovementRequest {
    /// Returns the source workspace owning the inbox.
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId {
        match self {
            Self::List(workspace)
            | Self::Suggest { workspace, .. }
            | Self::Dismiss { workspace, .. }
            | Self::Evaluate { workspace, .. } => *workspace,
        }
    }
}

/// A digest-bound terminal observation retained independently of later run updates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementEvidence {
    run: RunId,
    digest: Sha256Digest,
    summary: ImprovementText,
}

impl ImprovementEvidence {
    /// Binds a run and exact observed summary digest.
    #[must_use]
    pub const fn new(run: RunId, digest: Sha256Digest, summary: ImprovementText) -> Self {
        Self { run, digest, summary }
    }
    /// Returns the source run.
    #[must_use]
    pub const fn run(&self) -> RunId {
        self.run
    }
    /// Returns the immutable observation digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the observed evidence, never executable instructions.
    #[must_use]
    pub const fn summary(&self) -> &ImprovementText {
        &self.summary
    }
}

/// Durable suggestion and original evaluation run identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementCandidate {
    id: Sha256Digest,
    proposal: ImprovementText,
    evidence: Vec<ImprovementEvidence>,
    evaluation: Option<RunId>,
    dismissed: bool,
}

impl ImprovementCandidate {
    /// Checks bounded, distinct evidence. An evaluation is a patch run, not a promotion.
    ///
    /// # Errors
    /// Rejects missing, excessive, or duplicate source runs.
    pub fn new(
        id: Sha256Digest,
        proposal: ImprovementText,
        evidence: Vec<ImprovementEvidence>,
        evaluation: Option<RunId>,
        dismissed: bool,
    ) -> Result<Self, AppProtocolError> {
        if evidence.is_empty()
            || evidence.len() > MAX_IMPROVEMENT_EVIDENCE
            || evidence
                .iter()
                .enumerate()
                .any(|(i, e)| evidence[..i].iter().any(|other| other.run == e.run))
        {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { id, proposal, evidence, evaluation, dismissed })
    }
    /// Returns the stable deduplication identity.
    #[must_use]
    pub const fn id(&self) -> Sha256Digest {
        self.id
    }
    /// Returns the untested suggestion.
    #[must_use]
    pub const fn proposal(&self) -> &ImprovementText {
        &self.proposal
    }
    /// Returns bounded supporting observations.
    #[must_use]
    pub fn evidence(&self) -> &[ImprovementEvidence] {
        &self.evidence
    }
    /// Returns the explicit patch-generation/evaluation run, if reserved.
    #[must_use]
    pub const fn evaluation(&self) -> Option<RunId> {
        self.evaluation
    }
    /// Returns whether the user dismissed this suggestion.
    #[must_use]
    pub const fn dismissed(&self) -> bool {
        self.dismissed
    }
}

/// Bounded workspace inbox.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementInbox {
    workspace: WorkspaceId,
    candidates: Vec<ImprovementCandidate>,
}

impl ImprovementInbox {
    /// Checks the inbox bound and distinct candidate identities.
    ///
    /// # Errors
    /// Rejects oversized or duplicate records.
    pub fn new(
        workspace: WorkspaceId,
        candidates: Vec<ImprovementCandidate>,
    ) -> Result<Self, AppProtocolError> {
        if candidates.len() > MAX_IMPROVEMENTS
            || candidates
                .iter()
                .enumerate()
                .any(|(i, c)| candidates[..i].iter().any(|other| other.id == c.id))
        {
            return Err(AppProtocolError::new(AppErrorCode::LimitExceeded, None));
        }
        Ok(Self { workspace, candidates })
    }
    /// Returns the exact source workspace.
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId {
        self.workspace
    }
    /// Borrows retained suggestions, including dismissed history.
    #[must_use]
    pub fn candidates(&self) -> &[ImprovementCandidate] {
        &self.candidates
    }
}
