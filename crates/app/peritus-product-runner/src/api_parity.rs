//! Compile-time regression for the daemon-facing ordinary and verification-only API surface.

use std::{path::PathBuf, time::Duration};

use peritus_process::ProcessStore;
use peritus_types::{RevisionTuple, RunId};
use peritus_workspace::WorkspaceAuthorizationRequest;

use crate::{
    CommandRuntime, ConversationView, FolderPatchAuthority, FolderPatchAuthorityPlan,
    FolderPatchAuthorityPlanRequest, LocalContextConfig, PreviewCommand, PreviewLaunch,
    PreviewObservation, PreviewProcessState, ProductRunnerError, WorkspaceMutationKind,
    checked_protected_file,
};

#[allow(dead_code, clippy::too_many_arguments)]
fn constructors(
    state_root: PathBuf,
    workspace_root: PathBuf,
    direct_state_root: PathBuf,
    direct_workspace_root: PathBuf,
    run_id: RunId,
    direct_run_id: RunId,
    process_store: ProcessStore,
    direct_process_store: ProcessStore,
    local_context: LocalContextConfig,
) {
    let _: Result<CommandRuntime, ProductRunnerError> =
        CommandRuntime::open(state_root, workspace_root, run_id, process_store)
            .and_then(|runtime| runtime.with_local_context(local_context));
    let _: Result<CommandRuntime, ProductRunnerError> = CommandRuntime::open_direct(
        direct_state_root,
        direct_workspace_root,
        direct_run_id,
        direct_process_store,
    );
}

#[allow(dead_code)]
fn command_effects(
    runtime: &CommandRuntime,
    request: FolderPatchAuthorityPlanRequest,
    plan: FolderPatchAuthorityPlan,
    command: &PreviewCommand,
    launch: &PreviewLaunch,
) {
    let _: Result<FolderPatchAuthorityPlan, ProductRunnerError> =
        runtime.plan_folder_patch_authority(request);
    let _: RevisionTuple = plan.revision();
    let _ = plan.lease_holder();
    let _ = plan.resource_id();
    let _ = plan.environment_id();
    let _: Result<FolderPatchAuthority, ProductRunnerError> =
        runtime.commit_folder_patch_authority(plan, Vec::new());
    let _: Result<PreviewLaunch, ProductRunnerError> = runtime.launch_preview(command);
    let _: Result<PreviewObservation, ProductRunnerError> = runtime.observe_preview(launch);
    let _: Result<PreviewObservation, ProductRunnerError> =
        runtime.interact_preview(launch, Vec::new());
    let _: Result<PreviewObservation, ProductRunnerError> = runtime.stop_preview(launch);
    let _: Result<PreviewObservation, ProductRunnerError> = runtime.run_preview_helper(command);
}

#[allow(dead_code)]
fn folder_authority(authority: &FolderPatchAuthority) {
    let _: WorkspaceAuthorizationRequest<'_> = authority.request();
}

#[allow(dead_code)]
fn preview_values(
    command: &PreviewCommand,
    launch: &PreviewLaunch,
    observation: &PreviewObservation,
) {
    let _: Result<PreviewCommand, ProductRunnerError> = PreviewCommand::new(
        String::new(),
        Vec::new(),
        PathBuf::new(),
        Duration::from_secs(1),
        false,
        1,
        1,
        String::new(),
        Vec::new(),
    );
    let _ = command;
    let _ = launch.process_id();
    let _: PreviewProcessState = observation.state();
    let _: &str = observation.stdout();
    let _: &str = observation.stderr();
    let _: Option<i64> = observation.exit_code();
    let _: &[String] = observation.progress();
}

#[allow(dead_code)]
fn conversation(view: &dyn ConversationView) {
    let _: bool = view.uses_explicit_media();
    let _: u64 = view.revision();
    let _: u64 = view.incorporated_revision();
    let _: String = view.render();
    let _: String = view.stable_request_context();
    let _: Vec<PathBuf> = view.protected_paths();
    let _: crate::control::HostPermissions = view.effective_permissions();
    let _: bool = view.permits_pipeline_handoff();
    let _: Result<(), String> = view.checkpoint_before_workspace_mutation(
        std::path::Path::new("candidate.txt"),
        WorkspaceMutationKind::File,
    );
    let _: Result<(), String> = view.seal_workspace_mutation_checkpoint(
        std::path::Path::new("candidate.txt"),
        WorkspaceMutationKind::File,
        crate::control::CheckpointFileVersion::Absent,
    );
}

#[allow(dead_code)]
fn protected_file(root: &std::path::Path, relative: &str, contract: &str, protected: &[PathBuf]) {
    let _: Result<PathBuf, ProductRunnerError> =
        checked_protected_file(root, relative, contract, protected);
}

#[allow(dead_code)]
fn shared_control_and_attachment(
    _conversation: crate::control::ConversationId,
    _file: crate::attachment::ValidatedFileText,
    _input: peritus_agent::DeveloperInput,
    _admission: peritus_agent::DeveloperRequestAdmission,
) {
}
