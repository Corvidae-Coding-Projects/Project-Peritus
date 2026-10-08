//! Bounded read-only workspace inspection operations.

use std::{
    fmt::Write as _,
    fs,
    io::{BufRead as _, BufReader},
    path::Path,
};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::{
    access_policy::WorkspaceAccessPolicy,
    path::{checked, ignored, tool},
    resources::CommandResources,
    wire::{object, required_string, string},
};
use peritus_workspace::{
    DirectoryItem, NativeNameEncoding, ObservedDirectory, WorkspaceEntryKind,
};

mod listing;
mod retained;
mod search;
pub(crate) use listing::{DirectoryListingOwner, ListingError, RetainedListingPage};
pub(crate) use retained::{InspectionError, WorkspaceInspectionOwner};

pub(super) const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn list(
    root: &Path,
    arguments: &Value,
    resources: CommandResources,
    access_policy: &WorkspaceAccessPolicy,
    owner: &DirectoryListingOwner,
) -> Result<Value, DeveloperLoopError> {
    let relative = string(arguments, "path").unwrap_or("");
    let _ = checked(root, relative, false)?;
    let selected = if relative.is_empty() || relative == "." {
        None
    } else {
        Some(
            peritus_patch::WorkspacePath::new(relative)
                .map_err(|error| tool(error.to_string()))?,
        )
    };
    let page = owner
        .page(selected.as_ref(), string(arguments, "cursor"))
        .map_err(|error| match error {
            ListingError::Cancelled => DeveloperLoopError::Cancelled,
            error => tool(error.to_string()),
        })?;
    listing(root, page, resources, access_policy, arguments.get("depth"))
}

fn listing(
    root: &Path,
    page: RetainedListingPage,
    resources: CommandResources,
    access_policy: &WorkspaceAccessPolicy,
    requested_depth: Option<&Value>,
) -> Result<Value, DeveloperLoopError> {
    let mut entries = Vec::new();
    let mut omissions = Vec::new();
    let observed_items = page.items().len();
    for item in page.items() {
        let relative = item_relative(page.observation(), item)?;
        if ignored(&relative) {
            omissions.push(object(vec![
                ("path", Value::String(relative.to_string_lossy().into_owned())),
                ("reason", Value::String("ignored_path".to_owned())),
                (
                    "detail",
                    Value::String(
                        "entry is excluded from workspace inspection by repository policy"
                            .to_owned(),
                    ),
                ),
            ]));
            continue;
        }
        if !access_policy.permits_search_result(&relative) {
            omissions.push(object(vec![
                ("path", Value::Null),
                ("reason", Value::String("access_restricted".to_owned())),
                (
                    "detail",
                    Value::String(
                        "one entry is protected or opaque under the active request policy"
                            .to_owned(),
                    ),
                ),
            ]));
            continue;
        }
        entries.push(item_value(item, &relative)?);
    }
    let returned_items = entries.len();
    let omitted_items = omissions.len();
    let mut fields = vec![
        ("workspace_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("workspace-relative".to_owned())),
        ("execution_resources", resources.observation()),
        ("entries", Value::Array(entries)),
        ("omissions", Value::Array(omissions)),
        ("observation", Value::String(page.observation_handle())),
        ("start", Value::String(page.start().to_string())),
        (
            "total_observed_entries",
            Value::String(page.observation().count().to_string()),
        ),
        ("page_observed_entries", Value::from(observed_items)),
        ("page_returned_entries", Value::from(returned_items)),
        ("page_omitted_entries", Value::from(omitted_items)),
        (
            "exact_empty",
            Value::Bool(page.observation().count() == 0),
        ),
        ("coverage_complete", Value::Bool(omitted_items == 0)),
        ("complete", Value::Bool(page.complete())),
        ("partial", Value::Bool(!page.complete())),
        ("truncated", Value::Bool(!page.complete())),
        (
            "replay",
            page.replay().map_or(Value::Null, |cursor| Value::String(cursor.to_owned())),
        ),
        (
            "next",
            page.next().map_or(Value::Null, |cursor| Value::String(cursor.to_owned())),
        ),
    ];
    if let Some(depth) = requested_depth {
        fields.push(("requested_depth", depth.clone()));
        fields.push(("navigation_depth", Value::from(1)));
    }
    Ok(object(fields))
}

fn item_relative(
    observation: &ObservedDirectory,
    item: &DirectoryItem,
) -> Result<std::path::PathBuf, DeveloperLoopError> {
    let mut path = observation
        .path()
        .map_or_else(std::path::PathBuf::new, |path| path.as_path().to_path_buf());
    path.push(item.name().native_name().map_err(|error| tool(error.to_string()))?);
    Ok(path)
}

fn item_value(item: &DirectoryItem, relative: &Path) -> Result<Value, DeveloperLoopError> {
    if let Some(metadata) = item.metadata() {
        return Ok(object(vec![
            ("path", Value::String(metadata.path().as_str().to_owned())),
            (
                "kind",
                Value::String(
                    match metadata.kind() {
                        WorkspaceEntryKind::File => "file",
                        WorkspaceEntryKind::Directory => "directory",
                    }
                    .to_owned(),
                ),
            ),
            ("bytes", Value::from(metadata.size())),
            ("executable", Value::Bool(metadata.executable())),
        ]));
    }
    let exclusion = item
        .exclusion()
        .ok_or_else(|| tool("retained directory item has neither metadata nor exclusion"))?;
    Ok(object(vec![
        ("path", Value::String(relative.to_string_lossy().into_owned())),
        ("kind", Value::String("excluded".to_owned())),
        ("name", Value::String(item.name().display_name())),
        (
            "native_encoding",
            Value::String(
                match item.name().encoding() {
                    NativeNameEncoding::UnixBytes => "unix_bytes",
                    NativeNameEncoding::WindowsWide => "windows_utf16_be",
                    NativeNameEncoding::Utf8 => "utf8",
                }
                .to_owned(),
            ),
        ),
        ("native_units", Value::String(hex(item.name().encoded_bytes()))),
        (
            "exclusion",
            Value::String(exclusion.as_str().to_owned()),
        ),
    ]))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

pub(super) fn search(
    root: &Path,
    arguments: &Value,
    access_policy: &WorkspaceAccessPolicy,
    owner: &WorkspaceInspectionOwner,
) -> Result<Value, DeveloperLoopError> {
    let query = required_string(arguments, "query")?;
    let selected = match string(arguments, "path") {
        Some(value) if !value.is_empty() && value != "." => {
            let _ = checked(root, value, false)?;
            Some(
                peritus_patch::WorkspacePath::new(value)
                    .map_err(|error| tool(error.to_string()))?,
            )
        }
        _ => None,
    };
    let page = owner
        .search_page(
            selected.as_ref(),
            query,
            arguments.get("max_results").and_then(Value::as_u64),
            string(arguments, "cursor"),
            access_policy,
        )
        .map_err(|error| match error {
            InspectionError::Cancelled => DeveloperLoopError::Cancelled,
            error => tool(error.to_string()),
        })?;
    let mut matches = Vec::new();
    let mut omissions = Vec::new();
    for record in &page.records {
        match record.get("kind").and_then(Value::as_str) {
            Some("match") => matches.push(record.clone()),
            Some("omission") => omissions.push(record.clone()),
            _ => return Err(tool("retained workspace search result has no typed kind")),
        }
    }
    let complete = page.next.is_none();
    Ok(object(vec![
        ("workspace_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("workspace-relative".to_owned())),
        ("matches", Value::Array(matches)),
        ("omissions", Value::Array(omissions)),
        ("observation", Value::String(page.observation_handle)),
        (
            "workspace_identity",
            Value::String(retained::digest_hex(page.observation.folder)),
        ),
        (
            "membership_sha256",
            Value::String(retained::digest_hex(page.observation.membership)),
        ),
        (
            "sources_sha256",
            Value::String(retained::digest_hex(page.observation.sources)),
        ),
        ("start", Value::String(page.start.to_string())),
        (
            "page_returned_records",
            Value::from(page.records.len()),
        ),
        (
            "total_records",
            Value::String(page.observation.records.to_string()),
        ),
        (
            "total_match_fragments",
            Value::String(page.observation.matches.to_string()),
        ),
        (
            "total_omissions",
            Value::String(page.observation.omissions.to_string()),
        ),
        (
            "coverage_complete",
            Value::Bool(page.observation.omissions == 0),
        ),
        ("complete", Value::Bool(complete)),
        ("partial", Value::Bool(!complete)),
        ("truncated", Value::Bool(!complete)),
        (
            "replay",
            page.replay.map_or(Value::Null, Value::String),
        ),
        ("next", page.next.map_or(Value::Null, Value::String)),
    ]))
}

pub(super) fn read(
    root: &Path,
    arguments: &Value,
    owner: &WorkspaceInspectionOwner,
) -> Result<Value, DeveloperLoopError> {
    let relative = required_string(arguments, "path")?;
    let path = checked(root, relative, true)?;
    fs::symlink_metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            tool("not_found: this file does not exist. Use the observed workspace listing; do not repeat this read unless the file has since been created. For an authorized greenfield task, create the planned file with workspace_write rather than assuming a manifest already exists.")
        } else { tool(error.to_string()) }
    })?;
    let selected =
        peritus_patch::WorkspacePath::new(relative).map_err(|error| tool(error.to_string()))?;
    let first_line = arguments.get("start_line").and_then(Value::as_u64).unwrap_or(1);
    let last_line = arguments.get("end_line").and_then(Value::as_u64);
    let page = owner
        .read_page(&selected, first_line, last_line, string(arguments, "cursor"))
        .map_err(|error| match error {
            InspectionError::Cancelled => DeveloperLoopError::Cancelled,
            error => tool(error.to_string()),
        })?;
    let complete = page.next.is_none();
    let start_line = page
        .start_position
        .map_or(Value::Null, |position| Value::from(position.line));
    let end_line = page.last_rendered_line.map_or(Value::Null, Value::from);
    let start_column = page
        .start_position
        .map_or(Value::Null, |position| Value::from(position.column_bytes));
    let end_column = page
        .end_position
        .map_or(Value::Null, |position| Value::from(position.column_bytes));
    let start_cursor_line = page
        .start_position
        .map_or(Value::Null, |position| Value::from(position.line));
    let end_cursor_line = page
        .end_position
        .map_or(Value::Null, |position| Value::from(position.line));
    Ok(object(vec![
        ("path", Value::String(relative.to_owned())),
        ("content", Value::String(page.content)),
        ("start_line", start_line),
        ("end_line", end_line),
        ("requested_start_line", Value::from(first_line)),
        (
            "requested_end_line",
            last_line.map_or(Value::Null, Value::from),
        ),
        (
            "selection_start_byte",
            Value::String(page.selection.start.to_string()),
        ),
        (
            "selection_end_byte",
            Value::String(page.selection.end.to_string()),
        ),
        (
            "returned_start_byte",
            Value::String(page.range.0.to_string()),
        ),
        (
            "returned_end_byte",
            Value::String(page.range.1.to_string()),
        ),
        ("returned_start_cursor_line", start_cursor_line),
        ("returned_end_cursor_line", end_cursor_line),
        ("returned_start_column_bytes", start_column),
        ("returned_end_column_bytes", end_column),
        (
            "starts_mid_line",
            Value::Bool(page.start_position.is_some_and(|position| position.column_bytes != 0)),
        ),
        (
            "ends_mid_line",
            Value::Bool(
                !complete
                    && page.end_position.is_some_and(|position| position.column_bytes != 0),
            ),
        ),
        ("bytes", Value::from(page.observation.source_bytes())),
        (
            "source_sha256",
            Value::String(retained::digest_hex(page.observation.source_digest())),
        ),
        (
            "workspace_identity",
            Value::String(retained::digest_hex(page.observation.folder_digest())),
        ),
        ("permissions", Value::String(page.permissions)),
        ("observation", Value::String(page.observation_handle)),
        ("coverage_complete", Value::Bool(true)),
        ("complete", Value::Bool(complete)),
        ("partial", Value::Bool(!complete)),
        ("truncated", Value::Bool(!complete)),
        ("replay", page.replay.map_or(Value::Null, Value::String)),
        ("next", page.next.map_or(Value::Null, Value::String)),
    ]))
}

pub(super) fn entry_kind(kind: fs::FileType) -> &'static str {
    if kind.is_dir() {
        "directory"
    } else if kind.is_file() {
        "file"
    } else if kind.is_symlink() {
        "symlink"
    } else {
        "other"
    }
}

pub(super) fn read_line_range(
    path: &Path,
    start: usize,
    end: usize,
) -> Result<String, DeveloperLoopError> {
    let file = fs::File::open(path).map_err(|error| tool(error.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut current = 1_usize;
    let mut selected = Vec::new();
    let mut output = String::new();
    loop {
        let available = reader.fill_buf().map_err(|error| tool(error.to_string()))?;
        if available.is_empty() || current > end {
            break;
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let segment = &available[..consumed];
        if current >= start {
            if selected.len().saturating_add(segment.len()) > MAX_FILE_BYTES {
                return Err(tool("selected line range exceeds the inline byte bound"));
            }
            selected.extend_from_slice(segment);
        }
        let complete = segment.last() == Some(&b'\n');
        reader.consume(consumed);
        if complete {
            if current >= start {
                append_line(&mut output, current, &selected)?;
                selected.clear();
            }
            current = current.saturating_add(1);
        }
    }
    if current >= start && current <= end && !selected.is_empty() {
        append_line(&mut output, current, &selected)?;
    }
    Ok(output)
}

fn append_line(output: &mut String, number: usize, bytes: &[u8]) -> Result<(), DeveloperLoopError> {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    let line = std::str::from_utf8(bytes).map_err(|_| tool("selected line range is not UTF-8"))?;
    if !output.is_empty() {
        output.push('\n');
    }
    write!(output, "{number}: {line}").map_err(|error| tool(error.to_string()))
}
