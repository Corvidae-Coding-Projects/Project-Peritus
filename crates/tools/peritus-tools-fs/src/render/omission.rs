//! Bounded serialization and continuation arithmetic for scope omissions.

use peritus_tool_protocol::BoundedJson;

use crate::{DiscoverObservation, FsToolError, OmissionReason, ScopeOmission, SearchObservation};

use super::{
    json::{object, string},
    protocol_error,
};

pub(super) fn discover_next_omission(
    value: &DiscoverObservation,
    start: u64,
    rendered: usize,
) -> Result<Option<u64>, FsToolError> {
    let end = start
        .checked_add(u64::try_from(rendered).map_err(|_| protocol_error())?)
        .ok_or_else(protocol_error)?;
    let page_end = start
        .checked_add(u64::try_from(value.omissions().len()).map_err(|_| protocol_error())?)
        .ok_or_else(protocol_error)?;
    if end < page_end { Ok(Some(end)) } else { Ok(value.next_omission_offset()) }
}

pub(super) fn next_omission_offset(
    value: &SearchObservation,
    start: usize,
    rendered: usize,
    offset: u64,
) -> Result<Option<u64>, FsToolError> {
    if start.saturating_add(rendered) < value.omissions().len() {
        Ok(Some(
            offset
                .checked_add(u64::try_from(rendered).map_err(|_| protocol_error())?)
                .ok_or_else(protocol_error)?,
        ))
    } else {
        Ok(value.next_omission_offset())
    }
}

pub(super) fn omission_json(value: &ScopeOmission) -> Result<BoundedJson, FsToolError> {
    object(vec![
        (value.path_field(), string(value.path_value())),
        ("reason", string(omission_name(value.reason()).to_owned())),
    ])
}

pub(super) const fn omission_name(value: OmissionReason) -> &'static str {
    match value {
        OmissionReason::DepthLimit => "depth_limit",
        OmissionReason::UnsafeEntry => "unsafe_entry",
        OmissionReason::FileByteLimit => "file_byte_limit",
        OmissionReason::BinaryContent => "binary_content",
        OmissionReason::UnsupportedName => "unsupported_name",
        OmissionReason::UnsupportedType => "unsupported_type",
    }
}
