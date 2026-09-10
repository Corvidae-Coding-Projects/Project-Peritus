//! Authenticated A3/domain mapping and the single serialized control-journal owner.

use super::ProductRunService;
use crate::product_control::{ControlStore, ControlStoreError as Error};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, AppResponsePayload, ConversationTitle, WorkbenchCommand,
    WorkbenchIntent, WorkbenchQuery, WorkbenchReceipt, WorkbenchSnapshot,
};
use peritus_product_runner::control::{
    ControlError, ControlOperation, ControlReceipt, ConversationId, ConversationRecord,
};
use peritus_types::ActorId;

mod brief;
mod checkpoints;
#[cfg(test)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "crate-level tests inject exact crash boundaries"
)]
pub(crate) use checkpoints::{RewindFaultPoint, inject_rewind_fault};
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

use mapping::{domain_operation, domain_operation_with_store, equivalent_user_intent};

impl ProductRunService {
    pub(crate) fn preview_workbench_compaction(
        &self,
        actor: ActorId,
        request: &peritus_app_protocol::WorkbenchCompactionRequest,
    ) -> AppResponsePayload {
        let result = self.control_workspace(request.query()).and_then(|()| {
            let run = self.with_controls(false, |store| store.compaction_run(actor, request))?;
            let complete = self
                .inner
                .records
                .read()
                .map_err(|_| Error::Corrupt("product run registry lock poisoned"))?
                .get(&run)
                .is_some_and(|record| {
                    record.snapshot.phase() == peritus_app_protocol::ProductRunPhase::Complete
                });
            self.with_controls(false, |store| {
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
        let result = self
            .control_workspace(query.query())
            .and_then(|()| self.with_controls(false, |store| store.context_page(actor, query)));
        result.map_or_else(error_response, AppResponsePayload::WorkbenchContext)
    }

    pub(crate) fn workbench_memory(
        &self,
        actor: ActorId,
        query: peritus_app_protocol::WorkbenchMemoryQuery,
    ) -> AppResponsePayload {
        let result = self
            .control_workspace(query.query())
            .and_then(|()| self.with_controls(false, |store| store.guidance_page(actor, query)));
        result.map_or_else(error_response, AppResponsePayload::WorkbenchMemory)
    }

    pub(crate) async fn workbench_command(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let required = super::permissions::command_permissions(command.intent());
        if !required.is_empty()
            && let Err(error) = self.require_workspace_permissions(actor, command.query(), required)
        {
            return error_response(error);
        }
        match command.intent() {
            WorkbenchIntent::StartExecution(_) | WorkbenchIntent::StartGoal { .. } => {
                return self.start_workbench(actor, command).await;
            }
            WorkbenchIntent::CreateCheckpoint(_) => {
                return self.create_workbench_checkpoint(actor, command).await;
            }
            _ => {}
        }
        if matches!(
            command.intent(),
            WorkbenchIntent::StartPreview(_)
                | WorkbenchIntent::InteractPreview { .. }
                | WorkbenchIntent::StopPreview { .. }
                | WorkbenchIntent::CheckPreviewBehavior { .. }
                | WorkbenchIntent::AddArtifactFeedback { .. }
        ) {
            return self.workbench_preview_local_command(actor, command);
        }
        if matches!(command.intent(), WorkbenchIntent::ResumeGoal { .. }) {
            return self.resume_workbench_goal(actor, command).await;
        }
        let review = matches!(
            command.intent(),
            WorkbenchIntent::AddReview { .. }
                | WorkbenchIntent::RebindReview { .. }
                | WorkbenchIntent::DismissReview { .. }
        );
        if review {
            let operation = match domain_operation(actor, command) {
                Ok(operation) => operation,
                Err(error) => return error_response(error),
            };
            match self.with_controls(false, |store| store.resolve(&operation)) {
                Ok(Some(receipt)) => {
                    return WorkbenchReceipt::new(
                        command.operation(),
                        command.query(),
                        receipt.accepted_revision(),
                        receipt.payload_digest(),
                    )
                    .map_or_else(
                        |_| error_response(ControlError::InvalidInput.into()),
                        AppResponsePayload::WorkbenchReceipt,
                    );
                }
                Ok(None) => {}
                Err(error) => return error_response(error),
            }
            if let Err(error) = review::validate_command(self, actor, command) {
                return error_response(error);
            }
        }
        if matches!(command.intent(), WorkbenchIntent::ForkConversation(_)) {
            return self.fork_workbench(actor, command);
        }
        if guidance::is_guidance(command.intent()) {
            return self.workbench_guidance_command(actor, command);
        }
        let result = self.control_workspace(command.query()).and_then(|()| {
            let create = matches!(command.intent(), WorkbenchIntent::CreateConversation(_));
            let permission_host = if matches!(command.intent(), WorkbenchIntent::SetPermissions(_))
            {
                Some(self.permission_host(command.query().workspace())?)
            } else {
                None
            };
            self.with_controls(create, |store| {
                let operation = domain_operation_with_store(store, actor, command)?;
                if let Some(receipt) = resolve_user_operation(store, &operation)? {
                    return Ok((receipt, true));
                }
                match permission_host {
                    Some(host) => store.accept_permissions(&operation, host),
                    None => store.accept(&operation),
                }
                .map(|receipt| (receipt, false))
            })
            .and_then(|(receipt, replay)| {
                receipt_projection(command, &receipt).map(|projected| (projected, replay))
            })
        });
        let fresh = result.as_ref().is_ok_and(|(_, replay)| !replay);
        let response = result
            .map(|(receipt, _)| receipt)
            .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt);
        if matches!(
            command.intent(),
            WorkbenchIntent::PauseGoal {
                mode: peritus_app_protocol::WorkbenchGoalPauseMode::Now,
                ..
            } | WorkbenchIntent::ClearGoal { .. }
        ) && fresh
            && matches!(response, AppResponsePayload::WorkbenchReceipt(_))
        {
            self.signal_goal_cancellation(command);
        }
        if review
            && fresh
            && matches!(response, AppResponsePayload::WorkbenchReceipt(_))
            && let Err(error) = review::resume_feedback(self, actor, command).await
        {
            return error.response();
        }
        response
    }

    fn workbench_guidance_command(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let result = self.control_workspace(command.query()).and_then(|()| {
            self.with_controls(false, |store| {
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
            let id = ConversationId::new(query.conversation().into_bytes())?;
            let record =
                self.with_controls(false, |store| store.load(id))?.ok_or(ControlError::NotFound)?;
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
                .with_controls(false, |store| {
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
        let mut owner = self
            .inner
            .controls
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
            *owner = Some(ControlStore::open(&root, self.inner.control_store)?);
        }
        let store = owner.as_mut().ok_or(Error::Corrupt("control owner initialization failed"))?;
        operation(store)
    }
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
        Error::Io(_) | Error::Journal(_) => AppErrorCode::Backpressure,
        Error::PermissionDenied => AppErrorCode::ReadOnly,
        Error::Workspace(error)
            if error.recovery() == peritus_workspace::RecoveryClass::Reobserve =>
        {
            AppErrorCode::StaleRevision
        }
        Error::Corrupt(_) | Error::Workspace(_) | Error::Runner(_) => AppErrorCode::NotReady,
    };
    AppProtocolError::new(code, None)
}

#[cfg(test)]
mod tests;
