//! Durable construction coverage and capability-specific qualification for ordinary runs.

mod contract;
mod store;
#[cfg(test)]
mod tests;
mod workflow;

pub use contract::{Capability, DeliveryReview, DraftHandoff};
pub use store::Delivery;
pub use workflow::DEVELOPER_POLICY;

pub fn invalid(detail: impl Into<String>) -> crate::ProductRunnerError {
    crate::ProductRunnerError::new(
        crate::ProductRunnerErrorKind::InvalidModelOutput,
        "advance capability delivery",
        detail,
    )
}

pub fn digest(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

pub fn candidate(input: &crate::ProductRunInput) -> Result<String, crate::ProductRunnerError> {
    Ok(digest(input.checkpoint()?.digest().as_bytes()))
}

pub fn qualified(
    input: &crate::ProductRunInput,
    design: &crate::design::DesignDocument,
) -> Result<bool, crate::ProductRunnerError> {
    Ok(Delivery::load(input, design)?.is_qualified(&candidate(input)?))
}
