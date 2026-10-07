//! Immutable terminal settlement/reply obligations and honest legacy adoption state.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::ProductRunServiceError;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ObligationStep {
    Pending,
    Committed,
    EvidenceGap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ObligationFirstCause {
    RunnerOutcome,
    RunnerFailure,
    UserCancellation,
    DaemonShutdown,
    WorkerFailure,
    LegacyUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SettlementObligation {
    Pending(ObligationRoot),
    Resolved(ObligationRoot),
    LegacyAssessmentRequired(LegacyAssessment),
    ResolvedWithLegacyGap(LegacyAssessment),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ObligationRoot {
    pub(super) identity: peritus_types::Sha256Digest,
    pub(super) run: peritus_types::RunId,
    pub(super) attempt: u64,
    pub(super) start_operation: [u8; 16],
    pub(super) conversation: [u8; 16],
    pub(super) goal: [u8; 16],
    pub(super) goal_attempt: u64,
    pub(super) goal_user_revision: u64,
    pub(super) required_input_generation: Option<u64>,
    pub(super) evidence_input_generation: Option<u64>,
    pub(super) settlement: u16,
    pub(super) unresolved_effects: bool,
    pub(super) frozen_unix_millis: u64,
    pub(super) reply_expected_revision: u64,
    pub(super) reply_operation: [u8; 16],
    pub(super) reply_after_invocation: [u8; 16],
    pub(super) reply_text: String,
    pub(super) reply_digest: peritus_types::Sha256Digest,
    pub(super) reply_bytes: u64,
    pub(super) first_cause: ObligationFirstCause,
    pub(super) goal_step: ObligationStep,
    pub(super) reply_step: ObligationStep,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LegacyAssessment {
    pub(super) identity: peritus_types::Sha256Digest,
    pub(super) source_format: u16,
    pub(super) source_digest: peritus_types::Sha256Digest,
    pub(super) run: peritus_types::RunId,
    pub(super) attempt: u64,
    pub(super) handoff: u64,
    pub(super) incorporated: u64,
    pub(super) evidence_digest: peritus_types::Sha256Digest,
    pub(super) goal_step: ObligationStep,
    pub(super) reply_step: ObligationStep,
}

impl ObligationRoot {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        run: peritus_types::RunId,
        attempt: u64,
        start_operation: [u8; 16],
        conversation: [u8; 16],
        goal: [u8; 16],
        goal_attempt: u64,
        goal_user_revision: u64,
        required_input_generation: Option<u64>,
        evidence_input_generation: Option<u64>,
        settlement: u16,
        unresolved_effects: bool,
        frozen_unix_millis: u64,
        reply_expected_revision: u64,
        reply_operation: [u8; 16],
        reply_after_invocation: [u8; 16],
        reply_text: String,
        reply_digest: peritus_types::Sha256Digest,
        reply_bytes: u64,
        first_cause: ObligationFirstCause,
    ) -> Result<Self, ProductRunServiceError> {
        if attempt == 0
            || frozen_unix_millis == 0
            || reply_bytes == 0
            || u64::try_from(reply_text.len()).unwrap_or(u64::MAX) != reply_bytes
            || peritus_codec::sha256(reply_text.as_bytes()) != reply_digest
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        let identity = obligation_identity(
            run,
            attempt,
            start_operation,
            conversation,
            goal,
            goal_attempt,
            goal_user_revision,
            required_input_generation,
            evidence_input_generation,
            settlement,
            unresolved_effects,
            frozen_unix_millis,
            reply_expected_revision,
            reply_operation,
            reply_after_invocation,
            reply_digest,
            reply_bytes,
            first_cause,
        );
        Ok(Self {
            identity,
            run,
            attempt,
            start_operation,
            conversation,
            goal,
            goal_attempt,
            goal_user_revision,
            required_input_generation,
            evidence_input_generation,
            settlement,
            unresolved_effects,
            frozen_unix_millis,
            reply_expected_revision,
            reply_operation,
            reply_after_invocation,
            reply_text,
            reply_digest,
            reply_bytes,
            first_cause,
            goal_step: ObligationStep::Pending,
            reply_step: ObligationStep::Pending,
        })
    }

    pub(super) const fn identity(&self) -> peritus_types::Sha256Digest {
        self.identity
    }

    pub(super) fn reply_reference(
        &self,
    ) -> Result<peritus_product_runner::control::PublicReplyReference, ProductRunServiceError> {
        let operation = peritus_product_runner::control::OperationId::new(self.reply_operation)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let invocation = peritus_product_runner::control::InvocationId::new(
            self.reply_after_invocation,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        peritus_product_runner::control::PublicReplyReference::new(
            operation,
            invocation,
            self.reply_digest.into_bytes(),
            self.reply_bytes,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) fn mark_goal_committed(&mut self) {
        self.goal_step = ObligationStep::Committed;
    }

    pub(super) fn mark_reply_committed(&mut self) {
        self.reply_step = ObligationStep::Committed;
    }

    pub(super) fn complete(&self) -> bool {
        self.goal_step != ObligationStep::Pending
            && self.reply_step != ObligationStep::Pending
    }

    pub(super) const fn goal_pending(&self) -> bool {
        matches!(self.goal_step, ObligationStep::Pending)
    }

    pub(super) const fn reply_pending(&self) -> bool {
        matches!(self.reply_step, ObligationStep::Pending)
    }

    fn valid(&self) -> bool {
        self.identity
            == obligation_identity(
                self.run,
                self.attempt,
                self.start_operation,
                self.conversation,
                self.goal,
                self.goal_attempt,
                self.goal_user_revision,
                self.required_input_generation,
                self.evidence_input_generation,
                self.settlement,
                self.unresolved_effects,
                self.frozen_unix_millis,
                self.reply_expected_revision,
                self.reply_operation,
                self.reply_after_invocation,
                self.reply_digest,
                self.reply_bytes,
                self.first_cause,
            )
            && u64::try_from(self.reply_text.len()).unwrap_or(u64::MAX) == self.reply_bytes
            && peritus_codec::sha256(self.reply_text.as_bytes()) == self.reply_digest
    }
}

impl SettlementObligation {
    pub(super) const fn identity(&self) -> peritus_types::Sha256Digest {
        match self {
            Self::Pending(root) | Self::Resolved(root) => root.identity,
            Self::LegacyAssessmentRequired(assessment)
            | Self::ResolvedWithLegacyGap(assessment) => assessment.identity,
        }
    }

    pub(super) fn binds(&self, run: peritus_types::RunId, attempt: u64) -> bool {
        match self {
            Self::Pending(root) | Self::Resolved(root) => {
                root.run == run && root.attempt == attempt && root.valid()
            }
            Self::LegacyAssessmentRequired(assessment)
            | Self::ResolvedWithLegacyGap(assessment) => {
                assessment.run == run && assessment.attempt == attempt && assessment.valid()
            }
        }
    }

    pub(super) const fn pending_root(&self) -> Option<&ObligationRoot> {
        match self {
            Self::Pending(root) => Some(root),
            _ => None,
        }
    }

    pub(super) const fn resolved(&self) -> bool {
        matches!(self, Self::Resolved(_) | Self::ResolvedWithLegacyGap(_))
    }

    pub(super) fn accepted_settlement(&self) -> bool {
        matches!(
            self,
            Self::Pending(root) | Self::Resolved(root)
                if root.settlement
                    == peritus_product_runner::control::GoalSettlement::Accepted as u16
        )
    }

    pub(super) fn resolve_legacy_gap(
        &mut self,
        identity: peritus_types::Sha256Digest,
    ) -> Result<(), ProductRunServiceError> {
        match self {
            Self::LegacyAssessmentRequired(assessment) if assessment.identity == identity => {
                assessment.goal_step = ObligationStep::EvidenceGap;
                assessment.reply_step = ObligationStep::EvidenceGap;
                *self = Self::ResolvedWithLegacyGap(assessment.clone());
                Ok(())
            }
            Self::ResolvedWithLegacyGap(assessment) if assessment.identity == identity => Ok(()),
            _ => Err(ProductRunServiceError::InvalidState),
        }
    }

    pub(super) fn mark_goal_committed(
        &mut self,
        identity: peritus_types::Sha256Digest,
    ) -> Result<(), ProductRunServiceError> {
        let Self::Pending(root) = self else {
            return if self.identity() == identity {
                Ok(())
            } else {
                Err(ProductRunServiceError::InvalidState)
            };
        };
        if root.identity != identity {
            return Err(ProductRunServiceError::InvalidState);
        }
        root.mark_goal_committed();
        if root.complete() {
            *self = Self::Resolved(root.clone());
        }
        Ok(())
    }

    pub(super) fn mark_reply_committed(
        &mut self,
        identity: peritus_types::Sha256Digest,
    ) -> Result<(), ProductRunServiceError> {
        let Self::Pending(root) = self else {
            return if self.identity() == identity {
                Ok(())
            } else {
                Err(ProductRunServiceError::InvalidState)
            };
        };
        if root.identity != identity {
            return Err(ProductRunServiceError::InvalidState);
        }
        root.mark_reply_committed();
        if root.complete() {
            *self = Self::Resolved(root.clone());
        }
        Ok(())
    }
}

impl LegacyAssessment {
    pub(super) fn new(
        source_format: u16,
        source_digest: peritus_types::Sha256Digest,
        run: peritus_types::RunId,
        attempt: u64,
        handoff: u64,
        incorporated: u64,
        evidence_digest: peritus_types::Sha256Digest,
    ) -> Self {
        Self {
            identity: legacy_assessment_identity(
                source_format,
                source_digest,
                run,
                attempt,
                handoff,
                incorporated,
                evidence_digest,
            ),
            source_format,
            source_digest,
            run,
            attempt,
            handoff,
            incorporated,
            evidence_digest,
            goal_step: ObligationStep::Pending,
            reply_step: ObligationStep::Pending,
        }
    }

    fn valid(&self) -> bool {
        (6..=12).contains(&self.source_format)
            && self.attempt != 0
            && self.identity
                == legacy_assessment_identity(
                    self.source_format,
                    self.source_digest,
                    self.run,
                    self.attempt,
                    self.handoff,
                    self.incorporated,
                    self.evidence_digest,
                )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PersistedObligation {
    Pending(PersistedObligationRoot),
    Resolved(PersistedObligationRoot),
    LegacyAssessmentRequired(PersistedLegacyAssessment),
    ResolvedWithLegacyGap(PersistedLegacyAssessment),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedObligationRoot {
    identity: [u8; 32],
    run: [u8; 16],
    attempt: u64,
    start_operation: [u8; 16],
    conversation: [u8; 16],
    goal: [u8; 16],
    goal_attempt: u64,
    goal_user_revision: u64,
    required_input_generation: Option<u64>,
    evidence_input_generation: Option<u64>,
    settlement: u16,
    unresolved_effects: bool,
    frozen_unix_millis: u64,
    #[serde(default)]
    reply_expected_revision: u64,
    reply_operation: [u8; 16],
    reply_after_invocation: [u8; 16],
    reply_text: String,
    reply_digest: [u8; 32],
    reply_bytes: u64,
    first_cause: ObligationFirstCause,
    goal_step: ObligationStep,
    reply_step: ObligationStep,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedLegacyAssessment {
    identity: [u8; 32],
    source_format: u16,
    source_digest: [u8; 32],
    run: [u8; 16],
    attempt: u64,
    handoff: u64,
    incorporated: u64,
    evidence_digest: [u8; 32],
    goal_step: ObligationStep,
    reply_step: ObligationStep,
}

impl PersistedObligation {
    pub(super) fn capture(value: &SettlementObligation) -> Self {
        match value {
            SettlementObligation::Pending(root) => Self::Pending(root.into()),
            SettlementObligation::Resolved(root) => Self::Resolved(root.into()),
            SettlementObligation::LegacyAssessmentRequired(assessment) => {
                Self::LegacyAssessmentRequired(assessment.into())
            }
            SettlementObligation::ResolvedWithLegacyGap(assessment) => {
                Self::ResolvedWithLegacyGap(assessment.into())
            }
        }
    }

    pub(super) fn restore(self) -> Result<SettlementObligation, ProductRunServiceError> {
        match self {
            Self::Pending(root) => {
                let root = root.restore()?;
                if root.complete() {
                    return Err(ProductRunServiceError::InvalidMessage);
                }
                Ok(SettlementObligation::Pending(root))
            }
            Self::Resolved(root) => {
                let root = root.restore()?;
                if !root.complete() {
                    return Err(ProductRunServiceError::InvalidMessage);
                }
                Ok(SettlementObligation::Resolved(root))
            }
            Self::LegacyAssessmentRequired(assessment) => {
                Ok(SettlementObligation::LegacyAssessmentRequired(assessment.restore()?))
            }
            Self::ResolvedWithLegacyGap(assessment) => {
                let assessment = assessment.restore()?;
                if assessment.goal_step == ObligationStep::Pending
                    || assessment.reply_step == ObligationStep::Pending
                {
                    return Err(ProductRunServiceError::InvalidMessage);
                }
                Ok(SettlementObligation::ResolvedWithLegacyGap(assessment))
            }
        }
    }
}

impl From<&ObligationRoot> for PersistedObligationRoot {
    fn from(value: &ObligationRoot) -> Self {
        Self {
            identity: value.identity.into_bytes(),
            run: value.run.into_bytes(),
            attempt: value.attempt,
            start_operation: value.start_operation,
            conversation: value.conversation,
            goal: value.goal,
            goal_attempt: value.goal_attempt,
            goal_user_revision: value.goal_user_revision,
            required_input_generation: value.required_input_generation,
            evidence_input_generation: value.evidence_input_generation,
            settlement: value.settlement,
            unresolved_effects: value.unresolved_effects,
            frozen_unix_millis: value.frozen_unix_millis,
            reply_expected_revision: value.reply_expected_revision,
            reply_operation: value.reply_operation,
            reply_after_invocation: value.reply_after_invocation,
            reply_text: value.reply_text.clone(),
            reply_digest: value.reply_digest.into_bytes(),
            reply_bytes: value.reply_bytes,
            first_cause: value.first_cause,
            goal_step: value.goal_step,
            reply_step: value.reply_step,
        }
    }
}

impl PersistedObligationRoot {
    fn restore(self) -> Result<ObligationRoot, ProductRunServiceError> {
        let root = ObligationRoot {
            identity: peritus_types::Sha256Digest::new(self.identity),
            run: peritus_types::RunId::new(self.run)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            attempt: self.attempt,
            start_operation: self.start_operation,
            conversation: self.conversation,
            goal: self.goal,
            goal_attempt: self.goal_attempt,
            goal_user_revision: self.goal_user_revision,
            required_input_generation: self.required_input_generation,
            evidence_input_generation: self.evidence_input_generation,
            settlement: self.settlement,
            unresolved_effects: self.unresolved_effects,
            frozen_unix_millis: self.frozen_unix_millis,
            reply_expected_revision: self.reply_expected_revision,
            reply_operation: self.reply_operation,
            reply_after_invocation: self.reply_after_invocation,
            reply_text: self.reply_text,
            reply_digest: peritus_types::Sha256Digest::new(self.reply_digest),
            reply_bytes: self.reply_bytes,
            first_cause: self.first_cause,
            goal_step: self.goal_step,
            reply_step: self.reply_step,
        };
        if !root.valid() {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        Ok(root)
    }
}

impl From<&LegacyAssessment> for PersistedLegacyAssessment {
    fn from(value: &LegacyAssessment) -> Self {
        Self {
            identity: value.identity.into_bytes(),
            source_format: value.source_format,
            source_digest: value.source_digest.into_bytes(),
            run: value.run.into_bytes(),
            attempt: value.attempt,
            handoff: value.handoff,
            incorporated: value.incorporated,
            evidence_digest: value.evidence_digest.into_bytes(),
            goal_step: value.goal_step,
            reply_step: value.reply_step,
        }
    }
}

impl PersistedLegacyAssessment {
    fn restore(self) -> Result<LegacyAssessment, ProductRunServiceError> {
        let source_digest = peritus_types::Sha256Digest::new(self.source_digest);
        let evidence_digest = peritus_types::Sha256Digest::new(self.evidence_digest);
        let assessment = LegacyAssessment {
            identity: peritus_types::Sha256Digest::new(self.identity),
            source_format: self.source_format,
            source_digest,
            run: peritus_types::RunId::new(self.run)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            attempt: self.attempt,
            handoff: self.handoff,
            incorporated: self.incorporated,
            evidence_digest,
            goal_step: self.goal_step,
            reply_step: self.reply_step,
        };
        if !assessment.valid() {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        Ok(assessment)
    }
}

fn legacy_assessment_identity(
    source_format: u16,
    source_digest: peritus_types::Sha256Digest,
    run: peritus_types::RunId,
    attempt: u64,
    handoff: u64,
    incorporated: u64,
    evidence_digest: peritus_types::Sha256Digest,
) -> peritus_types::Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-run-legacy-obligation-v1\0");
    hasher.update(source_format.to_be_bytes());
    hasher.update(source_digest.into_bytes());
    hasher.update(run.as_bytes());
    hasher.update(attempt.to_be_bytes());
    hasher.update(handoff.to_be_bytes());
    hasher.update(incorporated.to_be_bytes());
    hasher.update(evidence_digest.into_bytes());
    peritus_types::Sha256Digest::new(hasher.finalize().into())
}

#[allow(clippy::too_many_arguments)]
fn obligation_identity(
    run: peritus_types::RunId,
    attempt: u64,
    start_operation: [u8; 16],
    conversation: [u8; 16],
    goal: [u8; 16],
    goal_attempt: u64,
    goal_user_revision: u64,
    required_input_generation: Option<u64>,
    evidence_input_generation: Option<u64>,
    settlement: u16,
    unresolved_effects: bool,
    frozen_unix_millis: u64,
    reply_expected_revision: u64,
    reply_operation: [u8; 16],
    reply_after_invocation: [u8; 16],
    reply_digest: peritus_types::Sha256Digest,
    reply_bytes: u64,
    first_cause: ObligationFirstCause,
) -> peritus_types::Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-run-terminal-obligation-v1\0");
    hasher.update(run.as_bytes());
    hasher.update(attempt.to_be_bytes());
    hasher.update(start_operation);
    hasher.update(conversation);
    hasher.update(goal);
    hasher.update(goal_attempt.to_be_bytes());
    hasher.update(goal_user_revision.to_be_bytes());
    match required_input_generation {
        Some(generation) => {
            hasher.update([1]);
            hasher.update(generation.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    match evidence_input_generation {
        Some(generation) => {
            hasher.update([1]);
            hasher.update(generation.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.update(settlement.to_be_bytes());
    hasher.update([u8::from(unresolved_effects)]);
    hasher.update(frozen_unix_millis.to_be_bytes());
    hasher.update(reply_expected_revision.to_be_bytes());
    hasher.update(reply_operation);
    hasher.update(reply_after_invocation);
    hasher.update(reply_digest.into_bytes());
    hasher.update(reply_bytes.to_be_bytes());
    hasher.update([first_cause as u8]);
    peritus_types::Sha256Digest::new(hasher.finalize().into())
}
