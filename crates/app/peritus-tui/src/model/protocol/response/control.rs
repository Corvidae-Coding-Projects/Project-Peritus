//! Correlated workbench and diagnostic response projection.

use crate::model::{AppModel, AppResponsePayload, Effect, PendingRequest};

pub(super) fn project(
    model: &mut AppModel,
    payload: &AppResponsePayload,
    pending: Option<PendingRequest>,
) -> Vec<Effect> {
    if is_setup_payload(payload) {
        project_setup(model, payload, pending)
    } else {
        project_live(model, payload, pending)
    }
}

fn project_setup(
    model: &mut AppModel,
    payload: &AppResponsePayload,
    pending: Option<PendingRequest>,
) -> Vec<Effect> {
    match (payload, pending) {
        (
            AppResponsePayload::WorkbenchRewindPreview(preview),
            Some(PendingRequest::WorkbenchRewind(request)),
        ) => model.accept_workbench_rewind(&request, preview.clone()),
        (
            AppResponsePayload::WorkbenchCheckpoint(receipt),
            Some(PendingRequest::WorkbenchCheckpointInspect(request)),
        ) => model.accept_workbench_checkpoint_inspection(&request, receipt),
        (
            AppResponsePayload::WorkbenchCheckpoint(receipt),
            Some(
                PendingRequest::WorkbenchControl(command)
                | PendingRequest::WorkbenchReceipt(command),
            ),
        ) => return model.accept_workbench_checkpoint(&command, receipt),
        (
            AppResponsePayload::WorkbenchRestore(receipt),
            Some(
                PendingRequest::WorkbenchControl(command)
                | PendingRequest::WorkbenchReceipt(command),
            ),
        ) => return model.accept_workbench_restore(&command, receipt),
        (
            AppResponsePayload::WorkbenchPermissions(permissions),
            Some(PendingRequest::WorkbenchPermissions(query)),
        ) => model.accept_workbench_permissions(query, permissions.clone()),
        (
            AppResponsePayload::InitProposal(proposal),
            Some(PendingRequest::WorkbenchInit(request)),
        ) => model.accept_init_proposal(request, proposal.clone()),
        (
            AppResponsePayload::WorkbenchMemory(memory),
            Some(PendingRequest::WorkbenchMemory(query)),
        ) => model.accept_workbench_memory(query, memory.clone()),
        (
            AppResponsePayload::WorkbenchCompactionPreview(preview),
            Some(PendingRequest::WorkbenchCompaction(request)),
        ) => model.accept_workbench_compaction(&request, preview.clone()),
        (
            AppResponsePayload::ConversationLibrary(page),
            Some(PendingRequest::ResumeConversationLibrary(query)),
        ) if page.query() == &query => return model.accept_latest_page(page),
        (
            AppResponsePayload::ConversationLibrary(page),
            Some(PendingRequest::ConversationLibrary(query)),
        ) if page.query() == &query => {
            model.accept_library_page(page);
        }
        (
            AppResponsePayload::WorkbenchFileImportPreview(preview),
            Some(PendingRequest::WorkbenchFileImportPreview(request)),
        ) => model.accept_file_import_preview(&request, preview.clone()),
        (
            AppResponsePayload::WorkbenchReview(page),
            Some(PendingRequest::WorkbenchReview(query)),
        ) => model.accept_review_page(query, page.clone()),
        _ => {}
    }
    Vec::new()
}

fn project_live(
    model: &mut AppModel,
    payload: &AppResponsePayload,
    pending: Option<PendingRequest>,
) -> Vec<Effect> {
    match (payload, pending) {
        (
            AppResponsePayload::WorkbenchFilePreview(preview),
            Some(PendingRequest::WorkbenchFilePreview(request)),
        ) => model.accept_file_preview(&request, preview.clone()),
        (AppResponsePayload::WorkbenchFiles(page), Some(PendingRequest::WorkbenchFiles(query))) => {
            model.accept_file_page(query, page.clone());
        }
        (
            AppResponsePayload::WorkbenchImages(page),
            Some(PendingRequest::WorkbenchImages(query)),
        ) => model.accept_workbench_images(query, page.clone()),
        (
            AppResponsePayload::WorkbenchImagePreview(preview),
            Some(PendingRequest::WorkbenchImagePreview(request)),
        ) => model.accept_image_preview(&request, preview.clone()),
        (
            AppResponsePayload::WorkbenchBrief(brief),
            Some(PendingRequest::WorkbenchBrief(query)),
        ) => return model.accept_workbench_brief(query, brief.clone()),
        (AppResponsePayload::WorkbenchGoal(goal), Some(PendingRequest::WorkbenchGoal(query))) => {
            return model.accept_workbench_goal(query, goal.clone());
        }
        (
            AppResponsePayload::WorkbenchContext(page),
            Some(PendingRequest::WorkbenchContext(query)),
        ) => model.accept_workbench_context(query, page.clone()),
        (AppResponsePayload::WorkbenchQueue(page), Some(PendingRequest::WorkbenchQueue(query))) => {
            model.accept_workbench_queue(query, page.clone());
        }
        (
            AppResponsePayload::Workbench(snapshot),
            Some(PendingRequest::WorkbenchQueueCommand { query, intent, draft }),
        ) => return model.accept_queue_command_snapshot(query, intent, &draft, snapshot),
        (
            AppResponsePayload::WorkbenchPreview(snapshot),
            Some(PendingRequest::WorkbenchResult(query)),
        ) => model.accept_preview_output(query, snapshot),
        (
            AppResponsePayload::WorkbenchResult(page),
            Some(PendingRequest::WorkbenchResult(query)),
        ) => model.accept_preview_page(query, page.clone()),
        (
            AppResponsePayload::WorkbenchExecution(state),
            Some(PendingRequest::WorkbenchExecution(query)),
        ) => {
            return model.accept_workbench_execution(query, state);
        }
        (AppResponsePayload::Workbench(snapshot), Some(PendingRequest::WorkbenchQuery(query))) => {
            if !model.accept_workbench_snapshot(query, snapshot.clone()) {
                return Vec::new();
            }
            if let Some(effects) = model.complete_command_snapshot_refresh() {
                return effects;
            }
            if let Some(effects) = model.complete_file_snapshot_refresh() {
                return effects;
            }
            if model.chat.workbench.files.open && model.chat.workbench.files.list {
                return model.complete_file_list_refresh();
            }
            if model.chat.workbench.memory_open() {
                return model.refresh_memory();
            }
            if model.chat.workbench.init_open() {
                return model.refresh_init();
            }
        }
        (
            AppResponsePayload::WorkbenchContinuationAdmission(admission),
            Some(PendingRequest::WorkbenchContinuationAdmission(command)),
        ) => return model.accept_workbench_continuation_admission(&command, *admission),
        (
            AppResponsePayload::WorkbenchReceipt(receipt),
            Some(PendingRequest::WorkbenchControl(command)),
        ) => return model.accept_workbench_receipt(&command, receipt),
        (
            AppResponsePayload::WorkbenchReceipt(receipt),
            Some(PendingRequest::WorkbenchReceipt(command)),
        ) => return model.observe_workbench_receipt(&command, receipt),
        (AppResponsePayload::Doctor(report), Some(PendingRequest::Doctor(query))) => {
            model.accept_doctor(query, report.clone());
        }
        _ => {}
    }
    Vec::new()
}

const fn is_setup_payload(payload: &AppResponsePayload) -> bool {
    matches!(
        payload,
        AppResponsePayload::WorkbenchRewindPreview(_)
            | AppResponsePayload::WorkbenchCheckpoint(_)
            | AppResponsePayload::WorkbenchRestore(_)
            | AppResponsePayload::WorkbenchPermissions(_)
            | AppResponsePayload::InitProposal(_)
            | AppResponsePayload::WorkbenchMemory(_)
            | AppResponsePayload::WorkbenchCompactionPreview(_)
            | AppResponsePayload::ConversationLibrary(_)
            | AppResponsePayload::WorkbenchFileImportPreview(_)
            | AppResponsePayload::WorkbenchReview(_)
    )
}
