use peritus_app_protocol::{
    ProductRunControlAction, ProductRunLegalControls, ProductRunOperation, ProductRunOperationKind,
    ProductRunOperationState, ProductRunPhase,
};
use peritus_types::RunId;

pub fn run_operation(run: RunId, phase: ProductRunPhase) -> ProductRunOperation {
    let state = match phase {
        ProductRunPhase::Queued
        | ProductRunPhase::Designing
        | ProductRunPhase::Writing
        | ProductRunPhase::Checking
        | ProductRunPhase::Reviewing
        | ProductRunPhase::Fixing
        | ProductRunPhase::Verifying => ProductRunOperationState::Running,
        ProductRunPhase::WaitingForUser => ProductRunOperationState::WaitingForUser,
        ProductRunPhase::Complete => ProductRunOperationState::Succeeded,
        ProductRunPhase::Failed => ProductRunOperationState::Failed,
        ProductRunPhase::Cancelled => ProductRunOperationState::Cancelled,
        ProductRunPhase::RecoveryRequired => ProductRunOperationState::RecoveryRequired,
    };
    let mut controls = ProductRunLegalControls::none();
    if matches!(state, ProductRunOperationState::Running | ProductRunOperationState::WaitingForUser)
    {
        controls = controls.with(ProductRunControlAction::Cancel);
    }
    if matches!(
        state,
        ProductRunOperationState::Failed
            | ProductRunOperationState::Cancelled
            | ProductRunOperationState::RecoveryRequired
    ) {
        controls = controls.with(ProductRunControlAction::Retry);
    }
    let identity = run.as_bytes().iter().fold("run/".to_owned(), |mut value, byte| {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    });
    ProductRunOperation::new(
        ProductRunOperationKind::Execution,
        state,
        identity,
        "The TUI fixture supplied this exact operation observation.".to_owned(),
        if state == ProductRunOperationState::RecoveryRequired {
            "The fixture execution was interrupted before settlement.".to_owned()
        } else {
            String::new()
        },
        controls,
    )
    .expect("fixture operation")
}
