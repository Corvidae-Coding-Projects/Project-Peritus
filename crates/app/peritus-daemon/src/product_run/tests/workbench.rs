//! Actual runner/provider admission over the authenticated, durable control ledger.

use super::*;
use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ConversationId, ConversationLibraryQuery,
    ConversationSearchText, ConversationTitle, CorrelationId, ProductInteractionMode,
    ProductRoleModels, ProductRunConversationQuery, WorkbenchBriefField, WorkbenchCaptureConsent,
    WorkbenchCaptureRequest, WorkbenchCaptureState, WorkbenchCaptureTarget, WorkbenchCommand,
    WorkbenchExecutionSettings, WorkbenchForkBudget, WorkbenchForkMode, WorkbenchForkRequest,
    WorkbenchGoalBudget, WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind,
    WorkbenchGoalDefinition, WorkbenchGoalPauseMode, WorkbenchGoalState, WorkbenchInputId,
    WorkbenchInputOrder, WorkbenchInputSelection, WorkbenchInputText, WorkbenchIntent,
    WorkbenchLaunchProfile, WorkbenchLaunchSource, WorkbenchLaunchSourceKind, WorkbenchLaunchState,
    WorkbenchLaunchText, WorkbenchNewInput, WorkbenchQuery, WorkbenchQueueIntent,
    WorkbenchResultQuery, WorkbenchReviewAnchor, WorkbenchReviewCommentState,
    WorkbenchReviewFeedback, WorkbenchReviewQuery, WorkbenchReviewTarget,
};
use peritus_product_runner::control::{ControlError, InputState};
use peritus_types::ActorId;

mod admission;
mod checkpoints;
mod files;
mod goals;
mod images;
mod init;
mod library;
mod memory;
mod permissions;
mod review;

mod preview;
mod tetris;

fn actor() -> ActorId {
    ActorId::new([1; 16]).expect("actor")
}

fn query(workspace: WorkspaceId) -> WorkbenchQuery {
    WorkbenchQuery::new(ConversationId::new([2; 16]).expect("conversation"), workspace)
}
fn command(
    workspace: WorkspaceId,
    id: u8,
    revision: u64,
    intent: WorkbenchIntent,
) -> WorkbenchCommand {
    WorkbenchCommand::new(
        ControlOperationId::new([id; 16]).expect("operation"),
        query(workspace),
        revision,
        intent,
    )
}
async fn queue(service: &ProductRunService, workspace: WorkspaceId) {
    let created = service
        .workbench_command(
            actor(),
            &command(
                workspace,
                3,
                0,
                WorkbenchIntent::CreateConversation(
                    ConversationTitle::new("governed test".to_owned()).expect("title"),
                ),
            ),
        )
        .await;
    assert!(matches!(created, AppResponsePayload::WorkbenchReceipt(_)));
    let input = WorkbenchInputId::new([4; 16]).expect("input");
    let enqueued = service
        .workbench_command(
            actor(),
            &command(
                workspace,
                5,
                1,
                WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                    WorkbenchNewInput::new(
                        input,
                        WorkbenchInputText::new("OBSOLETE_NEVER_SEND".to_owned()).expect("text"),
                        WorkbenchInputOrder::new(Vec::new()).expect("dependencies"),
                    )
                    .expect("input"),
                )),
            ),
        )
        .await;
    assert!(matches!(enqueued, AppResponsePayload::WorkbenchReceipt(_)));
    let edited = service
        .workbench_command(
            actor(),
            &command(
                workspace,
                6,
                2,
                WorkbenchIntent::Queue(WorkbenchQueueIntent::Edit {
                    selected: WorkbenchInputSelection::new(input, 1).expect("selection"),
                    text: WorkbenchInputText::new(
                        "CURRENT_EXACT_INSTRUCTION: say hello.".to_owned(),
                    )
                    .expect("text"),
                }),
            ),
        )
        .await;
    assert!(matches!(edited, AppResponsePayload::WorkbenchReceipt(_)));
}
fn start(
    workspace: WorkspaceId,
    run: RunId,
    providers: [&Arc<ScriptedProvider>; 3],
) -> WorkbenchCommand {
    command(
        workspace,
        7,
        3,
        WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            run,
            ProductProviderSelection::new(
                providers[0].profile.profile_id(),
                providers[1].profile.profile_id(),
                providers[2].profile.profile_id(),
            ),
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
        )),
    )
}

fn start_build(
    workspace: WorkspaceId,
    run: RunId,
    providers: [&Arc<ScriptedProvider>; 3],
) -> WorkbenchCommand {
    command(
        workspace,
        7,
        3,
        WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
            run,
            ProductProviderSelection::new(
                providers[0].profile.profile_id(),
                providers[1].profile.profile_id(),
                providers[2].profile.profile_id(),
            ),
            ProductInteractionMode::Build,
            ProductRoleModels::default(),
        )),
    )
}
