//! Evidence-bound completion proposals with no effect authority.

use crate::{AgentErrorCode, AgentOperation, AgentRecovery, AgentRejection, SafeText};
use peritus_types::{EvidenceId, RevisionTuple, Sha256Digest};

/// Exact evidence identity and the revision at which it was observed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvidenceReference {
    id: EvidenceId,
    revision: RevisionTuple,
}

impl EvidenceReference {
    #[must_use]
    pub const fn new(id: EvidenceId, revision: RevisionTuple) -> Self {
        Self { id, revision }
    }
    #[must_use]
    pub const fn id(self) -> EvidenceId {
        self.id
    }
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
}

/// Requested orchestration action. This is data, never authority to perform it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompletionRequest {
    RunGates,
    RequestReview,
    ContinueFixing,
    RequestAuthority,
    ReportBlocked,
}

/// Digests of the exact context, model response, and ordered tool transcript.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TranscriptDigests {
    context: Sha256Digest,
    model: Sha256Digest,
    tools: Sha256Digest,
}

impl TranscriptDigests {
    #[must_use]
    pub const fn new(context: Sha256Digest, model: Sha256Digest, tools: Sha256Digest) -> Self {
        Self { context, model, tools }
    }
    #[must_use]
    pub const fn context(self) -> Sha256Digest {
        self.context
    }
    #[must_use]
    pub const fn model(self) -> Sha256Digest {
        self.model
    }
    #[must_use]
    pub const fn tools(self) -> Sha256Digest {
        self.tools
    }
}

/// Bounded proposal emitted by a completed agent turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionProposal {
    summary: SafeText,
    evidence: Vec<EvidenceReference>,
    uncertainties: Vec<SafeText>,
    revision: RevisionTuple,
    transcripts: TranscriptDigests,
    requested: CompletionRequest,
}

impl CompletionProposal {
    pub const MAX_EVIDENCE: usize = 1_024;
    pub const MAX_UNCERTAINTIES: usize = 64;

    /// Validates revision freshness and canonical evidence order.
    ///
    /// # Errors
    ///
    /// Returns a typed rejection for excessive collections, stale evidence, or unstable order.
    pub fn new(
        summary: SafeText,
        evidence: Vec<EvidenceReference>,
        uncertainties: Vec<SafeText>,
        revision: RevisionTuple,
        transcripts: TranscriptDigests,
        requested: CompletionRequest,
    ) -> Result<Self, AgentRejection> {
        if evidence.len() > Self::MAX_EVIDENCE || uncertainties.len() > Self::MAX_UNCERTAINTIES {
            return Err(completion_error(
                AgentErrorCode::InvalidLimit,
                "completion collection exceeds its retained-state bound",
            ));
        }
        if evidence.iter().any(|reference| reference.revision != revision) {
            return Err(completion_error(
                AgentErrorCode::RevisionMismatch,
                "completion evidence is stale for the bound revision",
            ));
        }
        if evidence.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return Err(completion_error(
                AgentErrorCode::NonCanonicalOrder,
                "completion evidence must be strictly ordered",
            ));
        }
        Ok(Self { summary, evidence, uncertainties, revision, transcripts, requested })
    }

    #[must_use]
    pub const fn summary(&self) -> &SafeText {
        &self.summary
    }
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceReference] {
        &self.evidence
    }
    #[must_use]
    pub fn uncertainties(&self) -> &[SafeText] {
        &self.uncertainties
    }
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    pub const fn transcripts(&self) -> TranscriptDigests {
        self.transcripts
    }
    #[must_use]
    pub const fn requested(&self) -> CompletionRequest {
        self.requested
    }
}

const fn completion_error(code: AgentErrorCode, detail: &'static str) -> AgentRejection {
    AgentRejection::new(
        code,
        AgentOperation::ValidateCompletion,
        AgentRecovery::CorrectRequest,
        detail,
    )
}
