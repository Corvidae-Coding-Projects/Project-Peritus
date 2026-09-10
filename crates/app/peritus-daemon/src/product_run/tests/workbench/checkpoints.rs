//! Observed checkpoint -> owned edit -> rewind behavior through the daemon service.

use super::*;
use peritus_app_protocol::{
    ProductInteractionMode, ProductModelChoice, ProductRoleModels, WorkbenchCheckpointName,
    WorkbenchExecutionSettings, WorkbenchFileMode, WorkbenchFileRange, WorkbenchFileRequest,
    WorkbenchRestoreStatus, WorkbenchRewindDisposition, WorkbenchRewindMode,
    WorkbenchRewindRequest,
};
use peritus_product_runner::control::{CheckpointId, ConversationId as DomainConversationId};

#[test]
fn checkpoint_owned_edit_preview_and_restore_round_trip_exact_bytes_and_journal() {
    interaction::block_on(checkpoint_scenario(false, None, WorkbenchRewindMode::FilesOnly));
}

#[test]
fn independent_user_edit_is_a_conflict_and_preserves_every_current_byte() {
    interaction::block_on(checkpoint_scenario(true, None, WorkbenchRewindMode::FilesOnly));
}

#[test]
fn prepared_restore_before_c1_recovers_as_safe_no_effect_conflict() {
    interaction::block_on(checkpoint_scenario(
        false,
        Some(crate::product_run::workbench::RewindFaultPoint::AfterPrepare),
        WorkbenchRewindMode::FilesOnly,
    ));
}

#[test]
fn applied_folder_patch_before_settlement_recovers_as_applied_idempotently() {
    interaction::block_on(checkpoint_scenario(
        false,
        Some(crate::product_run::workbench::RewindFaultPoint::AfterFolderPatch),
        WorkbenchRewindMode::FilesOnly,
    ));
}

#[test]
fn actual_c1_indeterminate_error_remains_prepared_for_authorized_reconciliation() {
    interaction::block_on(checkpoint_scenario(
        false,
        Some(crate::product_run::workbench::RewindFaultPoint::InsideFolderPatch),
        WorkbenchRewindMode::FilesOnly,
    ));
}

#[test]
fn conversation_rewind_preserves_independent_files_and_works_without_write_permission() {
    interaction::block_on(checkpoint_scenario(true, None, WorkbenchRewindMode::ConversationOnly));
}

#[test]
fn combined_rewind_publishes_historical_branch_only_after_files_settle() {
    interaction::block_on(checkpoint_scenario(false, None, WorkbenchRewindMode::Combined));
}

#[test]
fn combined_rewind_recovers_completed_files_before_publishing_branch() {
    interaction::block_on(checkpoint_scenario(
        false,
        Some(crate::product_run::workbench::RewindFaultPoint::AfterFolderPatch),
        WorkbenchRewindMode::Combined,
    ));
}

#[test]
fn combined_rewind_conflict_never_publishes_a_child() {
    interaction::block_on(checkpoint_scenario(true, None, WorkbenchRewindMode::Combined));
}

mod scenario;
use scenario::checkpoint_scenario;

mod support;
use support::pipeline_responses;
