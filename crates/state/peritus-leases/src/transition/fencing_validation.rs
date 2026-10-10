//! Command-specific executable fencing guards and their exact specifications.

use super::validation::validate_claim_identity;
use crate::{LeaseError, ReleaseLease};

pub(super) const fn validate_release_quiescence(
    command: &ReleaseLease,
) -> Result<bool, LeaseError> {
    let quiescence = command.quiescence();
    let Some(evidence) = quiescence else {
        return Ok(false);
    };
    let evidence_claim = evidence.claim();
    let matches = validate_claim_identity(command.claim, evidence_claim).is_ok();
    if matches { Ok(true) } else { Err(LeaseError::HolderQuiescenceMismatch) }
}
