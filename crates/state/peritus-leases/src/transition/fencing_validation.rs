//! Command-specific executable fencing guards and their exact specifications.

use super::validation::validate_claim_identity;
use crate::{LeaseError, ReleaseLease};
use vstd::prelude::*;

verus! {

pub(super) const fn validate_release_quiescence(
    command: &ReleaseLease,
) -> (result: Result<bool, LeaseError>)
    ensures
        match result {
            Ok(direct_available) => match command.spec_quiescence() {
                Some(evidence) => {
                    direct_available
                        && crate::model::concrete_claim_matches(
                            evidence.spec_claim(),
                            command.claim,
                        )
                }
                None => !direct_available,
            },
            Err(error) => {
                error == LeaseError::HolderQuiescenceMismatch
                    && match command.spec_quiescence() {
                        Some(evidence) => !crate::model::concrete_claim_matches(
                            evidence.spec_claim(),
                            command.claim,
                        ),
                        None => false,
                    }
            }
        },
{
    let quiescence = command.quiescence();
    proof {
        assert(quiescence == command.spec_quiescence());
    }
    let Some(evidence) = quiescence else {
        return Ok(false);
    };
    let evidence_claim = evidence.claim();
    proof {
        assert(evidence_claim == evidence.spec_claim());
    }
    let matches = validate_claim_identity(command.claim, evidence_claim).is_ok();
    proof {
        assert(matches == crate::model::concrete_claim_matches(
            evidence.spec_claim(),
            command.claim,
        ));
    }
    if matches {
        Ok(true)
    } else {
        Err(LeaseError::HolderQuiescenceMismatch)
    }
}

} // verus!
