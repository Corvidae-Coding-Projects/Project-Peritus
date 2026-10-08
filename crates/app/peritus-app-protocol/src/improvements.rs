//! Evidence-backed suggestions. Only an explicit evaluation request may start inference.

use crate::{AppErrorCode, AppProtocolError, ConversationId, ProductProviderSelection};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

mod paging;
pub use paging::*;

/// Exact execution identity and target selected for one explicit evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImprovementEvaluationRequest {
    run: RunId,
    target: WorkspaceId,
    providers: ProductProviderSelection,
}

impl ImprovementEvaluationRequest {
    /// Binds the distinct run, target workspace, and provider roles without embedding task text.
    #[must_use]
    pub const fn new(run: RunId, target: WorkspaceId, providers: ProductProviderSelection) -> Self {
        Self { run, target, providers }
    }

    /// Returns the requested execution lineage.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }

    /// Returns the explicitly selected Peritus source workspace.
    #[must_use]
    pub const fn target(self) -> WorkspaceId {
        self.target
    }

    /// Returns the selected provider roles.
    #[must_use]
    pub const fn providers(self) -> ProductProviderSelection {
        self.providers
    }
}

/// Durable route to an evaluation, available before its first run is admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImprovementEvaluation {
    conversation: ConversationId,
    run: RunId,
    target: WorkspaceId,
}

impl ImprovementEvaluation {
    /// Binds the durable conversation, execution lineage, and target workspace.
    #[must_use]
    pub const fn new(conversation: ConversationId, run: RunId, target: WorkspaceId) -> Self {
        Self { conversation, run, target }
    }

    /// Returns the durable workbench identity.
    #[must_use]
    pub const fn conversation(self) -> ConversationId {
        self.conversation
    }

    /// Returns the evaluation run identity, whether or not admission has completed yet.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }

    /// Returns the exact Peritus source workspace.
    #[must_use]
    pub const fn target(self) -> WorkspaceId {
        self.target
    }
}

/// Checked inert text; evidence does not become an instruction or permission.
/// Physical codecs and stores own their representation and allocation bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementText(String);

impl ImprovementText {
    /// Checks nonempty text, rejecting terminal controls.
    ///
    /// # Errors
    /// Rejects empty or control-containing input.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        if value.trim().is_empty()
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
    /// Read one metadata page; retained text and evidence are independently addressable.
    ListPage {
        /// Source workspace.
        workspace: WorkspaceId,
        /// Exact scope and revision of the preceding page.
        after: Option<ImprovementPageCursor>,
    },
    /// Read one candidate's evidence metadata without loading its observation bodies.
    EvidencePage {
        /// Source workspace.
        workspace: WorkspaceId,
        /// Candidate identity.
        candidate: Sha256Digest,
        /// Exact revision returned with the owning candidate page.
        revision: u64,
        /// Exact scope and revision of the preceding page.
        after: Option<ImprovementPageCursor>,
    },
    /// Read a bounded UTF-8 slice of one immutable, digest-bound retained body.
    ReadText(ImprovementTextQuery),
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
    Evaluate {
        /// Source workspace.
        workspace: WorkspaceId,
        /// Exact suggestion identity.
        candidate: Sha256Digest,
        /// Proposed evaluation identity, target workspace and configured provider routes.
        evaluation: ImprovementEvaluationRequest,
    },
}

impl ImprovementRequest {
    /// Returns the source workspace owning the inbox.
    #[must_use]
    pub const fn workspace(&self) -> WorkspaceId {
        match self {
            Self::List(workspace)
            | Self::ListPage { workspace, .. }
            | Self::EvidencePage { workspace, .. }
            | Self::Suggest { workspace, .. }
            | Self::Dismiss { workspace, .. }
            | Self::Evaluate { workspace, .. } => *workspace,
            Self::ReadText(query) => query.workspace(),
        }
    }

    /// Returns whether this request requires the independently negotiated paged contract.
    #[must_use]
    pub const fn requires_paging(&self) -> bool {
        matches!(self, Self::ListPage { .. } | Self::EvidencePage { .. } | Self::ReadText(_))
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
    evaluation: Option<ImprovementEvaluation>,
    dismissed: bool,
}

impl ImprovementCandidate {
    /// Checks nonempty, distinct evidence. An evaluation is a patch run, not a promotion.
    ///
    /// # Errors
    /// Rejects missing or duplicate source runs.
    pub fn new(
        id: Sha256Digest,
        proposal: ImprovementText,
        evidence: Vec<ImprovementEvidence>,
        evaluation: Option<ImprovementEvaluation>,
        dismissed: bool,
    ) -> Result<Self, AppProtocolError> {
        let mut runs = std::collections::BTreeSet::new();
        if evidence.is_empty() || evidence.iter().any(|item| !runs.insert(item.run.into_bytes())) {
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
    /// Returns supporting observations.
    #[must_use]
    pub fn evidence(&self) -> &[ImprovementEvidence] {
        &self.evidence
    }
    /// Returns the durable evaluation route, if reserved.
    #[must_use]
    pub const fn evaluation(&self) -> Option<ImprovementEvaluation> {
        self.evaluation
    }
    /// Returns whether the user dismissed this suggestion.
    #[must_use]
    pub const fn dismissed(&self) -> bool {
        self.dismissed
    }
}

/// Workspace inbox retained for compatibility; paged transport owns response sizing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImprovementInbox {
    workspace: WorkspaceId,
    candidates: Vec<ImprovementCandidate>,
}

impl ImprovementInbox {
    /// Checks distinct candidate identities.
    ///
    /// # Errors
    /// Rejects duplicate records.
    pub fn new(
        workspace: WorkspaceId,
        candidates: Vec<ImprovementCandidate>,
    ) -> Result<Self, AppProtocolError> {
        let mut identities = std::collections::BTreeSet::new();
        if candidates.iter().any(|item| !identities.insert(item.id.into_bytes())) {
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
