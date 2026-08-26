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
    limit: usize,
) -> Result<(), EvolutionError> {
    if values.len() >= limit {
        return Err(limit_error());
    }
    match values.binary_search(&value) {
        Ok(_) => Err(binding()),
        Err(index) => {
            values.insert(index, value);
            Ok(())
        }
    }
}

pub(super) fn insert_by<T, K: Ord>(
    values: &mut Vec<T>,
    value: T,
    key: impl Fn(&T) -> K,
    limit: usize,
) -> Result<(), EvolutionError> {
    if values.len() >= limit {
        return Err(limit_error());
    }
    let value_key = key(&value);
    match values.binary_search_by_key(&value_key, key) {
        Ok(_) => Err(binding()),
        Err(index) => {
            values.insert(index, value);
            Ok(())
        }
    }
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
        EvolutionRecovery::ReduceScope,
        "campaign collection exceeds its frozen bound",
    )
}
