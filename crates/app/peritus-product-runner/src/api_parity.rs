//! Compile-time regression for the daemon-facing ordinary and verification-only API surface.

use std::{path::PathBuf, time::Duration};

use peritus_process::ProcessStore;
use peritus_run_settlement::CandidateCheckpoint;
use peritus_types::{ActionId, ProcessId, RevisionTuple, RunId};
use peritus_workspace::WorkspaceAuthorizationRequest;

use crate::{
    CommandRuntime, ConversationView, FolderPatchAuthority, FolderPatchAuthorityPlan,
    FolderPatchAuthorityPlanRequest, LocalContextConfig, PreviewCommand, PreviewLaunch,
    PreviewObservation, PreviewProcessState, ProductRunResume, ProductRunnerError, UncertainEffect,
    UncertainEffectState, WorkspaceMutationKind, acknowledge_uncertain_effect,
    checked_protected_file, uncertain_effects,
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
    let _: Result<Vec<(RunId, ActionId, ProcessId)>, ProductRunnerError> =
        CommandRuntime::receipt_linked_live_owners(
            std::path::Path::new("effects.bin"),
            run_id,
            &process_store,
        );
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
    let _: Result<(), ProductRunnerError> =
        runtime.reconcile_effect_receipts(std::path::Path::new("effects.bin"));
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
    let _: Result<crate::PreviewTerminal, ProductRunnerError> = runtime.preview_terminal(launch);
    let _: Result<PreviewObservation, ProductRunnerError> =
        runtime.interact_preview(launch, Vec::new());
    let _: Result<PreviewObservation, ProductRunnerError> = runtime.stop_preview(launch);
    let _: Result<PreviewObservation, ProductRunnerError> = runtime.run_preview_helper(command);
}

#[allow(dead_code)]
fn terminal_lease(lease: crate::PreviewTerminal) {
    let _: &peritus_process::ExecutionPlan = lease.plan();
    let _: peritus_process::ProcessControl = lease.control();
    let _: Result<peritus_process::TerminalResult, ProductRunnerError> = lease.wait();
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
    let _: String = view.reference_authority_context();
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
fn uncertain_effect(effect: &UncertainEffect, path: &std::path::Path) {
    let _: &str = effect.identity();
    let _: &str = effect.tool();
    let _: UncertainEffectState = effect.state();
    let _: Option<u64> = effect.requirements_revision();
    let _: bool = effect.owner_inactive();
    let _: Result<Vec<UncertainEffect>, ProductRunnerError> = uncertain_effects(path);
    let _: Result<(), ProductRunnerError> = acknowledge_uncertain_effect(path, effect.identity());
}

#[allow(dead_code)]
fn retained_resume(
    bytes: &[u8],
    transcript: &str,
    resume: ProductRunResume,
    checkpoint: &CandidateCheckpoint,
) {
    let _: Result<ProductRunResume, ProductRunnerError> =
        ProductRunResume::decode_durable(bytes, transcript);
    let _: Result<ProductRunResume, ProductRunnerError> =
        ProductRunResume::decode_durable_retained(bytes, transcript);
    let _: Result<ProductRunResume, ProductRunnerError> = resume.reconcile_candidate(*checkpoint);
}

#[allow(dead_code)]
fn shared_control_and_attachment(
    _conversation: crate::control::ConversationId,
    _file: crate::attachment::ValidatedFileText,
    _input: peritus_agent::DeveloperInput,
    _admission: peritus_agent::DeveloperRequestAdmission,
) {
}

#[allow(dead_code)]
fn task_baseline(
    workspace: &std::path::Path,
    trace: &std::path::Path,
    bytes: &str,
    paths: &[String],
    binding: peritus_types::Sha256Digest,
    candidate: peritus_types::Sha256Digest,
) {
    let _: Result<peritus_types::Sha256Digest, ProductRunnerError> =
        crate::ProductRunner::candidate_source_digest(workspace);
    let _: Result<String, ProductRunnerError> = crate::ProductRunner::retained_task_baseline(trace);
    let _: Result<(), ProductRunnerError> = crate::ProductRunner::validate_task_baseline(bytes);
    let _: Result<Vec<u8>, ProductRunnerError> =
        crate::ProductRunner::candidate_patch_from_baseline(workspace, bytes);
    let _: Result<Vec<PathBuf>, ProductRunnerError> =
        crate::ProductRunner::discard_from_baseline(workspace, bytes, paths);
    let _: Result<peritus_types::Sha256Digest, ProductRunnerError> =
        crate::ProductRunner::prepare_discard_transaction(
            workspace, bytes, paths, trace, binding, candidate,
        );
    let _: Result<Option<crate::DiscardTransactionState>, ProductRunnerError> =
        crate::ProductRunner::inspect_discard_transaction(trace, binding, candidate);
    let _: Result<Vec<PathBuf>, ProductRunnerError> =
        crate::ProductRunner::execute_discard_transaction(trace, binding, candidate);
}
