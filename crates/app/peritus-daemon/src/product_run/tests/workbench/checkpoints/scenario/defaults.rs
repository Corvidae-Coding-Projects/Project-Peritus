//! Historical small-name scenario defaults shared by the complete-metadata fixture.

use super::{WorkbenchFileRange, WorkbenchRewindMode, checkpoint_scenario_with_name};

pub(in crate::product_run::tests::workbench::checkpoints) async fn checkpoint_scenario(
    user_conflict: bool,
    crash: Option<crate::product_run::workbench::RewindFaultPoint>,
    mode: WorkbenchRewindMode,
) {
    checkpoint_scenario_with_selection(user_conflict, crash, mode, WorkbenchFileRange::All).await;
}

pub(in crate::product_run::tests::workbench::checkpoints) async fn checkpoint_scenario_with_selection(
    user_conflict: bool,
    crash: Option<crate::product_run::workbench::RewindFaultPoint>,
    mode: WorkbenchRewindMode,
    selection: WorkbenchFileRange,
) {
    checkpoint_scenario_with_name(
        user_conflict,
        crash,
        mode,
        selection,
        "before Peritus edit".to_owned(),
    )
    .await;
}
