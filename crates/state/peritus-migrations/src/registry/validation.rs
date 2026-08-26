//! Validation helpers for reviewed migration descriptors.

use crate::{MigrationError, MigrationErrorCode, RecoveryClass};

pub(super) fn reject_transaction_control(sql: &str) -> Result<(), MigrationError> {
    let uppercase = sql.to_ascii_uppercase();
    for forbidden in ["BEGIN", "COMMIT", "ROLLBACK", "ATTACH", "DETACH", "VACUUM"] {
        if uppercase
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
            .any(|token| token == forbidden)
        {
            return Err(invalid_registry(
                "migration SQL must not control transactions, attachment, or vacuum",
            ));
        }
    }
    Ok(())
}

pub(super) const fn invalid_registry(message: &'static str) -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::InvalidRegistry,
        RecoveryClass::CorrectRequest,
        "validate migration registry",
        message,
    )
}
