//! B1 reservation planning and cumulative model/tool usage settlement.

mod port;

pub use port::{AgentBudgetError, AgentBudgetPort, AgentBudgetPortError};

use peritus_budget::{
    Activation, AmbiguousFinalization, BudgetAmounts, BudgetCommand, BudgetDimension,
    BudgetOperation, BudgetReceipt, BudgetRequest, ReservationReference, UsageFinality,
    UsageObservation,
};
use peritus_model_protocol::UsageCounters;
use peritus_types::{ActionId, BudgetId, BudgetReservationId, RevisionTuple, Sha256Digest};

/// Immutable B1 request plan for one model attempt or tool invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentBudgetPlan {
    reservation_id: BudgetReservationId,
    budget_id: BudgetId,
    revision: RevisionTuple,
    action_id: ActionId,
    action_digest: Sha256Digest,
    ceiling: BudgetAmounts,
    retry: bool,
}

impl AgentBudgetPlan {
    /// Binds one fresh reservation to its exact action and maximum variable usage.
    ///
    /// # Errors
    ///
    /// Rejects an empty ceiling or one that incorrectly includes attempt/retry charges, which D0
    /// always supplies through B1's immediate `Begin` consumption instead.
    #[allow(clippy::too_many_arguments, reason = "all B1 authority bindings remain explicit")]
    pub const fn new(
        reservation_id: BudgetReservationId,
        budget_id: BudgetId,
        revision: RevisionTuple,
        action_id: ActionId,
        action_digest: Sha256Digest,
        ceiling: BudgetAmounts,
        retry: bool,
    ) -> Result<Self, AgentBudgetError> {
        if ceiling.is_zero()
            || ceiling.get(BudgetDimension::Attempts).get() != 0
            || ceiling.get(BudgetDimension::Retries).get() != 0
        {
            return Err(AgentBudgetError::InvalidPlan);
        }
        Ok(Self { reservation_id, budget_id, revision, action_id, action_digest, ceiling, retry })
    }

    /// Returns the fresh reservation identity.
    #[must_use]
    pub const fn reservation_id(self) -> BudgetReservationId {
        self.reservation_id
    }

    /// Returns the charged budget account.
    #[must_use]
    pub const fn budget_id(self) -> BudgetId {
        self.budget_id
    }

    /// Returns the immutable authority revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }

    /// Returns the action identity shared by retry attempts in the same lineage.
    #[must_use]
    pub const fn action_id(self) -> ActionId {
        self.action_id
    }

    /// Returns the exact action-content digest.
    #[must_use]
    pub const fn action_digest(self) -> Sha256Digest {
        self.action_digest
    }

    /// Returns the maximum variable usage reserved for this attempt.
    #[must_use]
    pub const fn ceiling(self) -> BudgetAmounts {
        self.ceiling
    }

    /// Returns whether B1 must charge this attempt as a retry.
    #[must_use]
    pub const fn is_retry(self) -> bool {
        self.retry
    }

    const fn request(self) -> BudgetRequest {
        BudgetRequest::new(
            self.reservation_id,
            self.budget_id,
            self.revision,
            self.action_id,
            self.action_digest,
            BudgetAmounts::from_units(0, 0, 0, 1, if self.retry { 1 } else { 0 }),
            self.ceiling,
        )
    }
}

/// Local observation of the externally durable B1 reservation lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentBudgetState {
    Held,
    Active,
    Settled,
    Cancelled,
    Indeterminate,
}

/// Checked handle used to enforce B1 accounting around one external operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentBudgetReservation {
    plan: AgentBudgetPlan,
    state: AgentBudgetState,
    last_usage: BudgetAmounts,
}

impl AgentBudgetReservation {
    /// Commits the attempt/retry charge and variable-use ceiling before any effect may begin.
    ///
    /// # Errors
    ///
    /// Returns the durable port failure or rejects a mismatched receipt.
    pub fn begin(
        port: &mut dyn AgentBudgetPort,
        plan: AgentBudgetPlan,
    ) -> Result<Self, AgentBudgetError> {
        let receipt = port.commit(BudgetCommand::Begin(plan.request()))?;
        validate_receipt(receipt, BudgetOperation::Begin, plan)?;
        Ok(Self { plan, state: AgentBudgetState::Held, last_usage: BudgetAmounts::zero() })
    }

    /// Returns the exact immutable request plan.
    #[must_use]
    pub const fn plan(self) -> AgentBudgetPlan {
        self.plan
    }

    /// Returns the last successfully committed lifecycle observation.
    #[must_use]
    pub const fn state(self) -> AgentBudgetState {
        self.state
    }

    /// Returns whether exact final usage or conservative ambiguity has closed the reservation.
    #[must_use]
    pub const fn is_settled(self) -> bool {
        matches!(self.state, AgentBudgetState::Settled | AgentBudgetState::Indeterminate)
    }

    /// Returns the last cumulative usage committed through B1.
    #[must_use]
    pub const fn last_usage(self) -> BudgetAmounts {
        self.last_usage
    }

    /// Commits evidence that the reserved operation became externally active.
    ///
    /// # Errors
    ///
    /// Rejects any phase other than held and validates the returned B1 receipt.
    pub fn activate(
        &mut self,
        port: &mut dyn AgentBudgetPort,
        evidence_digest: Sha256Digest,
    ) -> Result<BudgetReceipt, AgentBudgetError> {
        if self.state != AgentBudgetState::Held {
            return Err(AgentBudgetError::InvalidPhase);
        }
        let command = BudgetCommand::Activate(Activation::new(
            self.plan.reservation_id,
            self.plan.action_id,
            self.plan.action_digest,
            evidence_digest,
        ));
        let receipt = port.commit(command)?;
        validate_receipt(receipt, BudgetOperation::Activate, self.plan)?;
        self.state = AgentBudgetState::Active;
        Ok(receipt)
    }

    /// Reconciles normalized C5 high-water usage and accountable active time through B1.
    ///
    /// Final observations must contain every dimension with a nonzero reservation ceiling. If a
    /// provider omits one, the caller must conservatively call [`Self::finalize_ambiguous`].
    ///
    /// # Errors
    ///
    /// Rejects non-active use, incomplete final usage, arithmetic overflow, port failure, or a
    /// mismatched receipt.
    pub fn observe_model(
        &mut self,
        port: &mut dyn AgentBudgetPort,
        evidence_digest: Sha256Digest,
        usage: UsageCounters,
        active_effect_milliseconds: u64,
        finality: UsageFinality,
    ) -> Result<BudgetReceipt, AgentBudgetError> {
        if self.state != AgentBudgetState::Active {
            return Err(AgentBudgetError::InvalidPhase);
        }
        let cumulative =
            model_amounts(usage, active_effect_milliseconds, self.plan.ceiling, finality)?;
        self.observe(port, evidence_digest, cumulative, finality)
    }

    /// Reconciles cumulative active time for a non-model C4 effect.
    ///
    /// # Errors
    ///
    /// Rejects non-active use, port failure, or a mismatched receipt.
    pub fn observe_effect(
        &mut self,
        port: &mut dyn AgentBudgetPort,
        evidence_digest: Sha256Digest,
        active_effect_milliseconds: u64,
        finality: UsageFinality,
    ) -> Result<BudgetReceipt, AgentBudgetError> {
        if self.state != AgentBudgetState::Active {
            return Err(AgentBudgetError::InvalidPhase);
        }
        self.observe(
            port,
            evidence_digest,
            BudgetAmounts::from_units(0, 0, active_effect_milliseconds, 0, 0),
            finality,
        )
    }

    /// Conservatively consumes all unobserved capacity when an active outcome is uncertain.
    ///
    /// # Errors
    ///
    /// Rejects non-active use, port failure, or a mismatched receipt.
    pub fn finalize_ambiguous(
        &mut self,
        port: &mut dyn AgentBudgetPort,
        evidence_digest: Sha256Digest,
    ) -> Result<BudgetReceipt, AgentBudgetError> {
        if self.state != AgentBudgetState::Active {
            return Err(AgentBudgetError::InvalidPhase);
        }
        let reference = self.reference(evidence_digest);
        let receipt =
            port.commit(BudgetCommand::FinalizeAmbiguous(AmbiguousFinalization::new(reference)))?;
        validate_receipt(receipt, BudgetOperation::FinalizeAmbiguous, self.plan)?;
        self.state = AgentBudgetState::Indeterminate;
        Ok(receipt)
    }

    /// Requests release of a reservation proven never to have activated by the C0 port.
    ///
    /// The handle alone is not authority for that negative fact; the port must independently
    /// validate it against committed effect intent.
    ///
    /// # Errors
    ///
    /// Rejects non-held use, port failure, or a mismatched receipt.
    pub fn cancel_held(
        &mut self,
        port: &mut dyn AgentBudgetPort,
        evidence_digest: Sha256Digest,
    ) -> Result<BudgetReceipt, AgentBudgetError> {
        if self.state != AgentBudgetState::Held {
            return Err(AgentBudgetError::InvalidPhase);
        }
        let receipt = port.commit(BudgetCommand::CancelHeld(self.reference(evidence_digest)))?;
        validate_receipt(receipt, BudgetOperation::CancelHeld, self.plan)?;
        self.state = AgentBudgetState::Cancelled;
        Ok(receipt)
    }

    fn observe(
        &mut self,
        port: &mut dyn AgentBudgetPort,
        evidence_digest: Sha256Digest,
        cumulative: BudgetAmounts,
        finality: UsageFinality,
    ) -> Result<BudgetReceipt, AgentBudgetError> {
        let command = BudgetCommand::ObserveUsage(UsageObservation::new(
            self.plan.reservation_id,
            self.plan.action_id,
            self.plan.action_digest,
            evidence_digest,
            cumulative,
            finality,
        ));
        let receipt = port.commit(command)?;
        validate_receipt(receipt, BudgetOperation::ObserveUsage, self.plan)?;
        self.last_usage = cumulative;
        if finality == UsageFinality::Final {
            self.state = AgentBudgetState::Settled;
        }
        Ok(receipt)
    }

    const fn reference(self, evidence_digest: Sha256Digest) -> ReservationReference {
        ReservationReference::new(
            self.plan.reservation_id,
            self.plan.action_id,
            self.plan.action_digest,
            evidence_digest,
        )
    }
}

fn validate_receipt(
    receipt: BudgetReceipt,
    operation: BudgetOperation,
    plan: AgentBudgetPlan,
) -> Result<(), AgentBudgetError> {
    if receipt.operation() != operation
        || receipt.budget_id() != plan.budget_id
        || receipt.reservation_id() != Some(plan.reservation_id)
    {
        Err(AgentBudgetError::ReceiptMismatch)
    } else {
        Ok(())
    }
}

fn model_amounts(
    usage: UsageCounters,
    active_effect_milliseconds: u64,
    ceiling: BudgetAmounts,
    finality: UsageFinality,
) -> Result<BudgetAmounts, AgentBudgetError> {
    let tokens = match usage.total_tokens() {
        Some(total) => total,
        None => match (usage.input_tokens(), usage.output_tokens()) {
            (Some(input), Some(output)) => input
                .checked_add(output)
                .and_then(|subtotal| {
                    usage.tool_tokens().map_or(Some(subtotal), |tools| subtotal.checked_add(tools))
                })
                .ok_or(AgentBudgetError::UsageOverflow)?,
            _ if finality == UsageFinality::Final
                && ceiling.get(BudgetDimension::ModelTokens).get() > 0 =>
            {
                return Err(AgentBudgetError::IncompleteFinalUsage);
            }
            _ => 0,
        },
    };
    let cost = usage.provider_cost_microunits().unwrap_or(0);
    if finality == UsageFinality::Final
        && ceiling.get(BudgetDimension::ProviderCostMicrounits).get() > 0
        && usage.provider_cost_microunits().is_none()
    {
        return Err(AgentBudgetError::IncompleteFinalUsage);
    }
    Ok(BudgetAmounts::from_units(tokens, cost, active_effect_milliseconds, 0, 0))
}
