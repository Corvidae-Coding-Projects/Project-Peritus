//! Bounded canonical campaign collection helpers.

use crate::{
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery,
    ProductionHarnessBinding,
};
use peritus_types::Sha256Digest;

pub(super) fn arm_digest(binding: ProductionHarnessBinding) -> Sha256Digest {
    peritus_eval::HarnessArmBinding::new(
        binding.revision(),
        binding.harness_revision(),
        binding.materialization_receipt_digest(),
    )
    .digest()
}

pub(super) fn insert_unique<T: Ord>(
    values: &mut Vec<T>,
    value: T,
    limit: Option<usize>,
) -> Result<(), EvolutionError> {
    match values.binary_search(&value) {
        Ok(_) => Err(duplicate()),
        Err(index) => {
            if limit.is_some_and(|limit| values.len() >= limit) {
                return Err(limit_error());
            }
            values.insert(index, value);
            Ok(())
        }
    }
}

pub(super) fn insert_by<T: PartialEq, K: Ord>(
    values: &mut Vec<T>,
    value: T,
    key: impl Fn(&T) -> K,
    limit: Option<usize>,
) -> Result<(), EvolutionError> {
    let value_key = key(&value);
    match values.binary_search_by_key(&value_key, key) {
        Ok(index) if values[index] == value => Err(duplicate()),
        Ok(_) => Err(binding()),
        Err(index) => {
            if limit.is_some_and(|limit| values.len() >= limit) {
                return Err(limit_error());
            }
            values.insert(index, value);
            Ok(())
        }
    }
}

const fn duplicate() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::Reconcile,
        "campaign item already has an immutable receipt",
    )
}

const fn binding() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::CorrectInput,
        "campaign repeats an admitted identity",
    )
}

const fn limit_error() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::LimitExceeded,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::SuccessorCampaign,
        "campaign workload policy requires an owner-approved scope expansion",
    )
}
