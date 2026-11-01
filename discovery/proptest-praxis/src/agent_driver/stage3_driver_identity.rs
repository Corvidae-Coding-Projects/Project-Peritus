//! Deterministically derived nominal identities for one product command action.

use peritus_journal::{AggregateId, StoreId};
use peritus_spec::AcceptanceContract;
use peritus_types::{
    ActionId, ActorId, AttemptId, BudgetId, BudgetReservationId, CapabilityName, CommandId,
    EnvironmentId, EventId, HarnessId, PolicyId, ProjectId, ProviderProfileId, ResourceId,
    RevisionNumber, RevisionTuple, RunId, SessionId, TurnId, WorkspaceId,
};

use super::contract;

pub(crate) struct CommandIds {
    pub(crate) workspace: WorkspaceId,
    pub(crate) resource: ResourceId,
    pub(crate) environment: EnvironmentId,
    pub(crate) actor: ActorId,
    pub(crate) session: SessionId,
    pub(crate) action: ActionId,
    pub(crate) capability: CapabilityName,
    pub(crate) revision: RevisionTuple,
    pub(crate) project: ProjectId,
    pub(crate) run: RunId,
    pub(crate) attempt: AttemptId,
    pub(crate) turn: TurnId,
    pub(crate) kernel_root_budget: BudgetId,
    pub(crate) kernel_child_budget: BudgetId,
    pub(crate) effect_budget: BudgetId,
    pub(crate) reservation: BudgetReservationId,
    source_run: RunId,
    ordinal: u64,
}

impl CommandIds {
    pub(crate) fn new(
        source_run: RunId,
        ordinal: u64,
        acceptance: &AcceptanceContract,
    ) -> Result<Self, String> {
        let workspace = nominal(WorkspaceId::new, source_run, ordinal, "workspace")?;
        let revision = RevisionTuple::new(
            acceptance.id(),
            nominal(HarnessId::new, source_run, ordinal, "harness")?,
            workspace,
            peritus_types::Generation::first(),
            RevisionNumber::first(),
            nominal(PolicyId::new, source_run, ordinal, "policy")?,
            nominal(ProviderProfileId::new, source_run, ordinal, "provider")?,
        );
        Ok(Self {
            workspace,
            resource: nominal(ResourceId::new, source_run, ordinal, "resource")?,
            environment: nominal(EnvironmentId::new, source_run, ordinal, "environment")?,
            actor: nominal(ActorId::new, source_run, ordinal, "actor")?,
            session: nominal(SessionId::new, source_run, ordinal, "session")?,
            action: nominal(ActionId::new, source_run, ordinal, "action")?,
            capability: CapabilityName::new("shell.exec".to_owned())
                .map_err(|error| format!("construct command capability: {error:?}"))?,
            revision,
            project: nominal(ProjectId::new, source_run, ordinal, "project")?,
            run: nominal(RunId::new, source_run, ordinal, "execution-run")?,
            attempt: nominal(AttemptId::new, source_run, ordinal, "attempt")?,
            turn: nominal(TurnId::new, source_run, ordinal, "turn")?,
            kernel_root_budget: nominal(BudgetId::new, source_run, ordinal, "kernel-root-budget")?,
            kernel_child_budget: nominal(
                BudgetId::new,
                source_run,
                ordinal,
                "kernel-child-budget",
            )?,
            effect_budget: nominal(BudgetId::new, source_run, ordinal, "effect-budget")?,
            reservation: nominal(BudgetReservationId::new, source_run, ordinal, "reservation")?,
            source_run,
            ordinal,
        })
    }

    pub(crate) fn command(&self, label: &str) -> Result<CommandId, String> {
        nominal(CommandId::new, self.source_run, self.ordinal, label)
    }

    pub(crate) fn event(&self, label: &str) -> Result<EventId, String> {
        nominal(EventId::new, self.source_run, self.ordinal, label)
    }

    pub(crate) fn aggregate(&self, label: &str) -> Result<AggregateId, String> {
        nominal(AggregateId::new, self.source_run, self.ordinal, label)
    }

    pub(crate) fn store(&self, label: &str) -> Result<StoreId, String> {
        nominal(StoreId::new, self.source_run, self.ordinal, label)
    }
}

fn nominal<T, E>(
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
    run_id: RunId,
    ordinal: u64,
    label: &str,
) -> Result<T, String>
where
    E: std::fmt::Debug,
{
    constructor(contract::id(run_id, ordinal, label))
        .map_err(|error| format!("construct product command identity {label}: {error:?}"))
}
