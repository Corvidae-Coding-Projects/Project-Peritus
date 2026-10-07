//! Read-only authenticated discovery of the durable conversation execution.

use super::{
    ProductRunService, domain_operation_with_store, error_response, receipt_projection,
    resolve_user_operation, snapshot,
};
use peritus_app_protocol::{
    AppResponsePayload, WorkbenchCommand, WorkbenchContinuationAdmission,
    WorkbenchContinuationAdmissionState, WorkbenchExecutionSettings, WorkbenchExecutionState,
    WorkbenchIntent, WorkbenchQuery,
};
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ConversationId, ConversationRecord,
};
use peritus_types::{ActorId, RunId};

impl ProductRunService {
    pub(crate) fn query_interaction_binding(
        &self,
        actor: ActorId,
        query: peritus_app_protocol::ProductInteractionQuery,
    ) -> Result<
        peritus_app_protocol::ProductInteractionBinding,
        crate::product_run::ProductRunServiceError,
    > {
        use crate::product_run::ProductRunServiceError as Error;
        let conversation = {
            let records = self.inner.records.read().map_err(|_| Error::Unavailable)?;
            let record = records.get(&query.run_id()).ok_or(Error::NotFound)?;
            peritus_app_protocol::ConversationId::new(
                *record.interaction.workbench.conversation().as_bytes(),
            )
            .map(|id| WorkbenchQuery::new(id, record.request.workspace_id()))
            .map_err(|_| Error::InvalidMessage)?
        };
        {
            let scope = conversation;
            self.control_workspace(scope)?;
            let id =
                ConversationId::new(scope.conversation().into_bytes()).map_err(Error::Control)?;
            self.with_control_conversation(id, |store| {
                let record = store.load(id)?.ok_or(ControlError::NotFound)?;
                if record.owner_bytes() != actor.as_bytes()
                    || record.workspace_bytes() != scope.workspace().as_bytes()
                {
                    return Err(ControlError::ScopeMismatch.into());
                }
                Ok(())
            })?;
        }
        peritus_app_protocol::ProductInteractionBinding::new(
            self.query_interaction(query)?,
            conversation,
        )
        .map_err(|_| Error::InvalidMessage)
    }

    pub(crate) fn workbench_execution(
        &self,
        actor: ActorId,
        query: WorkbenchQuery,
    ) -> AppResponsePayload {
        let result = self.control_workspace(query).and_then(|()| {
            let id = ConversationId::new(query.conversation().into_bytes())?;
            self.with_control_conversation(id, |store| {
                let record = store.load(id)?.ok_or(ControlError::NotFound)?;
                if record.owner_bytes() != actor.as_bytes()
                    || record.workspace_bytes() != query.workspace().as_bytes()
                {
                    return Err(ControlError::ScopeMismatch.into());
                }
                let run = record
                    .execution()
                    .map(|execution| RunId::new(*execution.run_bytes()))
                    .transpose()
                    .map_err(|_| ControlError::InvalidInput)?;
                WorkbenchExecutionState::new(
                    snapshot(query, &record)?,
                    run,
                    record.goal().is_some(),
                )
                .map_err(|_| ControlError::InvalidInput.into())
            })
        });
        result.map_or_else(error_response, AppResponsePayload::WorkbenchExecution)
    }
}

impl ProductRunService {
    pub(super) async fn continue_workbench_execution(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let WorkbenchIntent::ContinueExecution(settings) = command.intent() else {
            return crate::product_run::ProductRunServiceError::InvalidMessage.response();
        };
        let prepared = self.control_workspace(command.query()).and_then(|()| {
            let conversation = ConversationId::new(command.query().conversation().into_bytes())?;
            self.with_control_conversation(conversation, |store| {
                let operation = domain_operation_with_store(store, actor, command)?;
                let receipt = resolve_user_operation(store, &operation)?;
                if receipt.is_none() {
                    let current = store
                        .load(operation.conversation())?
                        .ok_or(ControlError::NotFound)?;
                    ConversationRecord::apply(Some(&current), &operation)?;
                }
                let run = continuation_run(&operation)?;
                Ok((operation, run, receipt))
            })
        });
        let (operation, run, replay_receipt) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return error_response(error),
        };
        if let Some(receipt) = replay_receipt.as_ref() {
            match self.continuation_launch_owned(run, &operation) {
                Ok(true) => {
                    return receipt_projection(command, receipt)
                        .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt);
                }
                Ok(false) => {}
                Err(error) => return error.response(),
            }
        } else {
            if let Err(error) = self.continuation_record_ready(run, &operation, settings) {
                return error.response();
            }
        }
        if let Err(error) = self.validate_models(settings.providers(), settings.models()).await {
            return error.response();
        }
        let receipt = match self.with_control_conversation(operation.conversation(), |store| {
            Ok(match resolve_user_operation(store, &operation)? {
                Some(receipt) => receipt,
                None => store.accept(&operation)?,
            })
        }) {
            Ok(receipt) => receipt,
            Err(error) => return error_response(error),
        };
        // Receipt acceptance and launch ownership are deliberately separate durable facts. Once
        // C0 accepted this exact command, always return that receipt. The client then observes
        // typed admission state and retains exact replay if launch preparation is still pending.
        let _launch = self
            .retry_admitted(
                run,
                Some(crate::product_run::lifecycle::RetryAdmission::Continuation {
                    operation: operation.clone(),
                    settings: settings.clone(),
                }),
            )
            .await;
        receipt_projection(command, &receipt)
            .map_or_else(error_response, AppResponsePayload::WorkbenchReceipt)
    }

    pub(crate) fn workbench_continuation_admission(
        &self,
        actor: ActorId,
        command: &WorkbenchCommand,
    ) -> AppResponsePayload {
        let resolved = self.control_workspace(command.query()).and_then(|()| {
            let conversation = ConversationId::new(command.query().conversation().into_bytes())?;
            self.with_control_conversation(conversation, |store| {
                let operation = domain_operation_with_store(store, actor, command)?;
                if !matches!(operation.intent(), ControlIntent::ContinueExecution { .. }) {
                    return Err(ControlError::InvalidInput.into());
                }
                resolve_user_operation(store, &operation)?
                    .ok_or(ControlError::NotFound)?;
                let run = continuation_run(&operation)?;
                Ok((operation, run))
            })
        });
        let (operation, run) = match resolved {
            Ok(resolved) => resolved,
            Err(error) => return error_response(error),
        };
        let state = match self.continuation_launch_owned(run, &operation) {
            Ok(true) => WorkbenchContinuationAdmissionState::LaunchOwned,
            Ok(false) => WorkbenchContinuationAdmissionState::AcceptedPendingLaunch,
            Err(error) => return error.response(),
        };
        AppResponsePayload::WorkbenchContinuationAdmission(WorkbenchContinuationAdmission::new(
            command.operation(),
            command.query(),
            run,
            state,
        ))
    }

    fn continuation_record_ready(
        &self,
        run: RunId,
        operation: &ControlOperation,
        settings: &WorkbenchExecutionSettings,
    ) -> Result<(), crate::product_run::ProductRunServiceError> {
        use crate::product_run::ProductRunServiceError as Error;
        let record = self
            .inner
            .records
            .read()
            .map_err(|_| Error::Unavailable)?
            .get(&run)
            .ok_or(Error::NotFound)?
            .clone();
        validate_continuation_binding(&record, operation, settings)?;
        self.validate_workspace_mode(record.request.workspace_id(), settings.mode())?;
        if !super::super::operation::may_start_execution(&self.inner.directory, &record)?
            || !self.pending_record_input(&record)?
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }

    fn continuation_launch_owned(
        &self,
        run: RunId,
        operation: &ControlOperation,
    ) -> Result<bool, crate::product_run::ProductRunServiceError> {
        use crate::product_run::ProductRunServiceError as Error;
        let settled = {
            let records = self.inner.records.read().map_err(|_| Error::Unavailable)?;
            let record = records.get(&run).ok_or(Error::NotFound)?;
            validate_continuation_operation(record, operation)?;
            record
                .continuation_sources
                .iter()
                .find(|source| source.operation == operation.id())
                .is_some_and(|source| source.settled)
        };
        Ok(settled || self.continuation_owner_live(run, operation.id())?)
    }

    pub(in crate::product_run) fn continuation_pending(
        &self,
        record: &crate::product_run::RunRecord,
        operation: &ControlOperation,
        settings: &WorkbenchExecutionSettings,
    ) -> Result<
        Option<crate::product_run::ContinuationSource>,
        crate::product_run::ProductRunServiceError,
    > {
        validate_continuation_binding(record, operation, settings)?;
        if let Some(source) = record
            .continuation_sources
            .iter()
            .find(|source| source.operation == operation.id())
            .copied()
        {
            if source.settled
                || self.continuation_owner_retained(record.request.run_id(), operation.id())?
            {
                return Ok(None);
            }
            self.validate_continuation_source(record, operation, source)?;
            return Ok(Some(source));
        }
        let (context_generation, start_operation) = match operation.intent() {
            ControlIntent::ContinueExecution {
                context_generation,
                start_operation,
                ..
            } => (*context_generation, *start_operation),
            _ => return Err(ControlError::InvalidInput.into()),
        };
        self.with_control_conversation(operation.conversation(), |store| {
            let receipt = store.resolve(operation)?.ok_or(ControlError::NotFound)?;
            let admitted = store.capture_execution_revision(
                &record.interaction.workbench,
                receipt.accepted_revision(),
            )?;
            let execution = store
                .load_revision(operation.conversation(), receipt.accepted_revision())?
                .ok_or(ControlError::NotFound)?
                .execution()
                .cloned()
                .ok_or(ControlError::NotFound)?;
            if execution.start_operation() != start_operation
                || admitted.inputs().generation() != context_generation
                || admitted.inputs().pending().is_empty()
            {
                return Err(ControlError::InvalidInput.into());
            }
            Ok(Some(crate::product_run::ContinuationSource::new(
                operation.id(),
                receipt.accepted_revision(),
                context_generation,
            )))
        })
        .map_err(Into::into)
    }

    fn validate_continuation_source(
        &self,
        record: &crate::product_run::RunRecord,
        operation: &ControlOperation,
        source: crate::product_run::ContinuationSource,
    ) -> Result<(), crate::product_run::ProductRunServiceError> {
        let context_generation = match operation.intent() {
            ControlIntent::ContinueExecution { context_generation, .. } => *context_generation,
            _ => return Err(ControlError::InvalidInput.into()),
        };
        if source.generation != context_generation {
            return Err(ControlError::InvalidInput.into());
        }
        self.with_control_conversation(operation.conversation(), |store| {
            let receipt = store.resolve(operation)?.ok_or(ControlError::NotFound)?;
            if receipt.accepted_revision() != source.revision {
                return Err(ControlError::InvalidInput.into());
            }
            let admitted = store.capture_execution_revision(
                &record.interaction.workbench,
                source.revision,
            )?;
            if admitted.inputs().generation() != source.generation
                || admitted.inputs().pending().is_empty()
            {
                return Err(ControlError::InvalidInput.into());
            }
            Ok(())
        })
        .map_err(Into::into)
    }
}

fn continuation_run(
    operation: &ControlOperation,
) -> Result<RunId, crate::product_control::ControlStoreError> {
    let ControlIntent::ContinueExecution { run, .. } = operation.intent() else {
        return Err(ControlError::InvalidInput.into());
    };
    RunId::new(*run).map_err(|_| ControlError::InvalidInput.into())
}

fn validate_continuation_operation(
    record: &crate::product_run::RunRecord,
    operation: &ControlOperation,
) -> Result<(), crate::product_run::ProductRunServiceError> {
    use crate::product_run::ProductRunServiceError as Error;
    let start = &record.interaction.workbench;
    let (
        ControlIntent::StartExecution { run: start_run, .. },
        ControlIntent::ContinueExecution { run, start_operation, .. },
    ) = (start.intent(), operation.intent())
    else {
        return Err(Error::Control(ControlError::InvalidInput));
    };
    if run != start_run
        || run != record.request.run_id().as_bytes()
        || *start_operation != start.id()
        || operation.conversation() != start.conversation()
        || operation.actor_bytes() != start.actor_bytes()
        || operation.workspace_bytes() != start.workspace_bytes()
    {
        return Err(Error::Control(ControlError::ScopeMismatch));
    }
    Ok(())
}

fn validate_continuation_binding(
    record: &crate::product_run::RunRecord,
    operation: &ControlOperation,
    settings: &WorkbenchExecutionSettings,
) -> Result<(), crate::product_run::ProductRunServiceError> {
    use crate::product_run::ProductRunServiceError as Error;
    validate_continuation_operation(record, operation)?;
    let ControlIntent::ContinueExecution { settings_digest, .. } = operation.intent() else {
        return Err(Error::Control(ControlError::InvalidInput));
    };
    let requested_digest = settings
        .fingerprint()
        .map_err(|_| Error::Control(ControlError::InvalidInput))?;
    if settings.run() != record.request.run_id()
        || settings.providers() != record.request.providers()
        || requested_digest.as_bytes() != settings_digest
    {
        return Err(Error::Control(ControlError::InvalidInput));
    }
    Ok(())
}
