//! Persistent preview reconstruction and restart-safe process-state projection.

use super::{
    MAX_PREVIEW_OPERATIONS, MAX_PREVIEW_OUTPUT_BYTES, PersistedPreviewOperation,
    PersistedPreviewOutput, PreviewAggregate, PreviewOperationRecord, ProductRunServiceError,
};
use peritus_app_protocol::{
    ControlOperationId, WorkbenchLaunchResult, WorkbenchLaunchState, WorkbenchResultPage,
    decode_workbench_result_value,
};
use peritus_types::{RunId, WorkspaceId};
use std::collections::BTreeMap;

pub(super) fn restore_preview(
    page: Option<Vec<u8>>,
    operations: Vec<PersistedPreviewOperation>,
    outputs: Vec<PersistedPreviewOutput>,
    run: RunId,
    workspace: WorkspaceId,
    interaction: Option<&super::super::interaction::InteractionOptions>,
) -> Result<PreviewAggregate, ProductRunServiceError> {
    if operations.len() > MAX_PREVIEW_OPERATIONS
        || outputs.iter().map(|value| value.stdout.len()).sum::<usize>() > MAX_PREVIEW_OUTPUT_BYTES
    {
        return Err(ProductRunServiceError::InvalidMessage);
    }
    let page = page
        .map(|bytes| decode_workbench_result_value(&bytes))
        .transpose()
        .map_err(|_| ProductRunServiceError::InvalidMessage)?
        .map(recover_preview_page)
        .transpose()?;
    if let Some(page) = &page {
        let query = page.query();
        let binding = interaction.and_then(|options| options.workbench.as_ref());
        if query.run() != run
            || query.query().workspace() != workspace
            || binding.is_none_or(|operation| {
                operation.conversation().as_bytes() != query.query().conversation().as_bytes()
                    || operation.workspace_bytes() != query.query().workspace().as_bytes()
            })
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
    }
    let mut restored_operations = BTreeMap::new();
    for value in operations {
        let operation = ControlOperationId::new(value.operation)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if value.accepted_revision == 0
            || value.result_sequence > page.as_ref().map_or(0, WorkbenchResultPage::result_revision)
            || value.completed_sequence
                > page.as_ref().map_or(0, WorkbenchResultPage::result_revision)
            || (value.completed_sequence > 0 && value.completed_sequence <= value.result_sequence)
            || restored_operations
                .insert(
                    operation,
                    PreviewOperationRecord {
                        fingerprint: peritus_types::Sha256Digest::new(value.fingerprint),
                        accepted_revision: value.accepted_revision,
                        result_sequence: value.result_sequence,
                        completed_sequence: value.completed_sequence,
                    },
                )
                .is_some()
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
    }
    let launch_ids: std::collections::BTreeSet<ControlOperationId> = page
        .as_ref()
        .map(|value| value.launches().iter().map(WorkbenchLaunchResult::launch).collect())
        .unwrap_or_default();
    let mut restored_outputs = BTreeMap::new();
    for value in outputs {
        let launch = ControlOperationId::new(value.launch)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if !launch_ids.contains(&launch) || restored_outputs.insert(launch, value.stdout).is_some()
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
    }
    if page.is_none() && (!restored_operations.is_empty() || !restored_outputs.is_empty()) {
        return Err(ProductRunServiceError::InvalidMessage);
    }
    Ok(PreviewAggregate { page, operations: restored_operations, outputs: restored_outputs })
}

fn recover_preview_page(
    page: WorkbenchResultPage,
) -> Result<WorkbenchResultPage, ProductRunServiceError> {
    let launches = page
        .launches()
        .iter()
        .map(|launch| {
            let state = match launch.state() {
                WorkbenchLaunchState::Accepted | WorkbenchLaunchState::Running => {
                    WorkbenchLaunchState::Failed
                }
                state => state,
            };
            WorkbenchLaunchResult::new(
                launch.launch(),
                launch.profile().clone(),
                launch.process(),
                state,
                launch.ready(),
                launch.interactions().to_vec(),
                launch.captures().to_vec(),
                launch.feedback().to_vec(),
                launch.behavior_checks(),
                launch.stdout_digest(),
                launch.exit_code(),
            )
            .map_err(|_| ProductRunServiceError::InvalidMessage)
        })
        .collect::<Result<Vec<_>, _>>()?;
    WorkbenchResultPage::new(
        page.query(),
        page.control_revision(),
        page.result_revision(),
        page.capability().clone(),
        launches,
    )
    .map_err(|_| ProductRunServiceError::InvalidMessage)
}
