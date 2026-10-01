//! Read-only authenticated discovery of the durable conversation execution.

use super::{ProductRunService, error_response, snapshot};
use peritus_app_protocol::{AppResponsePayload, WorkbenchExecutionState, WorkbenchQuery};
use peritus_product_runner::control::{ControlError, ConversationId};
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
            self.with_controls(false, |store| {
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
            self.with_controls(false, |store| {
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
    pub(crate) async fn continue_workbench_execution(
        &self,
        actor: ActorId,
        continuation: peritus_app_protocol::WorkbenchContinuation,
    ) -> AppResponsePayload {
        let query = continuation.query();
        let state = match self.workbench_execution(actor, query) {
            AppResponsePayload::WorkbenchExecution(state) => state,
            error => return error,
        };
        if state.has_goal() || state.snapshot().archived() {
            return error_response(ControlError::InvalidInput.into());
        }
        let Some(run) = state.run() else {
            return error_response(ControlError::NotFound.into());
        };
        let should_resume = (|| {
            let records = self
                .inner
                .records
                .read()
                .map_err(|_| crate::product_run::ProductRunServiceError::Unavailable)?;
            let record =
                records.get(&run).ok_or(crate::product_run::ProductRunServiceError::NotFound)?;
            Ok::<_, crate::product_run::ProductRunServiceError>(
                super::super::operation::may_start_execution(&self.inner.directory, record)?
                    && self.pending_record_input(record)?,
            )
        })();
        match should_resume {
            Ok(true) => {
                if let Err(error) = self.prepare_conversation_mode(run, continuation.mode()).await {
                    return error.response();
                }
                if let Err(error) = self.retry(run).await {
                    return error.response();
                }
            }
            Ok(false) => {}
            Err(error) => return error.response(),
        }
        self.query_interaction(peritus_app_protocol::ProductInteractionQuery::new(run)).map_or_else(
            crate::product_run::ProductRunServiceError::response,
            AppResponsePayload::Interaction,
        )
    }
}

impl ProductRunService {
    async fn prepare_conversation_mode(
        &self,
        run: RunId,
        mode: peritus_app_protocol::ProductInteractionMode,
    ) -> Result<(), crate::product_run::ProductRunServiceError> {
        use crate::product_run::{ProductRunServiceError as Error, persist_record};
        let (providers, mut selection) = {
            let records = self.inner.records.read().map_err(|_| Error::Unavailable)?;
            let record = records.get(&run).ok_or(Error::NotFound)?;
            let options = &record.interaction;
            if options.mode == mode {
                return Ok(());
            }
            if !super::super::operation::may_start_execution(&self.inner.directory, record)? {
                return Err(Error::InvalidState);
            }
            (record.request.providers(), options.clone())
        };
        selection.mode = mode;
        self.validate_models(providers, &selection.models).await?;
        self.resolve_selected_providers(providers, &selection)?;
        let mut records = self.inner.records.write().map_err(|_| Error::Unavailable)?;
        let record = records.get_mut(&run).ok_or(Error::NotFound)?;
        if !super::super::operation::may_start_execution(&self.inner.directory, record)? {
            return Err(Error::InvalidState);
        }
        let previous = record.clone();
        let options = &mut record.interaction;
        if options.models != selection.models {
            return Err(Error::InvalidState);
        }
        options.mode = mode;
        options.append(
            peritus_app_protocol::ProductActivityKind::Status,
            &format!("{} selected for the next execution.", mode.label()),
            "",
        )?;
        record.resume = None;
        record.finding_state.clear();
        if let Err(error) = persist_record(&self.inner.directory, record) {
            *record = previous;
            return Err(error);
        }
        Ok(())
    }
}
