use super::*;

pub(in crate::product_run::tests::workbench::checkpoints) async fn checkpoint_scenario(
    user_conflict: bool,
    crash: Option<crate::product_run::workbench::RewindFaultPoint>,
    mode: WorkbenchRewindMode,
) {
    Box::pin(checkpoint_scenario_with_paging(user_conflict, crash, mode, false)).await;
}

pub(in crate::product_run::tests::workbench::checkpoints) async fn paged_confirmation_conflict_scenario()
 {
    Box::pin(checkpoint_scenario_with_paging(true, None, WorkbenchRewindMode::FilesOnly, true))
        .await;
}
