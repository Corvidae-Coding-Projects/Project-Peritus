//! One live projection of authoritative run, receipt, and deliverable owners.

use peritus_app_protocol::{
    ProductRunControlAction as Action, ProductRunLegalControls, ProductRunOperation,
    ProductRunOperationKind as Kind, ProductRunOperationState as State, ProductRunPhase,
};
use peritus_product_runner::{
    DiscardTransactionState, UncertainEffectState, acknowledge_uncertain_effect, uncertain_effects,
};
use std::{path::Path, sync::Arc};

use crate::diagnostic;

use super::{ProductRunServiceError, RunRecord, deliverable};

const REVIEWED_COMMAND_UNCERTAINTY: &str = "A reviewed command outcome remains unknown. Developer mutations for this requirements revision are frozen; inspect and preserve the candidate or provide new user input.";

struct ReceiptRecoveryOwner {
    inner: Arc<super::Inner>,
    run: peritus_types::RunId,
}

impl Drop for ReceiptRecoveryOwner {
    fn drop(&mut self) {
        self.inner
            .command_recoveries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.run);
    }
}

impl super::ProductRunService {
    pub(super) fn reconcile_restored_command_receipts(
        &self,
    ) -> Vec<(peritus_types::RunId, peritus_types::ActionId, peritus_types::ProcessId)> {
        let records = self.inner.records.read().ok().map(|records| {
            records
                .values()
                .filter(|record| record.snapshot.phase().terminal())
                .cloned()
                .collect::<Vec<_>>()
        });
        let Some(records) = records else { return Vec::new() };
        let mut live_owners = Vec::new();
        for record in records {
            let receipts = effects_path(&self.inner.directory, &record);
            if receipts.exists() {
                match peritus_product_runner::CommandRuntime::receipt_linked_live_owners(
                    &receipts,
                    record.request.run_id(),
                    &self.inner.processes,
                ) {
                    Ok(owners) => {
                        for owner in owners {
                            if !live_owners.contains(&owner) {
                                live_owners.push(owner);
                            }
                        }
                    }
                    Err(error) => {
                        diagnostic::report(&format!(
                            "peritusd: exact live command owner could not be verified for run {}: {error}",
                            deliverable::run_hex(record.request.run_id())
                        ));
                    }
                }
            }
            if let Err(error) = self.reconcile_command_receipts(&record) {
                diagnostic::report(&format!(
                    "peritusd: native command receipt recovery remains unresolved for run {}: {error}",
                    deliverable::run_hex(record.request.run_id())
                ));
            }
        }
        live_owners
    }

    pub(super) fn project_operation(
        &self,
        record: &RunRecord,
    ) -> Result<ProductRunOperation, ProductRunServiceError> {
        self.reconcile_command_receipts(record)?;
        project(&self.inner.directory, record)
    }

    fn reconcile_command_receipts(&self, record: &RunRecord) -> Result<(), ProductRunServiceError> {
        if !record.snapshot.phase().terminal() {
            return Ok(());
        }
        let receipts = effects_path(&self.inner.directory, record);
        if !receipts.exists() {
            return Ok(());
        }
        let pending = uncertain_effects(&receipts).map_err(|error| {
            ProductRunServiceError::internal("inspect native command receipt", error.to_string())
        })?;
        if !pending.iter().any(|effect| effect.state() != UncertainEffectState::Reviewed) {
            return Ok(());
        }
        let Ok(tasks) = self.inner.tasks.try_lock() else {
            return Ok(());
        };
        if tasks.iter().any(|(run, task)| *run == record.request.run_id() && !task.is_finished()) {
            return Ok(());
        }
        let run = record.request.run_id();
        let mut recovering = self
            .inner
            .command_recoveries
            .lock()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        if !recovering.insert(run) {
            return Ok(());
        }
        drop(recovering);
        drop(tasks);
        let _owner = ReceiptRecoveryOwner { inner: Arc::clone(&self.inner), run };
        let Some(root) = self.inner.workspaces.get(&record.request.workspace_id()) else {
            return Ok(());
        };
        let Ok(runtime) = super::execution::open_command_runtime(self, &record.request, root)
        else {
            return Ok(());
        };
        runtime.reconcile_effect_receipts(&receipts).map_err(|error| {
            ProductRunServiceError::internal("reconcile native command receipt", error.to_string())
        })
    }

    pub(super) fn ensure_control_legal(
        &self,
        run: peritus_types::RunId,
        action: Action,
    ) -> Result<(), ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
        if self.project_operation(record)?.legal_controls().allows(action) {
            Ok(())
        } else {
            Err(ProductRunServiceError::InvalidState)
        }
    }

    pub(super) fn acknowledge_command_outcome(
        &self,
        run: peritus_types::RunId,
    ) -> Result<peritus_app_protocol::ProductRunSnapshot, ProductRunServiceError> {
        let mut records =
            self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get_mut(&run).ok_or(ProductRunServiceError::NotFound)?;
        let projection = self.project_operation(record)?;
        if projection.kind() != Kind::Command
            || projection.state() != State::OutcomeUnknown
            || !projection.legal_controls().allows(Action::Acknowledge)
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        acknowledge_uncertain_effect(
            &effects_path(&self.inner.directory, record),
            projection.identity(),
        )
        .map_err(|error| {
            ProductRunServiceError::internal(
                "acknowledge uncertain command outcome",
                error.to_string(),
            )
        })?;
        let detail = REVIEWED_COMMAND_UNCERTAINTY;
        detail.clone_into(&mut record.interruption_cause);
        record.snapshot = super::snapshot::replace_snapshot(
            &record.snapshot,
            record.snapshot.phase(),
            detail,
            record.snapshot.summary(),
        )?;
        super::persistence::persist_record(&self.inner.directory, record)?;
        super::snapshot::live_snapshot(self, record)
    }
}

pub(super) fn project(
    directory: &Path,
    record: &RunRecord,
) -> Result<ProductRunOperation, ProductRunServiceError> {
    if let Some(pending) = deliverable::discard::Pending::read(directory, record)? {
        let state = pending.inspect(directory, record)?;
        let (known, uncertainty) = match &state {
            DiscardTransactionState::Prepared => (
                "The exact discard intent and candidate binding are durable; restoration has not started.".to_owned(),
                String::new(),
            ),
            DiscardTransactionState::Restoring => (
                "The original discard authority and completed restoration boundaries are durable.".to_owned(),
                "Workspace restoration may be partial until the exact Discard operation completes."
                    .to_owned(),
            ),
            DiscardTransactionState::Completed(paths) => (
                format!(
                    "Discard restoration completed with {} retained repository recovery record(s); its run acknowledgement is pending.",
                    paths.len()
                ),
                String::new(),
            ),
        };
        let state = match state {
            DiscardTransactionState::Prepared => State::RecoveryRequired,
            DiscardTransactionState::Restoring => State::OutcomeUnknown,
            DiscardTransactionState::Completed(_) => State::Succeeded,
        };
        let mut controls = ProductRunLegalControls::none().with(Action::Discard);
        if record.snapshot.deliverable().is_some_and(deliverable::export_available) {
            controls = controls.with(Action::Export);
        }
        return operation(
            Kind::Discard,
            state,
            format!("discard/{}", pending.identity()),
            known,
            uncertainty,
            controls,
        );
    }

    let uncertain = uncertain_effects(&effects_path(directory, record)).map_err(|error| {
        ProductRunServiceError::internal("inspect developer command outcome", error.to_string())
    })?;
    let unresolved = uncertain
        .iter()
        .filter(|effect| effect.state() != UncertainEffectState::Reviewed)
        .collect::<Vec<_>>();
    if let Some(effect) = unresolved.first() {
        let count = unresolved.len();
        let mut independent = command_export_control(record);
        if effect.state() == UncertainEffectState::Started
            && !effect.owner_inactive()
            && !record.snapshot.phase().terminal()
        {
            return operation(
                Kind::Command,
                State::Running,
                effect.identity().to_owned(),
                format!(
                    "The daemon still owns the active run and its durable {} command admission.",
                    effect.tool()
                ),
                "The command has not recorded a terminal result yet.".to_owned(),
                independent.with(Action::Cancel),
            );
        }
        if effect.owner_inactive() {
            independent = independent.with(Action::Acknowledge);
        }
        return operation(
            Kind::Command,
            State::OutcomeUnknown,
            effect.identity().to_owned(),
            format!(
                "The command receipt retains the original {} operation and forbids automatic replay.",
                effect.tool()
            ),
            if count == 1 {
                "The host cannot yet prove whether this command took effect.".to_owned()
            } else {
                format!(
                    "The host cannot yet prove whether this command took effect; {} additional command outcome(s) are also unresolved.",
                    count - 1
                )
            },
            independent,
        );
    }
    let reviewed_current = uncertain.iter().any(|effect| {
        effect.state() == UncertainEffectState::Reviewed
            && effect.requirements_revision() == Some(record.interaction.incorporated)
    });

    if let Some(deliverable) = record.snapshot.deliverable()
        && deliverable.commit_revision().is_empty()
        && deliverable::commit::attempt_matches(directory, record, deliverable)?
    {
        return operation(
            Kind::Commit,
            State::RecoveryRequired,
            format!("commit/{}", deliverable::run_hex(record.request.run_id())),
            "The exact source patch and commit retry binding are durable.".to_owned(),
            "One or more repository commits may already have completed; exact Commit retry reconciles current Git state before proceeding."
                .to_owned(),
            ProductRunLegalControls::none()
                .with(Action::Commit)
                .with(Action::Export),
        );
    }

    let state = execution_state(record.snapshot.phase());
    let controls = if reviewed_current {
        command_export_control(record)
    } else {
        deliverable_controls(record, execution_controls(state))
    };
    operation(
        Kind::Execution,
        state,
        format!("run/{}", deliverable::run_hex(record.request.run_id())),
        execution_known_fact(record.snapshot.phase()),
        execution_uncertainty(state, &record.interruption_cause, reviewed_current),
        controls,
    )
}

fn command_export_control(record: &RunRecord) -> ProductRunLegalControls {
    if record.snapshot.deliverable().is_some_and(deliverable::export_available) {
        ProductRunLegalControls::none().with(Action::Export)
    } else {
        ProductRunLegalControls::none()
    }
}

pub(super) fn may_start_execution(
    service: &super::ProductRunService,
    record: &RunRecord,
) -> Result<bool, ProductRunServiceError> {
    Ok(service.project_operation(record)?.may_start_execution())
}

/// Builds the execution projection retained inside a snapshot.
/// Public observations replace it through [`project`] after inspecting command and handoff owners.
pub(super) fn retained_execution(
    run: peritus_types::RunId,
    phase: ProductRunPhase,
    interruption_cause: &str,
) -> Result<ProductRunOperation, ProductRunServiceError> {
    let state = execution_state(phase);
    operation(
        Kind::Execution,
        state,
        format!("run/{}", deliverable::run_hex(run)),
        execution_known_fact(phase),
        execution_uncertainty(state, interruption_cause, false),
        execution_controls(state),
    )
}

const fn execution_state(phase: ProductRunPhase) -> State {
    match phase {
        ProductRunPhase::Queued
        | ProductRunPhase::Designing
        | ProductRunPhase::Writing
        | ProductRunPhase::Checking
        | ProductRunPhase::Reviewing
        | ProductRunPhase::Fixing
        | ProductRunPhase::Verifying => State::Running,
        ProductRunPhase::WaitingForUser => State::WaitingForUser,
        ProductRunPhase::Complete => State::Succeeded,
        ProductRunPhase::Failed => State::Failed,
        ProductRunPhase::Cancelled => State::Cancelled,
        ProductRunPhase::RecoveryRequired => State::RecoveryRequired,
    }
}

fn execution_controls(state: State) -> ProductRunLegalControls {
    let mut controls = ProductRunLegalControls::none();
    if state == State::Running || state == State::WaitingForUser {
        controls = controls.with(Action::Cancel);
    }
    if matches!(state, State::Failed | State::Cancelled | State::RecoveryRequired) {
        controls = controls.with(Action::Retry);
    }
    controls
}

fn effects_path(directory: &Path, record: &RunRecord) -> std::path::PathBuf {
    directory
        .join(format!("{}.trace", deliverable::run_hex(record.request.run_id())))
        .with_extension("effects.bin")
}

fn deliverable_controls(
    record: &RunRecord,
    mut controls: ProductRunLegalControls,
) -> ProductRunLegalControls {
    let Some(deliverable) = record.snapshot.deliverable() else { return controls };
    if !record.snapshot.phase().terminal() {
        return controls;
    }
    if !deliverable.export_path().is_empty() {
        controls = controls.with(Action::Export);
    }
    if deliverable.accepted() {
        controls = controls.with(Action::Accept);
    }
    if !deliverable.commit_revision().is_empty() {
        controls = controls.with(Action::Commit);
    }
    if deliverable.discarded() {
        return controls.with(Action::Discard);
    }
    if !record.candidate_actionable {
        return controls;
    }
    controls = controls.with(Action::Accept).with(Action::Export).with(Action::Commit);
    if deliverable.commit_revision().is_empty() {
        controls = controls.with(Action::Discard);
    }
    controls
}

fn execution_known_fact(phase: ProductRunPhase) -> String {
    let state = execution_state(phase);
    match state {
        State::Running => format!(
            "The daemon owns this active run at phase {phase:?}; the latest durable boundary is visible in its status and progress."
        ),
        State::WaitingForUser => {
            "The run stopped at a durable boundary and is waiting for user input.".to_owned()
        }
        State::Succeeded => {
            "The run reached its retained terminal settlement; candidate evidence remains separately qualified."
                .to_owned()
        }
        State::Failed => "The run stopped unsuccessfully at a retained boundary.".to_owned(),
        State::Cancelled => "The run owner retained the cancellation boundary.".to_owned(),
        State::RecoveryRequired => {
            "The original run identity and retained continuation are available for explicit exact retry."
                .to_owned()
        }
        State::OutcomeUnknown => unreachable!("specialized owners project unknown outcomes"),
    }
}

fn execution_uncertainty(state: State, interruption_cause: &str, reviewed_command: bool) -> String {
    if reviewed_command {
        return REVIEWED_COMMAND_UNCERTAINTY.to_owned();
    }
    if state != State::RecoveryRequired {
        return String::new();
    }
    if interruption_cause.is_empty() {
        "The interrupted provider phase did not reach a terminal settlement.".to_owned()
    } else {
        interruption_cause.to_owned()
    }
}

fn operation(
    kind: Kind,
    state: State,
    identity: String,
    known: String,
    uncertainty: String,
    controls: ProductRunLegalControls,
) -> Result<ProductRunOperation, ProductRunServiceError> {
    ProductRunOperation::new(kind, state, identity, known, uncertainty, controls)
        .map_err(|_| ProductRunServiceError::InvalidMessage)
}
