//! Product-operation fixture values.

use crate::{
    ProductRunControlAction, ProductRunLegalControls, ProductRunOperation, ProductRunOperationKind,
    ProductRunOperationState,
};
use peritus_types::RunId;

pub(super) fn run_operation(run: RunId, state: ProductRunOperationState) -> ProductRunOperation {
    let identity = run.as_bytes().iter().fold("run/".to_owned(), |mut value, byte| {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    });
    let mut controls = ProductRunLegalControls::none();
    if state == ProductRunOperationState::Running {
        controls = controls.with(ProductRunControlAction::Cancel);
    } else if matches!(
        state,
        ProductRunOperationState::Failed
            | ProductRunOperationState::Cancelled
            | ProductRunOperationState::RecoveryRequired
    ) {
        controls = controls.with(ProductRunControlAction::Retry);
    }
    ProductRunOperation::new(
        ProductRunOperationKind::Execution,
        state,
        identity,
        "The fixture operation owner supplied this exact observation.".to_owned(),
        String::new(),
        controls,
    )
    .expect("fixture operation")
}
