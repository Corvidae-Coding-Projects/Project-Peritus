//! Test adapters that enter product execution through the production workbench controls.

use super::*;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ConversationId, ConversationTitle, ProductRunControl,
    WorkbenchCommand, WorkbenchExecutionSettings, WorkbenchInputId, WorkbenchInputOrder,
    WorkbenchInputText, WorkbenchIntent, WorkbenchNewInput, WorkbenchQuery, WorkbenchQueueIntent,
};
use peritus_types::ActorId;

impl ProductRunService {
    pub(super) fn test_actor(&self, run: RunId) -> Result<ActorId, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let bytes = *records
            .get(&run)
            .ok_or(ProductRunServiceError::NotFound)?
            .interaction
            .workbench
            .actor_bytes();
        ActorId::new(bytes).map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) async fn control(
        &self,
        control: ProductRunControl,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let actor = self.test_actor(control.run_id())?;
        let request = peritus_app_protocol::RequestId::new(control.run_id().into_bytes())
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        match self.control_authenticated(actor, request, control).await {
            AppResponsePayload::ProductRunAccepted(snapshot) => Ok(snapshot),
            AppResponsePayload::ProductRunSettled(settled) => Ok(settled.snapshot().clone()),
            AppResponsePayload::Interaction(interaction) => Ok(interaction.snapshot().clone()),
            AppResponsePayload::Error(_) => Err(ProductRunServiceError::InvalidState),
            _ => Err(ProductRunServiceError::InvalidMessage),
        }
    }

    pub(super) fn load_test_records(
        &self,
    ) -> Result<BTreeMap<RunId, RunRecord>, ProductRunServiceError> {
        let root = self
            .inner
            .directory
            .parent()
            .ok_or(ProductRunServiceError::Unavailable)?
            .join("workbench-v1");
        let cancellation = peritus_journal::JournalCancellation::new();
        let _permit =
            self.inner.controls.acquire(&cancellation).map_err(ProductRunServiceError::from)?;
        let mut owner =
            self.inner.controls.owner.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        if owner.is_none() {
            *owner = Some(
                crate::product_control::ControlStore::open(&root, self.inner.control_store)
                    .map_err(ProductRunServiceError::from)?,
            );
        }
        let controls = owner.as_ref().ok_or(ProductRunServiceError::Unavailable)?;
        persistence::load_workbench_records(&root, Some(controls)).map_err(|error| {
            ProductRunServiceError::internal("load test workbench records", error.to_string())
        })
    }

    pub(super) async fn start(
        &self,
        request: ProductRunRequest,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        self.start_test_workbench(
            request,
            peritus_app_protocol::ProductInteractionMode::Build,
            peritus_app_protocol::ProductRoleModels::default(),
        )
        .await
    }

    pub(super) async fn start_interaction(
        &self,
        request: ProductRunRequest,
        mode: peritus_app_protocol::ProductInteractionMode,
        models: peritus_app_protocol::ProductRoleModels,
    ) -> Result<peritus_app_protocol::ProductInteractionSnapshot, ProductRunServiceError> {
        let run = request.run_id();
        self.start_test_workbench(request, mode, models).await?;
        self.query_interaction(peritus_app_protocol::ProductInteractionQuery::new(run))
    }

    async fn start_test_workbench(
        &self,
        request: ProductRunRequest,
        mode: peritus_app_protocol::ProductInteractionMode,
        models: peritus_app_protocol::ProductRoleModels,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let run = request.run_id();
        let workspace = request.workspace_id();
        self.validate_workspace_mode(workspace, mode)?;
        let providers = request.providers();
        let task = request.execution_task().to_owned();
        let bytes = run.into_bytes();
        let actor = ActorId::new(identity(bytes, 0x11))
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let query = WorkbenchQuery::new(
            ConversationId::new(identity(bytes, 0x22))
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            workspace,
        );
        let command = |salt, revision, intent| {
            Ok::<_, ProductRunServiceError>(WorkbenchCommand::new(
                ControlOperationId::new(identity(bytes, salt))
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                query,
                revision,
                intent,
            ))
        };
        let created = self
            .workbench_command(
                actor,
                &command(
                    0x33,
                    0,
                    WorkbenchIntent::CreateConversation(
                        ConversationTitle::new("test execution".to_owned())
                            .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                    ),
                )?,
            )
            .await;
        if !matches!(created, AppResponsePayload::WorkbenchReceipt(_)) {
            return Err(ProductRunServiceError::InvalidState);
        }
        let queued = self
            .workbench_command(
                actor,
                &command(
                    0x44,
                    1,
                    WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                        WorkbenchNewInput::new(
                            WorkbenchInputId::new(identity(bytes, 0x55))
                                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                            WorkbenchInputText::new(task)
                                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                            WorkbenchInputOrder::new(Vec::new())
                                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                        )
                        .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                    )),
                )?,
            )
            .await;
        if !matches!(queued, AppResponsePayload::WorkbenchReceipt(_)) {
            return Err(ProductRunServiceError::InvalidState);
        }
        let started = self
            .workbench_command(
                actor,
                &command(
                    0x66,
                    2,
                    WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
                        run, providers, mode, models,
                    )),
                )?,
            )
            .await;
        if !matches!(started, AppResponsePayload::WorkbenchReceipt(_)) {
            return Err(ProductRunServiceError::InvalidState);
        }
        self.query(ProductRunQuery::exact(run))?
            .into_iter()
            .next()
            .ok_or(ProductRunServiceError::NotFound)
    }
}

fn identity(mut bytes: [u8; 16], salt: u8) -> [u8; 16] {
    bytes[15] ^= salt;
    if bytes == [0; 16] {
        bytes[15] = salt.max(1);
    }
    bytes
}
