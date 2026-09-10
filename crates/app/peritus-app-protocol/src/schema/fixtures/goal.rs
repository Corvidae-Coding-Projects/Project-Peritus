//! Persistent-goal fixtures cover query, every user intent, evidence, and unknown usage.

use super::{
    FixtureClass, GeneratedFixtureCase,
    values::{context, encoded, id, request},
};
use crate::{
    AppRequestPayload, AppResponseEnvelope, AppResponsePayload, ControlOperationId, ConversationId,
    ProductInteractionMode, ProductProviderSelection, ProductRoleModels, WorkbenchCommand,
    WorkbenchExecutionSettings, WorkbenchGoalBudget, WorkbenchGoalCriterion,
    WorkbenchGoalCriterionDefinition, WorkbenchGoalCriterionKind, WorkbenchGoalCriterionState,
    WorkbenchGoalDefinition, WorkbenchGoalPauseMode, WorkbenchGoalRole, WorkbenchGoalRoleUsage,
    WorkbenchGoalSnapshot, WorkbenchGoalState, WorkbenchGoalUsage, WorkbenchInputText,
    WorkbenchIntent, WorkbenchQuery,
};
use peritus_codec::{CodecError, CodecLimits};

pub(super) fn cases(limits: CodecLimits) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let scope =
        WorkbenchQuery::new(id(81, ConversationId::new), id(82, peritus_types::WorkspaceId::new));
    let goal = id(83, ControlOperationId::new);
    let budget =
        WorkbenchGoalBudget::new(Some(600_000), Some(12), Some(48), Some(200_000)).expect("budget");
    let criterion = runner_criterion();
    let settings = execution_settings();
    let mut cases = vec![encoded(
        "minimal-workbench-goal-query",
        FixtureClass::Minimal,
        &request(AppRequestPayload::QueryWorkbenchGoal(scope)),
        limits,
    )?];
    cases.extend(command_cases(limits, scope, goal, budget, criterion.clone(), settings)?);
    cases.push(snapshot_case(limits, scope, goal, budget, criterion)?);
    Ok(cases)
}

fn command_cases(
    limits: CodecLimits,
    scope: WorkbenchQuery,
    goal: ControlOperationId,
    budget: WorkbenchGoalBudget,
    criterion: WorkbenchGoalCriterionDefinition,
    settings: WorkbenchExecutionSettings,
) -> Result<Vec<GeneratedFixtureCase>, CodecError> {
    let definition =
        WorkbenchGoalDefinition::new(objective(), vec![criterion], budget).expect("definition");
    let commands = [
        ("realistic-workbench-goal-start", WorkbenchIntent::StartGoal { definition, settings }),
        (
            "realistic-workbench-goal-pause",
            WorkbenchIntent::PauseGoal { goal, mode: WorkbenchGoalPauseMode::BeforeEdit },
        ),
        ("realistic-workbench-goal-resume", WorkbenchIntent::ResumeGoal { goal }),
        ("realistic-workbench-goal-budget", WorkbenchIntent::UpdateGoalBudget { goal, budget }),
        ("realistic-workbench-goal-clear", WorkbenchIntent::ClearGoal { goal }),
    ];
    commands
        .into_iter()
        .enumerate()
        .map(|(index, (name, intent))| {
            let byte = 88 + u8::try_from(index).expect("five operations");
            let command =
                WorkbenchCommand::new(id(byte, ControlOperationId::new), scope, 7, intent);
            encoded(
                name,
                FixtureClass::Realistic,
                &request(AppRequestPayload::WorkbenchCommand(command)),
                limits,
            )
        })
        .collect()
}

fn snapshot_case(
    limits: CodecLimits,
    scope: WorkbenchQuery,
    goal: ControlOperationId,
    budget: WorkbenchGoalBudget,
    criterion: WorkbenchGoalCriterionDefinition,
) -> Result<GeneratedFixtureCase, CodecError> {
    let graphical = WorkbenchGoalCriterionDefinition::new(
        WorkbenchGoalCriterionKind::GraphicalPlaytest,
        WorkbenchInputText::new("Exercise the native result".to_owned()).expect("criterion"),
        false,
    );
    let usage = WorkbenchGoalUsage::new(
        [
            WorkbenchGoalRoleUsage::new(WorkbenchGoalRole::Writer, 2, 2, 3, None, None),
            WorkbenchGoalRoleUsage::new(WorkbenchGoalRole::Reviewer, 1, 1, 0, Some(140), None),
            WorkbenchGoalRoleUsage::new(WorkbenchGoalRole::Fixer, 0, 0, 0, Some(0), Some(0)),
        ],
        42_000,
        90_000,
        1,
        0,
        1,
        12_000,
        4_000,
        64_000,
    );
    let snapshot = WorkbenchGoalSnapshot::new(
        scope,
        14,
        goal,
        id(84, peritus_types::RunId::new),
        objective(),
        WorkbenchGoalState::Pausing,
        "Pause durably requested; waiting for the selected safe boundary.".to_owned(),
        2,
        1,
        false,
        Some(WorkbenchGoalPauseMode::BeforeEdit),
        vec![
            WorkbenchGoalCriterion::new(criterion, WorkbenchGoalCriterionState::Pending, None),
            WorkbenchGoalCriterion::new(graphical, WorkbenchGoalCriterionState::Unavailable, None),
        ],
        budget,
        usage,
    )
    .expect("snapshot");
    let response = AppResponseEnvelope::new(
        context(),
        id(94, crate::RequestId::new),
        id(95, crate::CorrelationId::new),
        AppResponsePayload::WorkbenchGoal(snapshot),
    );
    encoded("realistic-workbench-goal", FixtureClass::Realistic, &response, limits)
}

fn execution_settings() -> WorkbenchExecutionSettings {
    WorkbenchExecutionSettings::new(
        id(84, peritus_types::RunId::new),
        ProductProviderSelection::new(
            id(85, peritus_types::ProviderProfileId::new),
            id(86, peritus_types::ProviderProfileId::new),
            id(87, peritus_types::ProviderProfileId::new),
        ),
        ProductInteractionMode::Build,
        ProductRoleModels::default(),
    )
}

fn runner_criterion() -> WorkbenchGoalCriterionDefinition {
    WorkbenchGoalCriterionDefinition::new(
        WorkbenchGoalCriterionKind::RunnerAcceptance,
        WorkbenchInputText::new("Strict runner acceptance".to_owned()).expect("criterion"),
        true,
    )
}

fn objective() -> WorkbenchInputText {
    WorkbenchInputText::new("Ship the exact durable change.".to_owned()).expect("objective")
}
