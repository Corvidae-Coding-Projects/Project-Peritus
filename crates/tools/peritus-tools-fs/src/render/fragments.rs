use super::{
    RenderedOutput, array, digest_hex, finish, integer, kind_name, object, omission_name,
    protocol_error, string, u64_integer,
};
use crate::{DiscoverObservation, FsToolError, ScopeOmission, SearchMatchField, SearchObservation};
use peritus_tool_protocol::BoundedJson;

pub(super) fn discover_path_page(
    value: &DiscoverObservation,
    index: u64,
    entry: &crate::DiscoverEntry,
    offset: u64,
    output_bytes: u64,
) -> Result<RenderedOutput, FsToolError> {
    let path = entry.metadata().path().as_str();
    let start = checked_string_offset(path, offset)?;
    let next_entry = index.checked_add(1).ok_or_else(protocol_error)?;
    let end = largest_prefix_end(path, start, |piece, end| {
        let complete = end == path.len();
        let next_offset = if complete && next_entry < value.observed_count() {
            Some(next_entry)
        } else if complete {
            None
        } else {
            Some(index)
        };
        let next_path_offset = (!complete).then(|| u64::try_from(end).ok()).flatten();
        discover_fragment_json(value, index, entry, offset, piece, next_offset, next_path_offset)
            .is_ok_and(|json| json.canonical_bytes().len() as u64 <= output_bytes)
    })?;
    let piece = &path[start..end];
    let complete = end == path.len();
    let next_offset = if complete && next_entry < value.observed_count() {
        Some(next_entry)
    } else if complete {
        None
    } else {
        Some(index)
    };
    let next_path_offset = (!complete).then(|| u64::try_from(end).ok()).flatten();
    let structured =
        discover_fragment_json(value, index, entry, offset, piece, next_offset, next_path_offset)?;
    let text = format!("Returned path bytes {offset} through {end} of {}.", path.len());
    let more = next_offset.is_some() || next_path_offset.is_some();
    finish(structured, text.clone(), text, more)
}

pub(super) fn discover_empty_page(
    value: &DiscoverObservation,
    output_bytes: u64,
) -> Result<RenderedOutput, FsToolError> {
    let structured = object(vec![
        ("digest", string(digest_hex(value.digest()))),
        ("entries", array(Vec::new())),
        ("omission_count", u64_integer(value.omission_count())),
        ("next_offset", Ok(BoundedJson::null())),
        ("next_path_offset", Ok(BoundedJson::null())),
        ("observed_count", u64_integer(value.observed_count())),
        ("root", Ok(BoundedJson::null())),
        ("root_in_arguments", Ok(BoundedJson::boolean(value.root().is_some()))),
        ("truncated", Ok(BoundedJson::boolean(false))),
    ])?;
    if structured.canonical_bytes().len() as u64 > output_bytes {
        return Err(protocol_error());
    }
    let text = format!(
        "Discovered no further entries; {} total entries were observed.",
        value.observed_count()
    );
    finish(structured, text.clone(), text, false)
}

fn discover_fragment_json(
    value: &DiscoverObservation,
    index: u64,
    entry: &crate::DiscoverEntry,
    offset: u64,
    piece: &str,
    next_offset: Option<u64>,
    next_path_offset: Option<u64>,
) -> Result<BoundedJson, FsToolError> {
    let metadata = entry.metadata();
    let fragment = object(vec![
        ("depth", Ok(integer(i64::from(entry.depth())))),
        (
            "metadata",
            object(vec![
                ("executable", Ok(BoundedJson::boolean(metadata.executable()))),
                ("kind", string(kind_name(metadata).to_owned())),
                ("path_fragment", string(piece.to_owned())),
                ("path_offset", u64_integer(offset)),
                (
                    "path_total_bytes",
                    u64_integer(
                        u64::try_from(metadata.path().as_str().len())
                            .map_err(|_| protocol_error())?,
                    ),
                ),
                ("size", u64_integer(metadata.size())),
            ]),
        ),
        (
            "omission",
            entry.omission_reason().map_or_else(
                || Ok(BoundedJson::null()),
                |reason| string(omission_name(reason).to_owned()),
            ),
        ),
        ("index", u64_integer(index)),
    ])?;
    object(vec![
        ("digest", string(digest_hex(value.digest()))),
        ("entries", array(vec![fragment])),
        ("omission_count", u64_integer(value.omission_count())),
        ("next_offset", next_offset.map_or_else(|| Ok(BoundedJson::null()), u64_integer)),
        ("next_path_offset", next_path_offset.map_or_else(|| Ok(BoundedJson::null()), u64_integer)),
        ("observed_count", u64_integer(value.observed_count())),
        ("root", Ok(BoundedJson::null())),
        ("root_in_arguments", Ok(BoundedJson::boolean(value.root().is_some()))),
        (
            "truncated",
            Ok(BoundedJson::boolean(next_offset.is_some() || next_path_offset.is_some())),
        ),
    ])
}

pub(super) fn search_match_path_page(
    value: &SearchObservation,
    index: u64,
    matched: &crate::SearchMatch,
    field: SearchMatchField,
    offset: u64,
    omission_offset: u64,
    output_bytes: u64,
) -> Result<RenderedOutput, FsToolError> {
    let source = match field {
        SearchMatchField::Path => matched.path().as_str(),
        SearchMatchField::Preview => matched.preview(),
    };
    let start = checked_string_offset(source, offset)?;
    let next_index = index.checked_add(1).ok_or_else(protocol_error)?;
    let end = largest_prefix_end(source, start, |piece, end| {
        let complete = end == source.len();
        let (next_match, next_field, next_field_offset) = match (field, complete) {
            (_, false) => (Some(index), Some(field), u64::try_from(end).ok()),
            (SearchMatchField::Path, true) => {
                (Some(index), Some(SearchMatchField::Preview), Some(0))
            }
            (SearchMatchField::Preview, true) => (next_search_match(value, next_index), None, None),
        };
        let next_omission = pending_omission(value, omission_offset);
        search_field_json(index, matched, field, offset, piece, source.len())
            .and_then(|match_field| {
                search_fragment_json(
                    value,
                    vec![match_field],
                    Vec::new(),
                    SearchFragmentCursor {
                        match_index: next_match,
                        omission_index: next_omission,
                        match_field: next_field,
                        match_field_offset: next_field_offset,
                        omission_path_offset: None,
                    },
                )
            })
            .is_ok_and(|json| json.canonical_bytes().len() as u64 <= output_bytes)
    })?;
    let piece = &source[start..end];
    let complete = end == source.len();
    let (next_match, next_field, next_field_offset) = match (field, complete) {
        (_, false) => (Some(index), Some(field), u64::try_from(end).ok()),
        (SearchMatchField::Path, true) => (Some(index), Some(SearchMatchField::Preview), Some(0)),
        (SearchMatchField::Preview, true) => (next_search_match(value, next_index), None, None),
    };
    let next_omission = pending_omission(value, omission_offset);
    let structured = search_fragment_json(
        value,
        vec![search_field_json(index, matched, field, offset, piece, source.len())?],
        Vec::new(),
        SearchFragmentCursor {
            match_index: next_match,
            omission_index: next_omission,
            match_field: next_field,
            match_field_offset: next_field_offset,
            omission_path_offset: None,
        },
    )?;
    let text = format!("Returned {}/{} bytes of match {index} {field:?}.", end, source.len());
    let more = next_match.is_some()
        || next_omission.is_some()
        || next_field.is_some()
        || next_field_offset.is_some();
    finish(structured, text.clone(), text, more)
}

pub(super) fn search_omission_path_page(
    value: &SearchObservation,
    match_offset: u64,
    index: u64,
    omission: &ScopeOmission,
    offset: u64,
    output_bytes: u64,
) -> Result<RenderedOutput, FsToolError> {
    let path = omission.path_value();
    let start = checked_string_offset(&path, offset)?;
    let next_index = index.checked_add(1).ok_or_else(protocol_error)?;
    let omission_page_end = value
        .omission_page_start()
        .checked_add(u64::try_from(value.omissions().len()).map_err(|_| protocol_error())?)
        .ok_or_else(protocol_error)?;
    let following_omission = if next_index < omission_page_end {
        Some(next_index)
    } else {
        value.next_omission_offset()
    };
    let end = largest_prefix_end(&path, start, |piece, end| {
        let complete = end == path.len();
        let next_omission = if complete { following_omission } else { Some(index) };
        search_omission_field_json(index, omission, offset, piece, path.len())
            .and_then(|omission_field| {
                search_fragment_json(
                    value,
                    Vec::new(),
                    vec![omission_field],
                    SearchFragmentCursor {
                        match_index: next_search_match(value, match_offset),
                        omission_index: next_omission,
                        match_field: None,
                        match_field_offset: None,
                        omission_path_offset: (!complete)
                            .then(|| u64::try_from(end).ok())
                            .flatten(),
                    },
                )
            })
            .is_ok_and(|json| json.canonical_bytes().len() as u64 <= output_bytes)
    })?;
    let piece = &path[start..end];
    let complete = end == path.len();
    let next_omission = if complete { following_omission } else { Some(index) };
    let structured = search_fragment_json(
        value,
        Vec::new(),
        vec![search_omission_field_json(index, omission, offset, piece, path.len())?],
        SearchFragmentCursor {
            match_index: next_search_match(value, match_offset),
            omission_index: next_omission,
            match_field: None,
            match_field_offset: None,
            omission_path_offset: (!complete).then(|| u64::try_from(end).ok()).flatten(),
        },
    )?;
    let text = format!("Returned omission path bytes {offset} through {end} of {}.", path.len());
    let more =
        next_search_match(value, match_offset).is_some() || next_omission.is_some() || !complete;
    finish(structured, text.clone(), text, more)
}

#[derive(Clone, Copy)]
struct SearchFragmentCursor {
    match_index: Option<u64>,
    omission_index: Option<u64>,
    match_field: Option<SearchMatchField>,
    match_field_offset: Option<u64>,
    omission_path_offset: Option<u64>,
}

fn search_fragment_json(
    value: &SearchObservation,
    matches: Vec<BoundedJson>,
    omissions: Vec<BoundedJson>,
    cursor: SearchFragmentCursor,
) -> Result<BoundedJson, FsToolError> {
    let SearchFragmentCursor {
        match_index: next_match,
        omission_index: next_omission,
        match_field: next_match_field,
        match_field_offset: next_match_field_offset,
        omission_path_offset: next_omission_path_offset,
    } = cursor;
    object(vec![
        ("digest", string(digest_hex(value.digest()))),
        ("match_count", u64_integer(value.match_count())),
        ("matches", array(matches)),
        (
            "next_match_field",
            next_match_field
                .map_or_else(|| Ok(BoundedJson::null()), |field| string(field.as_str().to_owned())),
        ),
        (
            "next_match_field_offset",
            next_match_field_offset.map_or_else(|| Ok(BoundedJson::null()), u64_integer),
        ),
        ("next_match_offset", next_match.map_or_else(|| Ok(BoundedJson::null()), u64_integer)),
        (
            "next_omission_offset",
            next_omission.map_or_else(|| Ok(BoundedJson::null()), u64_integer),
        ),
        (
            "next_omission_path_offset",
            next_omission_path_offset.map_or_else(|| Ok(BoundedJson::null()), u64_integer),
        ),
        ("omission_count", u64_integer(value.omission_count())),
        ("omissions", array(omissions)),
        ("scanned_bytes", u64_integer(value.scanned_bytes())),
        ("scanned_files", u64_integer(value.scanned_files())),
        (
            "truncated",
            Ok(BoundedJson::boolean(
                next_match.is_some()
                    || next_omission.is_some()
                    || next_match_field.is_some()
                    || next_omission_path_offset.is_some(),
            )),
        ),
    ])
}

fn search_field_json(
    index: u64,
    matched: &crate::SearchMatch,
    field: SearchMatchField,
    offset: u64,
    piece: &str,
    total: usize,
) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("column_bytes", u64_integer(matched.column_bytes())),
        ("field", string(field.as_str().to_owned())),
        ("index", u64_integer(index)),
        ("line", u64_integer(matched.line())),
        ("offset", u64_integer(offset)),
        ("text", string(piece.to_owned())),
        ("total_bytes", u64_integer(u64::try_from(total).map_err(|_| protocol_error())?)),
    ])
}

fn search_omission_field_json(
    index: u64,
    omission: &ScopeOmission,
    offset: u64,
    piece: &str,
    total: usize,
) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("field", string(omission.path_field().to_owned())),
        ("index", u64_integer(index)),
        ("offset", u64_integer(offset)),
        ("reason", string(omission_name(omission.reason()).to_owned())),
        ("text", string(piece.to_owned())),
        ("total_bytes", u64_integer(u64::try_from(total).map_err(|_| protocol_error())?)),
    ])
}

fn pending_omission(value: &SearchObservation, offset: u64) -> Option<u64> {
    let relative = offset.checked_sub(value.omission_page_start())?;
    if relative < u64::try_from(value.omissions().len()).ok()? {
        Some(offset)
    } else {
        value.next_omission_offset()
    }
}

fn next_search_match(value: &SearchObservation, next_index: u64) -> Option<u64> {
    let page_end = value.page_start().checked_add(u64::try_from(value.matches().len()).ok()?)?;
    if next_index < page_end { Some(next_index) } else { value.next_offset() }
}

fn checked_string_offset(value: &str, offset: u64) -> Result<usize, FsToolError> {
    let offset = usize::try_from(offset).map_err(|_| protocol_error())?;
    if offset >= value.len() || !value.is_char_boundary(offset) {
        return Err(protocol_error());
    }
    Ok(offset)
}

fn largest_prefix_end(
    value: &str,
    start: usize,
    mut fits: impl FnMut(&str, usize) -> bool,
) -> Result<usize, FsToolError> {
    let mut ends = value[start..]
        .char_indices()
        .map(|(offset, ch)| start + offset + ch.len_utf8())
        .collect::<Vec<_>>();
    if ends.last().copied() != Some(value.len()) {
        ends.push(value.len());
    }
    let mut low = 0_usize;
    let mut high = ends.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let end = ends[middle];
        if fits(&value[start..end], end) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let end = ends
        .get(low.checked_sub(1).ok_or_else(protocol_error)?)
        .copied()
        .ok_or_else(protocol_error)?;
    Ok(end)
}
