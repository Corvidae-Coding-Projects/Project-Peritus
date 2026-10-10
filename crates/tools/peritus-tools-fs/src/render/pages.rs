use super::{
    RenderedOutput, digest_hex, finish, fragments, kind_name, object, protocol_error, string,
    u64_integer,
};
use crate::{
    DiscoverObservation, FileContent, FileObservation, FsToolError, FsToolErrorKind,
    MetadataObservation, SearchMatchField, SearchObservation,
};
use peritus_tool_protocol::BoundedJson;

impl RenderedOutput {
    /// Renders metadata within the call's encoded-byte limit.
    ///
    /// # Errors
    /// Returns a typed protocol failure when even the compact metadata summary cannot fit.
    pub fn metadata_with_output_bound(
        value: &MetadataObservation,
        output_bytes: u64,
    ) -> Result<Self, FsToolError> {
        let rendered = Self::metadata(value)?;
        if rendered.structured().canonical_bytes().len() as u64 <= output_bytes {
            return Ok(rendered);
        }
        let structured = object(vec![
            ("executable", Ok(BoundedJson::boolean(value.executable()))),
            ("kind", string(kind_name(value).to_owned())),
            ("path", Ok(BoundedJson::null())),
            ("path_in_arguments", Ok(BoundedJson::boolean(true))),
            ("size", u64_integer(value.size())),
        ])?;
        if structured.canonical_bytes().len() as u64 > output_bytes {
            return Err(protocol_error());
        }
        let text = format!(
            "Observed {} bytes, {}, executable={}",
            value.size(),
            kind_name(value),
            value.executable()
        );
        finish(structured, text.clone(), text, false)
    }

    /// Renders exact file bytes within the call's encoded-byte limit.
    ///
    /// The requested path remains in the prepared call when a compact page must refer to it.
    ///
    /// # Errors
    /// Returns a typed protocol failure when the output limit cannot hold one advancing byte.
    pub fn file_with_output_bound(
        value: &FileObservation,
        output_bytes: u64,
    ) -> Result<Self, FsToolError> {
        let rendered = Self::file(value, output_bytes);
        if let Ok(rendered) = rendered {
            return Ok(rendered);
        }
        let (encoding, content) = match value.content() {
            FileContent::Utf8(text) => ("utf8", text.clone()),
            FileContent::Base64(text) => ("base64", text.clone()),
        };
        let metadata = value.metadata();
        let metadata_json = object(vec![
            ("executable", Ok(BoundedJson::boolean(metadata.executable()))),
            ("kind", string(kind_name(metadata).to_owned())),
            ("path", Ok(BoundedJson::null())),
            ("path_in_arguments", Ok(BoundedJson::boolean(true))),
            ("size", u64_integer(metadata.size())),
        ])?;
        let structured = object(vec![
            ("content", string(content)),
            ("content_digest", string(digest_hex(value.content_digest()))),
            ("encoding", string(encoding.to_owned())),
            ("metadata", Ok(metadata_json)),
            ("range_end", u64_integer(value.range().1)),
            ("range_start", u64_integer(value.range().0)),
            ("source_digest", string(digest_hex(value.source_digest()))),
            (
                "continuation_offset",
                value.continuation_offset().map_or_else(|| Ok(BoundedJson::null()), u64_integer),
            ),
        ])?;
        if structured.canonical_bytes().len() as u64 > output_bytes {
            return Err(protocol_error());
        }
        let text = format!(
            "Read bytes {} through {} of {} as {encoding}.",
            value.range().0,
            value.range().1,
            metadata.size(),
        );
        finish(structured, text.clone(), text, value.continuation_offset().is_some())
    }

    /// Renders an encoded-byte-sized discovery page, including a resumable oversized path.
    ///
    /// # Errors
    /// Returns a typed protocol failure when the output limit cannot hold a progressing fragment.
    pub fn discover_page_with_path_range(
        value: &DiscoverObservation,
        offset: u64,
        maximum_items: u32,
        path_offset: Option<u64>,
        output_bytes: u64,
    ) -> Result<Self, FsToolError> {
        if let Some(path_offset) = path_offset {
            let index =
                usize::try_from(offset.checked_sub(value.page_start()).ok_or_else(protocol_error)?)
                    .map_err(|_| protocol_error())?;
            let entry = value.entries().get(index).ok_or_else(protocol_error)?;
            return fragments::discover_path_page(value, offset, entry, path_offset, output_bytes);
        }

        match Self::discover_page(value, offset, maximum_items, output_bytes) {
            Ok(rendered) => Ok(rendered),
            Err(error) if error.kind() == FsToolErrorKind::Protocol => {
                let index = usize::try_from(
                    offset.checked_sub(value.page_start()).ok_or_else(protocol_error)?,
                )
                .map_err(|_| protocol_error())?;
                value.entries().get(index).map_or_else(
                    || fragments::discover_empty_page(value, output_bytes),
                    |entry| fragments::discover_path_page(value, offset, entry, 0, output_bytes),
                )
            }
            Err(error) => Err(error),
        }
    }

    /// Renders encoded-byte-sized search pages with exact match and omission field cursors.
    ///
    /// # Errors
    /// Returns a typed protocol failure when the output limit cannot hold a progressing fragment.
    pub fn search_page_with_field_ranges(
        value: &SearchObservation,
        offset: u64,
        maximum_items: u32,
        omission_offset: u64,
        match_field: Option<(SearchMatchField, u64)>,
        omission_path_offset: Option<u64>,
        output_bytes: u64,
    ) -> Result<Self, FsToolError> {
        if let Some((field, match_field_offset)) = match_field {
            let relative = offset.checked_sub(value.page_start()).ok_or_else(protocol_error)?;
            let relative = usize::try_from(relative).map_err(|_| protocol_error())?;
            let matched = value.matches().get(relative).ok_or_else(protocol_error)?;
            return fragments::search_match_path_page(
                value,
                offset,
                matched,
                field,
                match_field_offset,
                omission_offset,
                output_bytes,
            );
        }
        if let Some(path_offset) = omission_path_offset {
            let index = usize::try_from(
                omission_offset
                    .checked_sub(value.omission_page_start())
                    .ok_or_else(protocol_error)?,
            )
            .map_err(|_| protocol_error())?;
            let omission = value.omissions().get(index).ok_or_else(protocol_error)?;
            return fragments::search_omission_path_page(
                value,
                offset,
                omission_offset,
                omission,
                path_offset,
                output_bytes,
            );
        }

        match Self::search_page(value, offset, maximum_items, omission_offset, output_bytes) {
            Ok(rendered) => Ok(rendered),
            Err(error) if error.kind() == FsToolErrorKind::Protocol => {
                let index = usize::try_from(
                    offset.checked_sub(value.page_start()).ok_or_else(protocol_error)?,
                )
                .map_err(|_| protocol_error())?;
                if let Some(matched) = value.matches().get(index) {
                    fragments::search_match_path_page(
                        value,
                        offset,
                        matched,
                        SearchMatchField::Path,
                        0,
                        omission_offset,
                        output_bytes,
                    )
                } else {
                    let index = usize::try_from(
                        omission_offset
                            .checked_sub(value.omission_page_start())
                            .ok_or_else(protocol_error)?,
                    )
                    .map_err(|_| protocol_error())?;
                    let omission = value.omissions().get(index).ok_or(error)?;
                    fragments::search_omission_path_page(
                        value,
                        offset,
                        omission_offset,
                        omission,
                        0,
                        output_bytes,
                    )
                }
            }
            Err(error) => Err(error),
        }
    }
}
