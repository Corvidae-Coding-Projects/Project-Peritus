//! Authenticated A3/domain mapping and the single serialized control-journal owner.

use super::ProductRunService;
use crate::product_control::{
    AuthorityKey, AuthoritySet, ControlReconciliation, ControlStore,
    ControlStoreError as Error,
};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppResponsePayload, ConversationTitle, WorkbenchCommand,
    WorkbenchIntent, WorkbenchQuery, WorkbenchReceipt, WorkbenchSnapshot,
};
use peritus_product_runner::control::{
    ControlError, ControlOperation, ControlReceipt, ConversationId, ConversationRecord,
};
use peritus_types::ActorId;
#[cfg(test)]
use std::{collections::VecDeque, sync::Arc, time::Duration};

mod brief;
mod checkpoints;
mod command;
mod conversation;
#[cfg(test)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "crate-level tests inject exact crash boundaries"
)]
pub(crate) use checkpoints::RewindFaultPoint;
mod execution;
mod files;
mod folder_mutation;
mod fork;
mod goal;
mod guidance;
mod images;
mod inputs;
mod launch;
mod mapping;
mod review;
mod run_control;
mod sources;

use mapping::{domain_operation, domain_operation_with_store, equivalent_user_intent};

#[cfg(test)]
pub(super) struct ControlOwnerQueue {
    pub(super) owner: std::sync::Mutex<Option<ControlStore>>,
    order: std::sync::Mutex<ControlQueueState>,
    ready: std::sync::Condvar,
}

#[cfg(test)]
#[derive(Default)]
struct ControlQueueState {
    active: bool,
    waiters: VecDeque<Arc<()>>,
}

#[cfg(test)]
pub(super) struct ControlPermit<'a>(&'a ControlOwnerQueue);

#[cfg(test)]
impl ControlOwnerQueue {
    pub(super) fn new(owner: Option<ControlStore>) -> Self {
        Self {
            owner: std::sync::Mutex::new(owner),
            order: std::sync::Mutex::new(ControlQueueState::default()),
            ready: std::sync::Condvar::new(),
        }
    }

    pub(super) fn acquire(
        &self,
        cancellation: &peritus_journal::JournalCancellation,
    ) -> Result<ControlPermit<'_>, Error> {
        let waiter = Arc::new(());
        let mut order =
            self.order.lock().map_err(|_| Error::Corrupt("control owner queue lock poisoned"))?;
        order.waiters.push_back(Arc::clone(&waiter));
        loop {
            if cancellation.is_cancelled() {
                if let Some(index) =
                    order.waiters.iter().position(|queued| Arc::ptr_eq(queued, &waiter))
                {
                    order.waiters.remove(index);
                }
                self.ready.notify_all();
                return Err(Error::ContentionCancelled);
            }
            if !order.active
                && order.waiters.front().is_some_and(|queued| Arc::ptr_eq(queued, &waiter))
            {
                order.waiters.pop_front();
                order.active = true;
                return Ok(ControlPermit(self));
            }
            let wake = self
                .ready
                .wait_timeout(order, Duration::from_millis(1))
                .map_err(|_| Error::Corrupt("control owner queue wait poisoned"))?;
            order = wake.0;
        }
    }
}

#[cfg(test)]
impl Drop for ControlPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut order) = self.0.order.lock() {
            order.active = false;
            self.0.ready.notify_all();
        }
    }
}

impl ProductRunService {
    pub(super) fn with_control_authorities<T>(
        &self,
        authorities: AuthoritySet,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.with_control_authorities_cancellable(
            authorities,
            &self.inner.control_shutdown,
            operation,
        )
    }

    pub(super) fn with_run_authorities<T>(
        &self,
        authorities: AuthoritySet,
        operation: impl FnOnce() -> Result<T, super::ProductRunServiceError>,
    ) -> Result<T, super::ProductRunServiceError> {
        let authority = self
            .inner
            .control_generation
            .acquire(authorities, &self.inner.control_shutdown)?;
        let _store = self.inner.control_generation.open_scope(
            authority.into_scope(),
            &self.inner.control_shutdown,
        )?;
        if self.inner.control_shutdown.is_cancelled() {
            return Err(super::ProductRunServiceError::Unavailable);
        }
        operation()
    }

    pub(super) fn with_control_authorities_cancellable<T>(
        &self,
        authorities: AuthoritySet,
        cancellation: &peritus_journal::JournalCancellation,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let authority = self.inner.control_generation.acquire(authorities, cancellation)?;
        let mut store = self
            .inner
            .control_generation
            .open_scope(authority.into_scope(), cancellation)?;
        cancellation.run(|| operation(&mut store))
    }

    pub(super) fn with_control_conversation<T>(
        &self,
        conversation: peritus_product_runner::control::ConversationId,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.with_control_authorities(
            AuthoritySet::new([AuthorityKey::Conversation(conversation)]),
            operation,
        )
    }

    pub(super) fn with_control_conversation_cancellable<T>(
        &self,
        conversation: peritus_product_runner::control::ConversationId,
        cancellation: &peritus_journal::JournalCancellation,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.with_control_authorities_cancellable(
            AuthoritySet::new([AuthorityKey::Conversation(conversation)]),
            cancellation,
            operation,
        )
    }

    pub(super) fn with_control_index_read<T>(
        &self,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut store = self
            .inner
            .control_generation
            .open_index_read(&self.inner.control_shutdown)?;
        self.inner.control_shutdown.run(|| operation(&mut store))
    }

    pub(super) fn register_control_reconciliation(
        &self,
        authorities: AuthoritySet,
    ) -> Result<ControlReconciliation, Error> {
        self.inner.control_generation.register_reconciliation(authorities)
    }

    pub(super) fn with_control_reconciliation<T>(
        &self,
        reconciliation: &ControlReconciliation,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let authority = reconciliation.acquire(&self.inner.control_reconciliation)?;
        let mut store = self.inner.control_generation.open_scope(
            authority.into_scope(),
            &self.inner.control_reconciliation,
        )?;
        self.inner.control_reconciliation.run(|| operation(&mut store))
    }

    pub(super) fn retained_conversation(
        &self,
        run: peritus_types::RunId,
    ) -> Result<peritus_product_runner::control::ConversationId, super::ProductRunServiceError>
    {
        let records =
            self.inner.records.read().map_err(|_| super::ProductRunServiceError::Unavailable)?;
        let record = records.get(&run).ok_or(super::ProductRunServiceError::NotFound)?;
        Ok(record.interaction.workbench.conversation())
    }

    pub(super) fn retained_run_authorities(
        &self,
        run: peritus_types::RunId,
    ) -> Result<AuthoritySet, super::ProductRunServiceError> {
        let conversation = self.retained_conversation(run)?;
        Ok(AuthoritySet::new([
            AuthorityKey::Conversation(conversation),
            AuthorityKey::Run(run),
        ]))
    }

    pub(super) fn retained_effect_authorities(
        &self,
        run: peritus_types::RunId,
    ) -> Result<AuthoritySet, super::ProductRunServiceError> {
        let records =
            self.inner.records.read().map_err(|_| super::ProductRunServiceError::Unavailable)?;
        let record = records.get(&run).ok_or(super::ProductRunServiceError::NotFound)?;
        Ok(AuthoritySet::new([
            AuthorityKey::Run(run),
            AuthorityKey::Workspace(record.request.workspace_id()),
        ]))
    }

    pub(crate) fn preview_workbench_compaction(
        &self,
        actor: ActorId,
        request: &peritus_app_protocol::WorkbenchCompactionRequest,
    ) -> AppResponsePayload {
        let result = self.control_workspace(request.query()).and_then(|()| {
            let conversation = control_conversation(request.query())?;
            let run = self.with_control_conversation(conversation, |store| {
                store.compaction_run(actor, request)
            })?;
            let records = self
                .inner
                .records
                .read()
                .map_err(|_| Error::Corrupt("product run registry lock poisoned"))?;
            let complete = run.and_then(|run| records.get(&run)).is_some_and(|record| {
                record.snapshot.phase() == peritus_app_protocol::ProductRunPhase::Complete
            });
            drop(records);
            self.with_control_conversation(conversation, |store| {
                store.compaction_preview(actor, request, complete).map(|(_, preview)| preview)
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchCompactionPreview)
    }

    pub(crate) fn workbench_context(
        &self,
        actor: ActorId,
        query: peritus_app_protocol::WorkbenchContextQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query.query()).and_then(|()| {
            self.with_control_conversation(control_conversation(query.query())?, |store| {
                store.context_page(actor, query)
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchContext)
    }

    pub(crate) fn workbench_memory(
        &self,
        actor: ActorId,
        query: peritus_app_protocol::WorkbenchMemoryQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query.query()).and_then(|()| {
            self.with_control_authorities(workbench_authorities(query.query())?, |store| {
                store.guidance_page(actor, query)
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchMemory)
    }

    fn workbench_guidance_command(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let result = self.control_workspace(command.query()).and_then(|()| {
            self.with_control_authorities(workbench_authorities(command.query())?, |store| {
                let operation = domain_operation_with_store(store, actor, command)?;
                let mutation = guidance::mutation(command.intent())?;
                store.accept_guidance(&operation, mutation)
            })
            .and_then(|receipt| {
                WorkbenchReceipt::new(
                    command.operation(),
                    command.query(),
                    receipt.accepted_revision(),
                    receipt.payload_digest(),
                )
                .map_err(|_| ControlError::InvalidInput.into())
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchReceipt)
    }

    pub(crate) fn workbench_query(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query).and_then(|()| {
            let id = control_conversation(query)?;
            let record = self
                .with_control_conversation(id, |store| store.load(id))?
                .ok_or(ControlError::NotFound)?;
            if record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != query.workspace().as_bytes()
            {
                return Err(ControlError::ScopeMismatch.into());
            }
            snapshot(query, &record)
        });
        result.map_or_else(error_response, AppResponsePayload::Workbench)
    }

    pub(crate) fn workbench_receipt(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        match command.intent() {
            WorkbenchIntent::StartPreview(_)
            | WorkbenchIntent::InteractPreview { .. }
            | WorkbenchIntent::CapturePreview(_)
            | WorkbenchIntent::StopPreview { .. }
            | WorkbenchIntent::CheckPreviewBehavior { .. }
            | WorkbenchIntent::AddArtifactFeedback { .. } => {
                return self.resolve_preview_receipt(actor, command);
            }
            WorkbenchIntent::ApplyInitDiff(_) => {
                return self
                    .resolve_workbench_initialization(actor, command)
                    .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt);
            }
            WorkbenchIntent::CreateCheckpoint(_) => {
                return self
                    .resolve_workbench_checkpoint(actor, command)
                    .map_or_else(error_response, AppResponsePayload::WorkbenchCheckpoint);
            }
            WorkbenchIntent::ApplyRewind(_) => {
                return self
                    .observe_workbench_restore(actor, command)
                    .map_or_else(error_response, AppResponsePayload::WorkbenchRestore);
            }
            _ => {}
        }
        let result = self.control_workspace(command.query()).and_then(|()| {
            let receipt = self
                .with_control_authorities(workbench_authorities(command.query())?, |store| {
                    let operation = domain_operation_with_store(store, actor, command)?;
                    resolve_user_operation(store, &operation)
                })?
                .ok_or(ControlError::NotFound)?;
            receipt_projection(command, &receipt)
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchReceipt)
    }

    pub(super) fn control_workspace(&self, query: WorkbenchQuery) -> Result<(), Error> {
        if !self.inner.workspaces.contains_key(&query.workspace()) {
            return Err(ControlError::ScopeMismatch.into());
        }
        Ok(())
    }

    pub(super) fn with_controls<T>(
        &self,
        create: bool,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        // Foreground admission waits have no request deadline, but daemon shutdown owns their
        // cancellation so task joining cannot be held forever by transient database ownership.
        self.with_controls_cancellable(create, &self.inner.control_shutdown, operation)
    }

    pub(super) fn with_controls_cancellable<T>(
        &self,
        create: bool,
        cancellation: &peritus_journal::JournalCancellation,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        #[cfg(not(test))]
        {
            let _ = (create, cancellation, operation);
            return Err(Error::Corrupt(
                "unscoped control access must declare its complete authority set",
            ));
        }
        #[cfg(test)]
        {
        let _permit = self.inner.controls.acquire(cancellation)?;
        let mut owner = self
            .inner
            .controls
            .owner
            .lock()
            .map_err(|_| Error::Corrupt("control owner lock poisoned"))?;
        if owner.is_none() {
            if !create {
                return Err(ControlError::NotFound.into());
            }
            let root = self
                .inner
                .directory
                .parent()
                .ok_or(Error::Corrupt("control parent directory missing"))?
                .join("workbench-v1");
            *owner = Some(ControlStore::open_cancellable(
                &root,
                self.inner.control_store,
                cancellation,
            )?);
        }
        let store = owner.as_mut().ok_or(Error::Corrupt("control owner initialization failed"))?;
        let result = cancellation.run(|| operation(store));
        match result {
            Err(Error::Journal(error)) if error.is_contention() && cancellation.is_cancelled() => {
                Err(Error::ContentionCancelled)
            }
            result => result,
        }
        }
    }

    pub(super) fn with_controls_reconciling<T>(
        &self,
        operation: impl FnOnce(&mut ControlStore) -> Result<T, Error>,
    ) -> Result<T, Error> {
        // Run cancellation and shutdown admission cancellation cannot revoke already-owned
        // checkpoint, settlement, or reply publication. Those exact identities drain here and
        // remain recoverable from the journal if process shutdown ultimately interrupts them.
        self.with_controls_cancellable(false, &self.inner.control_reconciliation, operation)
    }
}

fn control_conversation(query: WorkbenchQuery) -> Result<ConversationId, Error> {
    ConversationId::new(query.conversation().into_bytes()).map_err(Into::into)
}

fn workbench_authorities(query: WorkbenchQuery) -> Result<AuthoritySet, Error> {
    Ok(AuthoritySet::new([
        AuthorityKey::Conversation(control_conversation(query)?),
        AuthorityKey::Workspace(query.workspace()),
    ]))
}

pub(super) fn resolve_user_operation(
    store: &ControlStore,
    proposed: &ControlOperation,
) -> Result<Option<ControlReceipt>, Error> {
    let Some(existing) = store.operation(proposed.conversation(), proposed.id())? else {
        return Ok(None);
    };
    if existing.actor_bytes() != proposed.actor_bytes()
        || existing.workspace_bytes() != proposed.workspace_bytes()
        || !equivalent_user_intent(existing.intent(), proposed.intent())
    {
        return Err(ControlError::IdempotencyConflict.into());
    }
    store
        .resolve(&existing)?
        .map(Some)
        .ok_or(Error::Corrupt("accepted control operation has no receipt"))
}

pub(super) fn receipt_projection(
    command: &WorkbenchCommand,
    receipt: &ControlReceipt,
) -> Result<WorkbenchReceipt, Error> {
    WorkbenchReceipt::new(
        command.operation(),
        command.query(),
        receipt.accepted_revision(),
        receipt.payload_digest(),
    )
    .map_err(|_| ControlError::InvalidInput.into())
}

fn snapshot(
    query: WorkbenchQuery,
    record: &ConversationRecord,
) -> Result<WorkbenchSnapshot, Error> {
    let title = ConversationTitle::new(record.title().to_owned())
        .map_err(|_| ControlError::InvalidInput)?;
    WorkbenchSnapshot::new(query, record.revision(), title, record.pinned(), record.archived())
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn error_response(error: Error) -> AppResponsePayload {
    AppResponsePayload::Error(error_value(error))
}

pub(super) fn error_value(error: Error) -> AppProtocolError {
    let code = match error {
        Error::Control(ControlError::InvalidInput) => AppErrorCode::MalformedFrame,
        Error::Control(ControlError::StaleRevision) => AppErrorCode::StaleRevision,
        Error::Control(ControlError::IdempotencyConflict) => AppErrorCode::IdempotencyConflict,
        Error::Control(ControlError::ScopeMismatch) => AppErrorCode::SessionMismatch,
        Error::Control(ControlError::Capacity) => AppErrorCode::LimitExceeded,
        Error::Control(ControlError::UnsupportedSchema) => AppErrorCode::UnsupportedSchema,
        Error::Control(ControlError::NotFound) => AppErrorCode::InvalidIdentifier,
        Error::ContentionCancelled | Error::Io(_) | Error::Journal(_) => AppErrorCode::Backpressure,
        Error::PermissionDenied => AppErrorCode::ReadOnly,
        Error::Workspace(error) if std::error::Error::source(&error).is_some() => {
            AppErrorCode::Backpressure
        }
        Error::Corrupt(_) | Error::Workspace(_) | Error::Runner(_) | Error::StalePreimage => {
            AppErrorCode::NotReady
        }
    };
    AppProtocolError::new(code, None)
}

#[cfg(test)]
mod tests;
