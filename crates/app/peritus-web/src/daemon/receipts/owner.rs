//! A retained native request stays bound to its exact store, endpoint, and IPC session.
use super::{AppRequestEnvelope, NativeOwner, Result, Value, problem};
use crate::error::uncertain;

pub(super) fn retained(input: &Value, original: &AppRequestEnvelope) -> Result<Option<NativeOwner>> {
    let Some(value) = input.get("owner") else { return Ok(None) };
    let owner: NativeOwner = serde_json::from_value(value.clone()).map_err(|error| {
        uncertain(format!("The original native owner is unreadable: {error}"))
    })?;
    if owner.session_id()? != original.context().session_id() {
        return Err(problem("The retained request and native owner have different IPC sessions"));
    }
    Ok(Some(owner))
}

pub(super) fn require(
    input: &Value, original: &AppRequestEnvelope, expected: &NativeOwner,
) -> Result<NativeOwner> {
    let owner = retained(input, original)?.ok_or_else(|| uncertain(
        "The legacy request has no retained native owner. Its unknown effect cannot be rebound or retransmitted.",
    ))?;
    if &owner != expected {
        return Err(problem("The original native request belongs to another execution owner"));
    }
    Ok(owner)
}
