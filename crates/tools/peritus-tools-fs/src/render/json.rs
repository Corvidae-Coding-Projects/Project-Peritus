//! Capacity-aware JSON constructors for rendered filesystem pages.

use peritus_tool_protocol::{BoundedJson, JsonLimits};

use crate::FsToolError;

use super::protocol_error;

pub(super) fn object(
    members: Vec<(&str, Result<BoundedJson, FsToolError>)>,
) -> Result<BoundedJson, FsToolError> {
    let members = members
        .into_iter()
        .map(|(name, value)| value.map(|value| (name.to_owned(), value)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut capacity = 2_usize;
    for (index, (name, value)) in members.iter().enumerate() {
        if index > 0 {
            capacity = capacity.checked_add(1).ok_or_else(protocol_error)?;
        }
        capacity = capacity
            .checked_add(escaped_string_capacity(name.len()).ok_or_else(protocol_error)?)
            .and_then(|size| size.checked_add(1))
            .and_then(|size| size.checked_add(value.canonical_bytes().len()))
            .ok_or_else(protocol_error)?;
    }
    BoundedJson::object(members, json_limits_for_capacity(capacity)?).map_err(|_| protocol_error())
}

pub(super) fn array(values: Vec<BoundedJson>) -> Result<BoundedJson, FsToolError> {
    let mut capacity = 2_usize;
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            capacity = capacity.checked_add(1).ok_or_else(protocol_error)?;
        }
        capacity =
            capacity.checked_add(value.canonical_bytes().len()).ok_or_else(protocol_error)?;
    }
    BoundedJson::array(values, json_limits_for_capacity(capacity)?).map_err(|_| protocol_error())
}

pub(super) fn string(value: String) -> Result<BoundedJson, FsToolError> {
    let capacity = escaped_string_capacity(value.len()).ok_or_else(protocol_error)?;
    BoundedJson::string(value, json_limits_for_capacity(capacity)?).map_err(|_| protocol_error())
}

fn escaped_string_capacity(bytes: usize) -> Option<usize> {
    bytes.checked_mul(6)?.checked_add(2)
}

fn json_limits_for_capacity(capacity: usize) -> Result<JsonLimits, FsToolError> {
    let capacity = capacity.max(1);
    JsonLimits::new(capacity, capacity, capacity, capacity).map_err(|_| protocol_error())
}
