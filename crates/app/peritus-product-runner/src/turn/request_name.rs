//! Stable provider-request identities for one product role and cycle.

use peritus_types::RunId;

/// A fresh host invocation is new work, even when its role/cycle/revision did not change.
/// Its identity is retained by the existing context and effect journals before tool dispatch.
pub(super) fn invocation(
    run_id: RunId,
    role: &str,
    cycle: u32,
    revision: u64,
    invocation: u32,
) -> Result<String, crate::ProductRunnerError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        crate::ProductRunnerError::new(
            crate::ProductRunnerErrorKind::Repository,
            "allocate developer invocation identity",
            error.to_string(),
        )
    })?;
    let nonce = u128::from_be_bytes(bytes);
    Ok(format!(
        "{}-revision-{revision}-invocation-{invocation}-{nonce:032x}",
        format(run_id, role, cycle),
    ))
}

pub(super) fn format(run_id: RunId, role: &str, cycle: u32) -> String {
    let mut value = String::from("peritus-");
    for byte in run_id.as_bytes() {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    format!("{value}-{role}-{cycle}")
}
