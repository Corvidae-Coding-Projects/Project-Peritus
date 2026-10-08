//! Authoritative operation knowledge and legal controls projected with every product run.

use super::{ProductRunControlAction, ProductRunMessageError, bounded_text};

/// Kind of operation whose outcome and recovery choices are currently authoritative for a run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunOperationKind {
    /// The coding run itself, including an exact retry of the same run identity.
    Execution,
    /// An explicit managed-worktree commit.
    Commit,
    /// An explicit managed-worktree discard transaction.
    Discard,
    /// A developer command whose external outcome is not yet known.
    Command,
}

impl ProductRunOperationKind {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Execution => 1,
            Self::Commit => 2,
            Self::Discard => 3,
            Self::Command => 4,
        }
    }

    /// Decodes a stable wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Execution),
            2 => Some(Self::Commit),
            3 => Some(Self::Discard),
            4 => Some(Self::Command),
            _ => None,
        }
    }
}

/// What the authoritative owner knows about the current operation outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunOperationState {
    /// The operation is durably admitted and still active.
    Running,
    /// The operation is waiting for material user input.
    WaitingForUser,
    /// The owner has durable evidence that the operation completed successfully.
    Succeeded,
    /// The owner has durable evidence that the operation stopped unsuccessfully.
    Failed,
    /// The owner has durable evidence that the user cancelled the operation.
    Cancelled,
    /// The original authority is retained and an explicit exact retry is safe.
    RecoveryRequired,
    /// An effect may have occurred, so replay is forbidden until its outcome is reconciled.
    OutcomeUnknown,
}

impl ProductRunOperationState {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Running => 1,
            Self::WaitingForUser => 2,
            Self::Succeeded => 3,
            Self::Failed => 4,
            Self::Cancelled => 5,
            Self::RecoveryRequired => 6,
            Self::OutcomeUnknown => 7,
        }
    }

    /// Decodes a stable wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Running),
            2 => Some(Self::WaitingForUser),
            3 => Some(Self::Succeeded),
            4 => Some(Self::Failed),
            5 => Some(Self::Cancelled),
            6 => Some(Self::RecoveryRequired),
            7 => Some(Self::OutcomeUnknown),
            _ => None,
        }
    }
}

/// Closed set of product controls admitted by the authoritative owner at this observation.
#[allow(clippy::struct_excessive_bools, reason = "one flag per closed product control")]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ProductRunLegalControls {
    cancel: bool,
    retry: bool,
    accept: bool,
    commit: bool,
    export: bool,
    discard: bool,
    acknowledge: bool,
}

impl ProductRunLegalControls {
    /// No product control is currently legal.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            cancel: false,
            retry: false,
            accept: false,
            commit: false,
            export: false,
            discard: false,
            acknowledge: false,
        }
    }

    /// Adds one legal control.
    #[must_use]
    pub const fn with(mut self, action: ProductRunControlAction) -> Self {
        match action {
            ProductRunControlAction::Cancel => self.cancel = true,
            ProductRunControlAction::Retry => self.retry = true,
            ProductRunControlAction::Accept => self.accept = true,
            ProductRunControlAction::Commit => self.commit = true,
            ProductRunControlAction::Export => self.export = true,
            ProductRunControlAction::Discard => self.discard = true,
            ProductRunControlAction::Acknowledge => self.acknowledge = true,
        }
        self
    }

    /// Whether one exact product control is admitted by this observation.
    #[must_use]
    pub const fn allows(self, action: ProductRunControlAction) -> bool {
        match action {
            ProductRunControlAction::Cancel => self.cancel,
            ProductRunControlAction::Retry => self.retry,
            ProductRunControlAction::Accept => self.accept,
            ProductRunControlAction::Commit => self.commit,
            ProductRunControlAction::Export => self.export,
            ProductRunControlAction::Discard => self.discard,
            ProductRunControlAction::Acknowledge => self.acknowledge,
        }
    }

    /// Whether cancellation is legal.
    #[must_use]
    pub const fn cancel(self) -> bool {
        self.cancel
    }
    /// Whether exact retry is legal.
    #[must_use]
    pub const fn retry(self) -> bool {
        self.retry
    }
    /// Whether accepting the candidate is legal.
    #[must_use]
    pub const fn accept(self) -> bool {
        self.accept
    }
    /// Whether committing the candidate is legal.
    #[must_use]
    pub const fn commit(self) -> bool {
        self.commit
    }
    /// Whether exporting the candidate is legal.
    #[must_use]
    pub const fn export(self) -> bool {
        self.export
    }
    /// Whether discarding the candidate is legal.
    #[must_use]
    pub const fn discard(self) -> bool {
        self.discard
    }
    /// Whether acknowledging an unprovable outcome without replay is legal.
    #[must_use]
    pub const fn acknowledge(self) -> bool {
        self.acknowledge
    }
}

/// Read-only projection of one authoritative operation owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunOperation {
    kind: ProductRunOperationKind,
    state: ProductRunOperationState,
    identity: String,
    known: String,
    uncertainty: String,
    legal_controls: ProductRunLegalControls,
}

impl ProductRunOperation {
    /// Creates a bounded operation projection.
    ///
    /// # Errors
    /// Rejects an empty identity or known-facts description, or an oversized field.
    pub fn new(
        kind: ProductRunOperationKind,
        state: ProductRunOperationState,
        identity: String,
        known: String,
        uncertainty: String,
        legal_controls: ProductRunLegalControls,
    ) -> Result<Self, ProductRunMessageError> {
        bounded_text(&identity, usize::MAX)?;
        bounded_text(&known, usize::MAX)?;
        if state == ProductRunOperationState::OutcomeUnknown {
            bounded_text(&uncertainty, usize::MAX)?;
        }
        Ok(Self { kind, state, identity, known, uncertainty, legal_controls })
    }

    /// Operation class.
    #[must_use]
    pub const fn kind(&self) -> ProductRunOperationKind {
        self.kind
    }
    /// Current owner knowledge state.
    #[must_use]
    pub const fn state(&self) -> ProductRunOperationState {
        self.state
    }
    /// Stable identity of the original operation authority.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }
    /// Facts proven by the authoritative owner.
    #[must_use]
    pub fn known(&self) -> &str {
        &self.known
    }
    /// Facts that remain unknown; empty only when no material uncertainty remains.
    #[must_use]
    pub fn uncertainty(&self) -> &str {
        &self.uncertainty
    }
    /// Product controls legal for this exact observation.
    #[must_use]
    pub const fn legal_controls(&self) -> ProductRunLegalControls {
        self.legal_controls
    }

    /// Whether this exact operation owner is at a boundary where explicit input may start work.
    #[must_use]
    pub fn may_start_execution(&self) -> bool {
        self.kind == ProductRunOperationKind::Execution
            && matches!(
                self.state,
                ProductRunOperationState::WaitingForUser
                    | ProductRunOperationState::Succeeded
                    | ProductRunOperationState::Failed
                    | ProductRunOperationState::Cancelled
                    | ProductRunOperationState::RecoveryRequired
            )
    }
}
