//! Production adapters for the closed E0-E3 and F0 destination inventory.

use std::{future::Future, pin::Pin, sync::Arc};

use peritus_journal::OutboxMessage;
use peritus_orchestrator::DirectiveDestination;

use super::{
    DurableOutboxClaim, DurableOutboxPort, OrchestratorDirectiveClaim, TypedOutboxClaim,
    router::OutboxHandler,
};
use crate::{AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn child_directive_handlers(
    authority: &AuthorityHandle,
) -> Vec<(String, Arc<dyn OutboxHandler>)> {
    [
        DirectiveDestination::Gates,
        DirectiveDestination::Review,
        DirectiveDestination::Scheduler,
        DirectiveDestination::Collaboration,
    ]
    .into_iter()
    .map(|destination| {
        let handler: Arc<dyn OutboxHandler> =
            Arc::new(ChildDirectiveHandler { authority: authority.clone(), destination });
        (destination.outbox_destination().to_owned(), handler)
    })
    .collect()
}

/// Builds all production handlers beyond the authority's existing D1/D2 lifecycle adapters.
pub(super) fn durable_domain_handlers(
    port: Arc<dyn DurableOutboxPort>,
) -> Vec<(String, Arc<dyn OutboxHandler>)> {
    DurableDestination::ALL
        .into_iter()
        .map(|destination| {
            let handler: Arc<dyn OutboxHandler> =
                Arc::new(DurableDomainHandler { port: Arc::clone(&port), destination });
            (destination.name().to_owned(), handler)
        })
        .collect()
}

struct ChildDirectiveHandler {
    authority: AuthorityHandle,
    destination: DirectiveDestination,
}

impl OutboxHandler for ChildDirectiveHandler {
    fn deliver<'a>(
        &'a self,
        _message: &'a OutboxMessage,
        claim: &'a TypedOutboxClaim,
    ) -> Pin<Box<dyn Future<Output = Result<bool, DaemonError>> + Send + 'a>> {
        Box::pin(async move {
            let claim = self.exact_claim(claim)?.clone();
            self.authority.deliver_orchestrator_child(claim).await?;
            Ok(false)
        })
    }
}

impl ChildDirectiveHandler {
    fn exact_claim<'a>(
        &self,
        claim: &'a TypedOutboxClaim,
    ) -> Result<&'a OrchestratorDirectiveClaim, DaemonError> {
        match (self.destination, claim) {
            (DirectiveDestination::Gates, TypedOutboxClaim::OrchestratorGates(claim))
            | (DirectiveDestination::Review, TypedOutboxClaim::OrchestratorReview(claim))
            | (DirectiveDestination::Scheduler, TypedOutboxClaim::OrchestratorScheduler(claim))
            | (
                DirectiveDestination::Collaboration,
                TypedOutboxClaim::OrchestratorCollaboration(claim),
            ) => Ok(claim),
            _ => Err(DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::ReadOnly,
                "deliver E0 child directive",
                "outbox router supplied a typed claim for another destination",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DurableDestination {
    OrchestratorAgent,
    OrchestratorQualityEvaluator,
    OrchestratorKernel,
    HarnessMaterialization,
    DebuggerModel,
    DebuggerPublication,
    EvaluationSchedule,
    EvaluationExecution,
    EvaluationPublication,
    EvolutionPublication,
}

impl DurableDestination {
    const ALL: [Self; 10] = [
        Self::OrchestratorAgent,
        Self::OrchestratorQualityEvaluator,
        Self::OrchestratorKernel,
        Self::HarnessMaterialization,
        Self::DebuggerModel,
        Self::DebuggerPublication,
        Self::EvaluationSchedule,
        Self::EvaluationExecution,
        Self::EvaluationPublication,
        Self::EvolutionPublication,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::OrchestratorAgent => DirectiveDestination::Agent.outbox_destination(),
            Self::OrchestratorQualityEvaluator => {
                DirectiveDestination::QualityEvaluator.outbox_destination()
            }
            Self::OrchestratorKernel => DirectiveDestination::Kernel.outbox_destination(),
            Self::HarnessMaterialization => super::claims::HARNESS_MATERIALIZATION_DESTINATION,
            Self::DebuggerModel => peritus_debugger::MODEL_ANALYSIS_DESTINATION,
            Self::DebuggerPublication => peritus_debugger::PUBLICATION_DESTINATION,
            Self::EvaluationSchedule => peritus_eval::SCHEDULE_DESTINATION,
            Self::EvaluationExecution => peritus_eval::EXECUTION_DESTINATION,
            Self::EvaluationPublication => peritus_eval::PUBLICATION_DESTINATION,
            Self::EvolutionPublication => peritus_evolution::EVOLUTION_PUBLICATION_DESTINATION,
        }
    }

    fn bind(self, claim: &TypedOutboxClaim) -> Option<DurableOutboxClaim> {
        match (self, claim) {
            (Self::OrchestratorAgent, TypedOutboxClaim::OrchestratorAgent(claim)) => {
                Some(DurableOutboxClaim::OrchestratorAgent(claim.clone()))
            }
            (
                Self::OrchestratorQualityEvaluator,
                TypedOutboxClaim::OrchestratorQualityEvaluator(claim),
            ) => Some(DurableOutboxClaim::OrchestratorQualityEvaluator(claim.clone())),
            (Self::OrchestratorKernel, TypedOutboxClaim::OrchestratorKernel(claim)) => {
                Some(DurableOutboxClaim::OrchestratorKernel(claim.clone()))
            }
            (Self::HarnessMaterialization, TypedOutboxClaim::HarnessMaterialization(claim)) => {
                Some(DurableOutboxClaim::HarnessMaterialization(claim.clone()))
            }
            (Self::DebuggerModel, TypedOutboxClaim::DebuggerModel(claim)) => {
                Some(DurableOutboxClaim::DebuggerModel(*claim))
            }
            (Self::DebuggerPublication, TypedOutboxClaim::DebuggerPublication(claim)) => {
                Some(DurableOutboxClaim::DebuggerPublication(*claim))
            }
            (Self::EvaluationSchedule, TypedOutboxClaim::EvaluationSchedule(claim)) => {
                Some(DurableOutboxClaim::EvaluationSchedule(claim.clone()))
            }
            (Self::EvaluationExecution, TypedOutboxClaim::EvaluationExecution(claim)) => {
                Some(DurableOutboxClaim::EvaluationExecution(claim.clone()))
            }
            (Self::EvaluationPublication, TypedOutboxClaim::EvaluationPublication(claim)) => {
                Some(DurableOutboxClaim::EvaluationPublication(claim.clone()))
            }
            (Self::EvolutionPublication, TypedOutboxClaim::EvolutionPublication(claim)) => {
                Some(DurableOutboxClaim::EvolutionPublication(claim.clone()))
            }
            _ => None,
        }
    }
}

struct DurableDomainHandler {
    port: Arc<dyn DurableOutboxPort>,
    destination: DurableDestination,
}

impl OutboxHandler for DurableDomainHandler {
    fn deliver<'a>(
        &'a self,
        _message: &'a OutboxMessage,
        claim: &'a TypedOutboxClaim,
    ) -> Pin<Box<dyn Future<Output = Result<bool, DaemonError>> + Send + 'a>> {
        Box::pin(async move {
            let claim = self.destination.bind(claim).ok_or_else(|| {
                DaemonError::new(
                    DaemonErrorCode::CorruptState,
                    DaemonRecovery::ReadOnly,
                    "deliver durable domain directive",
                    "outbox router supplied a typed claim for another destination",
                )
            })?;
            self.port.deliver_and_settle(claim).await?;
            Ok(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    struct SuccessfulPort;

    impl DurableOutboxPort for SuccessfulPort {
        fn deliver_and_settle<'a>(
            &'a self,
            _claim: DurableOutboxClaim,
        ) -> super::super::DurableDelivery<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn durable_handlers_cover_every_non_child_destination_once() {
        let handlers = durable_domain_handlers(Arc::new(SuccessfulPort));
        let names = handlers.iter().map(|(name, _)| name.as_str()).collect::<BTreeSet<_>>();
        let mut complete = names.clone();
        complete.insert(DirectiveDestination::Gates.outbox_destination());
        complete.insert(DirectiveDestination::Review.outbox_destination());
        complete.insert(DirectiveDestination::Scheduler.outbox_destination());
        complete.insert(DirectiveDestination::Collaboration.outbox_destination());
        let expected = super::super::CLAIM_DESTINATIONS.into_iter().collect::<BTreeSet<_>>();

        assert_eq!(handlers.len(), DurableDestination::ALL.len());
        assert_eq!(names.len(), DurableDestination::ALL.len());
        assert!(!names.contains(DirectiveDestination::Gates.outbox_destination()));
        assert!(!names.contains(DirectiveDestination::Review.outbox_destination()));
        assert!(!names.contains(DirectiveDestination::Scheduler.outbox_destination()));
        assert!(!names.contains(DirectiveDestination::Collaboration.outbox_destination()));
        assert_eq!(complete, expected);
        for destination in DurableDestination::ALL {
            assert!(names.contains(destination.name()));
        }
    }
}
