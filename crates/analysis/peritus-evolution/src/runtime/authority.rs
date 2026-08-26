//! Exact observation of durable B0/B1 and approve-once promotion authority.

use peritus_approval::{ApprovalPhase, ApprovalUseOutcome};
use peritus_journal::{CommittedCapabilityUse, CommittedKernelTransition};
use peritus_kernel::ActionPhase;
use peritus_policy::{RiskClass, RiskSet};
use peritus_types::{ActionId, Sha256Digest};

use crate::{
    ActivationAuthorization, EvolutionError, EvolutionErrorKind, EvolutionOperation,
    EvolutionRecovery,
};

/// Required exact capability for production harness promotion and rollback.
pub const HARNESS_PROMOTION_CAPABILITY: &str = "harness.promote";

/// Borrowed durable authority observations for one immutable action.
pub struct PromotionAuthorityRequest<'a> {
    action_id: ActionId,
    action_digest: Sha256Digest,
    kernel: &'a CommittedKernelTransition,
    capability: &'a CommittedCapabilityUse,
    approval: &'a ApprovalUseOutcome,
}

impl<'a> PromotionAuthorityRequest<'a> {
    /// Binds exact durable observations without granting authority by construction.
    #[must_use]
    pub const fn new(
        action_id: ActionId,
        action_digest: Sha256Digest,
        kernel: &'a CommittedKernelTransition,
        capability: &'a CommittedCapabilityUse,
        approval: &'a ApprovalUseOutcome,
    ) -> Self {
        Self { action_id, action_digest, kernel, capability, approval }
    }
}

/// Copyable non-authorizing facts admitted into the verified pointer reducer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PromotionAuthority {
    authorization: ActivationAuthorization,
}

impl PromotionAuthority {
    /// Validates every durable action, capability, approval, risk, and registry binding.
    ///
    /// # Errors
    /// Rejects absent dispatch, wrong capability/risk/scope, stale registry facts, or an approval
    /// for another action.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the request is a short-lived ownership boundary over borrowed authority observations"
    )]
    pub fn capture(request: PromotionAuthorityRequest<'_>) -> Result<Self, EvolutionError> {
        let action = request
            .kernel
            .aggregate()
            .action(request.action_id)
            .ok_or_else(|| authority("B0 durable aggregate has no matching action"))?;
        let capability = request.capability.transition();
        let witness = action
            .authorization()
            .ok_or_else(|| authority("dispatched B0 action has no capability witness"))?;
        let approval = request.approval.aggregate();
        let approval_request = approval.request();
        let approval_transition = request.approval.transition();
        let resolution = approval
            .resolution()
            .ok_or_else(|| authority("consumed approval has no resolution facts"))?;
        let harness_risk = RiskSet::new(vec![RiskClass::HarnessPromotion])
            .map_err(|_| authority("HarnessPromotion risk construction failed"))?;
        if action.phase() != ActionPhase::Dispatched
            || action.id() != request.action_id
            || action.digest() != request.action_digest
            || capability.action_id() != request.action_id
            || capability.action_digest() != request.action_digest
            || capability.permission().capability_name().as_str() != HARNESS_PROMOTION_CAPABILITY
            || witness.transition_digest() != capability.transition_digest()
            || witness.resource_id() != capability.permission().resource_id()
            || witness.capability_name() != capability.permission().capability_name()
            || approval.phase() != ApprovalPhase::Consumed
            || approval_request.action_id() != request.action_id
            || approval_request.action_digest().sha256() != request.action_digest
            || approval_request.risks() != &harness_risk
            || approval_transition.action_id() != request.action_id
            || approval_transition.action_digest().sha256() != request.action_digest
            || approval_transition.revision() != capability.scope().revision()
            || resolution.registry_revision() != approval_transition.registry_revision()
        {
            return Err(authority("B0, B1, approval, or HarnessPromotion risk binding differs"));
        }
        let approval_digest = approval_use_digest(request.approval);
        let authority_digest = authority_binding_digest(
            request.kernel.state_digest(),
            request.capability.state_digest(),
            approval_request.digest().sha256(),
            resolution.registry_digest(),
            resolution.registry_revision().get(),
        );
        Ok(Self {
            authorization: ActivationAuthorization::new(
                request.action_digest,
                request.kernel.state_digest(),
                capability.transition_digest(),
                approval_digest,
                authority_digest,
            ),
        })
    }

    /// Exact checked facts accepted by the pointer reducer.
    #[must_use]
    pub const fn authorization(self) -> ActivationAuthorization {
        self.authorization
    }
}

pub(crate) fn approval_use_digest(outcome: &ApprovalUseOutcome) -> Sha256Digest {
    let transition = outcome.transition();
    let consumed = outcome.consumed();
    let mut bytes = b"PERITUS-F0-APPROVAL-USE\0".to_vec();
    bytes.extend_from_slice(transition.request_id().as_bytes());
    bytes.extend_from_slice(transition.request_digest().sha256().as_bytes());
    bytes.extend_from_slice(transition.action_id().as_bytes());
    bytes.extend_from_slice(transition.action_digest().sha256().as_bytes());
    bytes.extend_from_slice(transition.decision_digest().sha256().as_bytes());
    bytes.extend_from_slice(transition.command_id().as_bytes());
    bytes.extend_from_slice(&transition.registry_revision().get().to_be_bytes());
    bytes.extend_from_slice(consumed.request_id().as_bytes());
    bytes.extend_from_slice(consumed.decision_digest().sha256().as_bytes());
    bytes.extend_from_slice(consumed.action_id().as_bytes());
    peritus_codec::sha256(&bytes)
}

fn authority_binding_digest(
    kernel: Sha256Digest,
    capability: Sha256Digest,
    approval: Sha256Digest,
    registry: Sha256Digest,
    registry_revision: u64,
) -> Sha256Digest {
    let mut bytes = b"PERITUS-F0-AUTHORITY-BINDING\0".to_vec();
    bytes.extend_from_slice(kernel.as_bytes());
    bytes.extend_from_slice(capability.as_bytes());
    bytes.extend_from_slice(approval.as_bytes());
    bytes.extend_from_slice(registry.as_bytes());
    bytes.extend_from_slice(&registry_revision.to_be_bytes());
    peritus_codec::sha256(&bytes)
}

const fn authority(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Authority,
        EvolutionOperation::Authorize,
        EvolutionRecovery::RequestAuthority,
        detail,
    )
}
