//! Missing check contracts are harness prerequisites, not candidate defects for a fixer.

use peritus_gates::TargetGatePlan;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

pub(super) fn authorize(plan: &TargetGatePlan) -> Result<(), ProductRunnerError> {
    // Empty candidates have separate acceptance semantics, including authorized external effects.
    if plan.changed_paths().is_empty() || plan.has_complete_coverage() {
        return Ok(());
    }

    let missing = plan
        .uncovered_paths()
        .iter()
        .take(20)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let detail = if missing.is_empty() {
        "the affected projects have no complete executable check contract".to_owned()
    } else {
        format!("no applicable check contract was found for: {missing}")
    };
    Err(ProductRunnerError::new(
        ProductRunnerErrorKind::Gate,
        "plan exact-target checks",
        format!(
            "Acceptance is blocked: {detail}. Peritus did not run automatic checks or request a fixer for this missing contract. Completed edits are retained for inspection and an unqualified handoff. Configure an applicable project or workspace check contract before retrying."
        ),
    ))
}
