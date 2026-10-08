//! Read-only inspection of absolute paths explicitly named by the user's task.

use std::{
    collections::{BTreeSet, VecDeque},
    ffi::OsStr,
    fs::{self, File},
    io::{Read as _, Seek as _, SeekFrom},
    path::{Component, Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use peritus_types::Sha256Digest;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{
    executor::WorkspaceDeveloperTools,
    inspection::entry_kind,
    path::{protected_metadata, targets_default_exclusion, traversal_exclusion, tool},
    wire::{bounded_usize, object, required_string},
};
use crate::file_metadata;

const LIST_PAGE_RECORDS: usize = 256;
const READ_PAGE_BYTES: usize = 16 * 1024;
const LIST_CURSOR_MAGIC: &[u8; 8] = b"PRFLv001";
const READ_CURSOR_MAGIC: &[u8; 8] = b"PRFRv001";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ExplicitReferences {
    roots: Vec<PathBuf>,
}

impl ExplicitReferences {
    pub(super) fn merge(&mut self, other: &Self) {
        self.roots.extend(other.roots.iter().cloned());
        self.roots.sort();
        self.roots.dedup();
    }

    fn from_task(workspace_root: &Path, task: &str) -> Self {
        let mut roots = BTreeSet::new();
        for token in task_tokens(task) {
            let Some(path) = explicit_absolute_path(&token) else { continue };
            if path_starts_with_case_insensitive(&path, workspace_root) {
                continue;
            }
            roots.insert(path);
        }
        Self { roots: roots.into_iter().collect() }
    }

    fn resolve(&self, raw: &str) -> Result<(PathBuf, PathBuf), DeveloperLoopError> {
        let requested = explicit_absolute_path(raw)
            .ok_or_else(|| tool("reference path must be a normal absolute path"))?;
        let root = self
            .roots
            .iter()
            .filter(|root| path_starts_with_case_insensitive(&requested, root))
            .max_by_key(|root| root.components().count())
            .cloned()
            .ok_or_else(|| {
                tool("reference path was not explicitly named by the user's task; no read authority was granted")
            })?;
        let root_components = root.components().count();
        let resolved_root = case_correct_path(&root)?;
        let resolved_requested =
            case_correct_descendant(&resolved_root, &requested, root_components)?;
        Ok((resolved_root, resolved_requested))
    }

    pub(super) fn extend_from_task(&mut self, workspace_root: &Path, task: &str) {
        let added = Self::from_task(workspace_root, task);
        self.roots.extend(added.roots);
        self.roots.sort();
        self.roots.dedup();
    }
}

impl WorkspaceDeveloperTools {
    #[must_use]
    pub(crate) fn with_reference_contract(mut self, task: &str) -> Self {
        self.references = ExplicitReferences::from_task(&self.root, task);
        self
    }
}

pub(super) fn list(
    references: &ExplicitReferences,
    arguments: &Value,
) -> Result<Value, DeveloperLoopError> {
    let (root, start) = references.resolve(required_string(arguments, "path")?)?;
    if protected_metadata(&start) {
        return Err(tool("repository control metadata is not an external reference target"));
    }
    let metadata = fs::symlink_metadata(&start).map_err(|error| reference_io_error(&error))?;
    if !metadata.is_dir() {
        return Err(tool("external reference path is not a directory"));
    }
    let depth = bounded_usize(arguments, "depth", 3, 1, usize::MAX);
    let request = list_request_binding(&root, &start, depth);
    let requested_cursor = arguments.get("cursor").and_then(Value::as_str);
    let cursor = requested_cursor.map(decode_list_cursor).transpose()?;
    if cursor.as_ref().is_some_and(|cursor| cursor.request != request) {
        return Err(tool(
            "external reference listing continuation belongs to another path or depth",
        ));
    }
    let mut queue = VecDeque::from([(start, 0_usize)]);
    let targeted_root = queue.front().map(|(path, _)| path.clone());
    let mut records = Vec::new();
    while let Some((directory, level)) = queue.pop_front() {
        let children = match fs::read_dir(&directory) {
            Ok(children) => children,
            Err(error) if level == 0 => return Err(tool(error.to_string())),
            Err(error) => {
                records.push(ReferenceRecord::Omission(json!({
                    "path": directory.to_string_lossy(),
                    "reason": "directory_unavailable",
                    "detail": error.to_string(),
                    "targetable": false
                })));
                continue;
            }
        };
        let mut children = children.filter_map(Result::ok).collect::<Vec<_>>();
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            if let Some(exclusion) =
                traversal_exclusion(&path, targeted_root.as_deref())
            {
                records.push(ReferenceRecord::Omission(json!({
                    "path": path.to_string_lossy(),
                    "reason": exclusion.reason(),
                    "detail": exclusion.detail(),
                    "targetable": exclusion.targetable()
                })));
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    records.push(ReferenceRecord::Omission(json!({
                        "path": path.to_string_lossy(),
                        "reason": "entry_unavailable",
                        "detail": error.to_string(),
                        "targetable": false
                    })));
                    continue;
                }
            };
            let kind = metadata.file_type();
            records.push(ReferenceRecord::Entry(object(vec![
                ("path", Value::String(path.to_string_lossy().into_owned())),
                ("kind", Value::String(entry_kind(kind).to_owned())),
                ("bytes", Value::from(metadata.len())),
                ("permissions", Value::String(file_metadata::permissions(&metadata))),
            ])));
            if kind.is_dir() && level + 1 < depth {
                queue.push_back((path, level + 1));
            }
        }
    }
    let observation = reference_records_digest(&records)?;
    if cursor.as_ref().is_some_and(|cursor| cursor.observation != observation) {
        return Err(tool(
            "external reference listing changed since its continuation was issued",
        ));
    }
    let start_index = cursor.as_ref().map_or(0, |cursor| cursor.start);
    if start_index > records.len() || start_index == records.len() && !records.is_empty() {
        return Err(tool("external reference listing continuation is beyond the observation"));
    }
    let end_index = start_index.saturating_add(LIST_PAGE_RECORDS).min(records.len());
    let mut entries = Vec::new();
    let mut omissions = Vec::new();
    for record in &records[start_index..end_index] {
        match record {
            ReferenceRecord::Entry(value) => entries.push(value.clone()),
            ReferenceRecord::Omission(value) => omissions.push(value.clone()),
        }
    }
    let next = (end_index < records.len())
        .then(|| encode_list_cursor(request, observation, end_index));
    Ok(object(vec![
        ("reference_root", Value::String(root.to_string_lossy().into_owned())),
        ("path_kind", Value::String("explicit-absolute-reference".to_owned())),
        ("entries", Value::Array(entries)),
        ("omissions", Value::Array(omissions)),
        ("observation", Value::String(digest_hex(observation))),
        ("start", Value::String(start_index.to_string())),
        ("page_records", Value::from(end_index.saturating_sub(start_index))),
        ("total_records", Value::String(records.len().to_string())),
        ("coverage_complete", Value::Bool(records.iter().all(|record| matches!(record, ReferenceRecord::Entry(_))))),
        ("coverage_policy", reference_coverage_policy(targeted_root.as_deref())),
        ("complete", Value::Bool(next.is_none())),
        ("partial", Value::Bool(next.is_some())),
        ("truncated", Value::Bool(next.is_some())),
        ("replay", requested_cursor.map_or(Value::Null, |value| Value::String(value.to_owned()))),
        ("next", next.map_or(Value::Null, Value::String)),
    ]))
}

pub(super) fn read(
    references: &ExplicitReferences,
    arguments: &Value,
) -> Result<Value, DeveloperLoopError> {
    let (root, path) = references.resolve(required_string(arguments, "path")?)?;
    if protected_metadata(&path) {
        return Err(tool("repository control metadata is not an external reference target"));
    }
    let metadata = fs::symlink_metadata(&path).map_err(|error| reference_io_error(&error))?;
    if !metadata.is_file() {
        return Err(tool("external reference path is not a regular text file"));
    }
    let start = bounded_usize(arguments, "start_line", 1, 1, usize::MAX);
    let end = bounded_usize(arguments, "end_line", usize::MAX, start, usize::MAX);
    let request = read_request_binding(&root, &path, start, end);
    let requested_cursor = arguments.get("cursor").and_then(Value::as_str);
    let cursor = requested_cursor.map(decode_read_cursor).transpose()?;
    if cursor.as_ref().is_some_and(|cursor| cursor.request != request) {
        return Err(tool(
            "external reference read continuation belongs to another path or line selection",
        ));
    }
    let mut file = File::open(&path).map_err(|error| tool(error.to_string()))?;
    let before = file.metadata().map_err(|error| tool(error.to_string()))?;
    let snapshot = scan_reference(&mut file, start, end)?;
    if cursor.as_ref().is_some_and(|cursor| {
        cursor.source != snapshot.digest
            || cursor.source_bytes != snapshot.bytes
            || cursor.selection_start != snapshot.selection_start
            || cursor.selection_end != snapshot.selection_end
    }) {
        return Err(tool(
            "external reference source changed since its continuation was issued",
        ));
    }
    let offset = cursor.as_ref().map_or(snapshot.selection_start, |cursor| cursor.offset);
    if offset < snapshot.selection_start || offset > snapshot.selection_end {
        return Err(tool("external reference continuation is outside its selected range"));
    }
    let content = read_reference_page(&mut file, offset, snapshot.selection_end)?;
    let returned_end = offset
        .checked_add(content.len() as u64)
        .ok_or_else(|| tool("external reference page offset overflowed"))?;
    let verified = scan_reference(&mut file, start, end)?;
    let after = file.metadata().map_err(|error| tool(error.to_string()))?;
    if snapshot != verified || !same_metadata(&before, &after) {
        return Err(tool("external reference changed while it was being read"));
    }
    let next = (returned_end < snapshot.selection_end).then(|| {
        encode_read_cursor(
            request,
            snapshot.digest,
            snapshot.bytes,
            snapshot.selection_start,
            snapshot.selection_end,
            returned_end,
        )
    });
    Ok(object(vec![
        ("reference_root", Value::String(root.to_string_lossy().into_owned())),
        ("path", Value::String(path.to_string_lossy().into_owned())),
        ("content", Value::String(content)),
        ("start_line", Value::from(start)),
        ("end_line", if end == usize::MAX { Value::Null } else { Value::from(end) }),
        ("selection_start_byte", Value::String(snapshot.selection_start.to_string())),
        ("selection_end_byte", Value::String(snapshot.selection_end.to_string())),
        ("returned_start_byte", Value::String(offset.to_string())),
        ("returned_end_byte", Value::String(returned_end.to_string())),
        ("bytes", Value::from(snapshot.bytes)),
        ("source_sha256", Value::String(digest_hex(snapshot.digest))),
        ("permissions", Value::String(file_metadata::permissions(&before))),
        ("coverage_complete", Value::Bool(true)),
        ("complete", Value::Bool(next.is_none())),
        ("partial", Value::Bool(next.is_some())),
        ("truncated", Value::Bool(next.is_some())),
        ("replay", requested_cursor.map_or(Value::Null, |value| Value::String(value.to_owned()))),
        ("next", next.map_or(Value::Null, Value::String)),
    ]))
}

#[derive(Clone)]
enum ReferenceRecord {
    Entry(Value),
    Omission(Value),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReferenceSnapshot {
    digest: Sha256Digest,
    bytes: u64,
    selection_start: u64,
    selection_end: u64,
}

struct ListCursor {
    request: Sha256Digest,
    observation: Sha256Digest,
    start: usize,
}

struct ReadCursor {
    request: Sha256Digest,
    source: Sha256Digest,
    source_bytes: u64,
    selection_start: u64,
    selection_end: u64,
    offset: u64,
}

fn scan_reference(
    file: &mut File,
    requested_start: usize,
    requested_end: usize,
) -> Result<ReferenceSnapshot, DeveloperLoopError> {
    file.seek(SeekFrom::Start(0)).map_err(|error| tool(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    let mut line = 1_usize;
    let mut selection_start = (requested_start == 1).then_some(0_u64);
    let mut selection_end = None;
    loop {
        let read = file.read(&mut buffer).map_err(|error| tool(error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        for (index, byte) in buffer[..read].iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            let next = bytes
                .checked_add(index as u64 + 1)
                .ok_or_else(|| tool("external reference byte offset overflowed"))?;
            if line == requested_end {
                selection_end = Some(next);
            }
            line = line.saturating_add(1);
            if line == requested_start && selection_start.is_none() {
                selection_start = Some(next);
            }
        }
        bytes = bytes
            .checked_add(read as u64)
            .ok_or_else(|| tool("external reference byte count overflowed"))?;
    }
    let selection_start = selection_start.unwrap_or(bytes);
    let selection_end = selection_end.unwrap_or(bytes).max(selection_start);
    Ok(ReferenceSnapshot {
        digest: Sha256Digest::new(hasher.finalize().into()),
        bytes,
        selection_start,
        selection_end,
    })
}

fn read_reference_page(
    file: &mut File,
    offset: u64,
    end: u64,
) -> Result<String, DeveloperLoopError> {
    let maximum = usize::try_from(end.saturating_sub(offset).min(READ_PAGE_BYTES as u64))
        .map_err(|_| tool("external reference page is not representable"))?;
    if maximum == 0 {
        return Ok(String::new());
    }
    file.seek(SeekFrom::Start(offset)).map_err(|error| tool(error.to_string()))?;
    let mut bytes = vec![0_u8; maximum];
    file.read_exact(&mut bytes).map_err(|error| tool(error.to_string()))?;
    if let Err(error) = std::str::from_utf8(&bytes) {
        if error.error_len().is_some() || error.valid_up_to() == 0 {
            return Err(tool("selected external reference range is not UTF-8"));
        }
        bytes.truncate(error.valid_up_to());
    }
    String::from_utf8(bytes).map_err(|_| tool("selected external reference range is not UTF-8"))
}

fn same_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
        && left.permissions() == right.permissions()
}

fn reference_records_digest(records: &[ReferenceRecord]) -> Result<Sha256Digest, DeveloperLoopError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/reference-list-observation/v1\0");
    for record in records {
        let (kind, value) = match record {
            ReferenceRecord::Entry(value) => (b'e', value),
            ReferenceRecord::Omission(value) => (b'o', value),
        };
        let encoded = serde_json::to_vec(value).map_err(|error| tool(error.to_string()))?;
        hasher.update([kind]);
        hasher.update((encoded.len() as u64).to_be_bytes());
        hasher.update(encoded);
    }
    Ok(Sha256Digest::new(hasher.finalize().into()))
}

fn list_request_binding(root: &Path, start: &Path, depth: usize) -> Sha256Digest {
    request_digest(
        b"peritus/reference-list-request/v1\0",
        &[root.as_os_str().as_encoded_bytes(), start.as_os_str().as_encoded_bytes(), &depth.to_be_bytes()],
    )
}

fn read_request_binding(root: &Path, path: &Path, start: usize, end: usize) -> Sha256Digest {
    request_digest(
        b"peritus/reference-read-request/v1\0",
        &[
            root.as_os_str().as_encoded_bytes(),
            path.as_os_str().as_encoded_bytes(),
            &start.to_be_bytes(),
            &end.to_be_bytes(),
        ],
    )
}

fn request_digest(domain: &[u8], values: &[&[u8]]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for value in values {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    Sha256Digest::new(hasher.finalize().into())
}

fn encode_list_cursor(request: Sha256Digest, observation: Sha256Digest, start: usize) -> String {
    let mut bytes = LIST_CURSOR_MAGIC.to_vec();
    bytes.extend_from_slice(request.as_bytes());
    bytes.extend_from_slice(observation.as_bytes());
    bytes.extend_from_slice(&(start as u64).to_be_bytes());
    encode_cursor(bytes)
}

fn decode_list_cursor(value: &str) -> Result<ListCursor, DeveloperLoopError> {
    let mut bytes = decode_cursor(value, LIST_CURSOR_MAGIC)?;
    let request = take_digest(&mut bytes)?;
    let observation = take_digest(&mut bytes)?;
    let start = usize::try_from(take_u64(&mut bytes)?)
        .map_err(|_| tool("external reference listing cursor exceeds this platform"))?;
    if !bytes.is_empty() {
        return Err(tool("external reference listing cursor has trailing data"));
    }
    Ok(ListCursor { request, observation, start })
}

fn encode_read_cursor(
    request: Sha256Digest,
    source: Sha256Digest,
    source_bytes: u64,
    selection_start: u64,
    selection_end: u64,
    offset: u64,
) -> String {
    let mut bytes = READ_CURSOR_MAGIC.to_vec();
    bytes.extend_from_slice(request.as_bytes());
    bytes.extend_from_slice(source.as_bytes());
    for value in [source_bytes, selection_start, selection_end, offset] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    encode_cursor(bytes)
}

fn decode_read_cursor(value: &str) -> Result<ReadCursor, DeveloperLoopError> {
    let mut bytes = decode_cursor(value, READ_CURSOR_MAGIC)?;
    let request = take_digest(&mut bytes)?;
    let source = take_digest(&mut bytes)?;
    let source_bytes = take_u64(&mut bytes)?;
    let selection_start = take_u64(&mut bytes)?;
    let selection_end = take_u64(&mut bytes)?;
    let offset = take_u64(&mut bytes)?;
    if !bytes.is_empty() {
        return Err(tool("external reference read cursor has trailing data"));
    }
    Ok(ReadCursor { request, source, source_bytes, selection_start, selection_end, offset })
}

fn encode_cursor(mut bytes: Vec<u8>) -> String {
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    encode_hex(&bytes)
}

fn decode_cursor(value: &str, magic: &[u8; 8]) -> Result<Vec<u8>, DeveloperLoopError> {
    let bytes = decode_hex(value)?;
    if bytes.len() < magic.len() + 32 || &bytes[..magic.len()] != magic {
        return Err(tool("external reference cursor has an invalid format"));
    }
    let split = bytes.len() - 32;
    if peritus_codec::sha256(&bytes[..split]).as_bytes() != &bytes[split..] {
        return Err(tool("external reference cursor checksum is invalid"));
    }
    Ok(bytes[magic.len()..split].to_vec())
}

fn take_digest(bytes: &mut Vec<u8>) -> Result<Sha256Digest, DeveloperLoopError> {
    if bytes.len() < 32 {
        return Err(tool("external reference cursor is incomplete"));
    }
    let tail = bytes.split_off(32);
    let digest = Sha256Digest::new(bytes[..32].try_into().map_err(|_| tool("invalid digest"))?);
    *bytes = tail;
    Ok(digest)
}

fn take_u64(bytes: &mut Vec<u8>) -> Result<u64, DeveloperLoopError> {
    if bytes.len() < 8 {
        return Err(tool("external reference cursor is incomplete"));
    }
    let tail = bytes.split_off(8);
    let value = u64::from_be_bytes(bytes[..8].try_into().map_err(|_| tool("invalid cursor number"))?);
    *bytes = tail;
    Ok(value)
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>, DeveloperLoopError> {
    if !value.len().is_multiple_of(2) {
        return Err(tool("external reference cursor is not hexadecimal"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0]).ok_or_else(|| tool("external reference cursor is not hexadecimal"))?;
            let low = hex_digit(pair[1]).ok_or_else(|| tool("external reference cursor is not hexadecimal"))?;
            Ok((high << 4) | low)
        })
        .collect()
}

const fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn digest_hex(digest: Sha256Digest) -> String {
    encode_hex(digest.as_bytes())
}

fn reference_coverage_policy(targeted_root: Option<&Path>) -> Value {
    object(vec![
        ("protected_metadata", Value::Array(vec![Value::String(".git".to_owned())])),
        (
            "default_exclusions",
            Value::Array(
                ["target", "node_modules", ".venv", "__pycache__"]
                    .into_iter()
                    .map(|name| Value::String(name.to_owned()))
                    .collect(),
            ),
        ),
        ("default_exclusions_targetable", Value::Bool(true)),
        ("targeted_default_exclusion", Value::Bool(targets_default_exclusion(targeted_root))),
    ])
}

fn reference_io_error(error: &std::io::Error) -> DeveloperLoopError {
    if error.kind() == std::io::ErrorKind::NotFound {
        reference_not_found()
    } else {
        tool(error.to_string())
    }
}

fn reference_not_found() -> DeveloperLoopError {
    tool(
        "not_found: no exact or uniquely case-insensitive match exists for the explicit reference path",
    )
}

fn path_starts_with_case_insensitive(path: &Path, root: &Path) -> bool {
    let mut components = path.components();
    root.components().all(|root_component| {
        components.next().is_some_and(|component| components_match(component, root_component))
    })
}

fn components_match(left: Component<'_>, right: Component<'_>) -> bool {
    match (left, right) {
        (Component::RootDir, Component::RootDir)
        | (Component::CurDir, Component::CurDir)
        | (Component::ParentDir, Component::ParentDir) => true,
        (Component::Prefix(left), Component::Prefix(right)) => {
            names_match(left.as_os_str(), right.as_os_str())
        }
        (Component::Normal(left), Component::Normal(right)) => names_match(left, right),
        _ => false,
    }
}

fn names_match(left: &OsStr, right: &OsStr) -> bool {
    left == right
        || left
            .to_str()
            .zip(right.to_str())
            .is_some_and(|(left, right)| left.to_lowercase() == right.to_lowercase())
}

fn case_correct_path(path: &Path) -> Result<PathBuf, DeveloperLoopError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(part) => current = case_correct_component(&current, part)?,
            Component::CurDir | Component::ParentDir => {
                return Err(tool("reference path contains an invalid component"));
            }
        }
    }
    // Ancestors of the user-named root may be system aliases such as macOS /var. The named
    // target itself and every descendant must remain ordinary entries rather than redirects.
    reject_symlink(&current)?;
    Ok(current)
}

fn case_correct_descendant(
    root: &Path,
    requested: &Path,
    root_components: usize,
) -> Result<PathBuf, DeveloperLoopError> {
    let mut current = root.to_path_buf();
    for component in requested.components().skip(root_components) {
        let Component::Normal(part) = component else {
            return Err(tool("reference path contains an invalid descendant component"));
        };
        current = case_correct_component(&current, part)?;
        reject_symlink(&current)?;
    }
    Ok(current)
}

fn case_correct_component(parent: &Path, requested: &OsStr) -> Result<PathBuf, DeveloperLoopError> {
    // A successful lookup does not prove the spelling is exact on a case-insensitive filesystem.
    // Enumerate actual names, and defer ambiguity until an exact entry has had a chance to win.
    let mut matched = None;
    let mut ambiguous = false;
    for entry in fs::read_dir(parent).map_err(|error| reference_io_error(&error))? {
        let entry = entry.map_err(|error| tool(error.to_string()))?;
        let actual = entry.file_name();
        if !names_match(&actual, requested) {
            continue;
        }
        if actual == requested {
            return Ok(entry.path());
        }
        ambiguous |= matched.is_some();
        matched = Some(entry.path());
    }
    if ambiguous {
        return Err(tool(
            "ambiguous: multiple filesystem entries match a reference path component when case is ignored; use exact casing",
        ));
    }
    if let Some(path) = matched {
        return Ok(path);
    }
    // Windows short-name aliases may resolve without appearing among the long entry names.
    let alias = parent.join(requested);
    fs::symlink_metadata(&alias).map_err(|error| reference_io_error(&error))?;
    Ok(alias)
}

fn reject_symlink(path: &Path) -> Result<(), DeveloperLoopError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| reference_io_error(&error))?;
    if metadata.file_type().is_symlink() {
        Err(tool("symbolic links are not explicit reference targets"))
    } else {
        Ok(())
    }
}

fn explicit_absolute_path(raw: &str) -> Option<PathBuf> {
    let token = raw.trim_matches(|character: char| {
        matches!(
            character,
            '`' | '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';'
        )
    });
    let token = token.strip_suffix('.').unwrap_or(token);
    if token.is_empty()
        || token.len() > 4096
        || token.contains("://")
        || token.contains(char::is_control)
    {
        return None;
    }
    let path = PathBuf::from(token);
    let mut normal = 0_usize;
    for component in path.components() {
        match component {
            Component::Normal(_) => normal = normal.saturating_add(1),
            Component::RootDir | Component::Prefix(_) => {}
            Component::CurDir | Component::ParentDir => return None,
        }
    }
    (path.is_absolute() && normal > 0).then_some(path)
}

fn task_tokens(task: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in task.chars() {
        if let Some(expected) = quote {
            if character == expected {
                quote = None;
            } else {
                current.push(character);
            }
        } else if current.is_empty() && matches!(character, '`' | '\'' | '"') {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_paths_are_exact_bounded_and_outside_the_workspace() {
        let workspace = Path::new("/work/output");
        let references = ExplicitReferences::from_task(
            workspace,
            "match '/reference files/invoices' and /WORK/OUTPUT/src while ignoring https://host/a",
        );
        assert!(references.roots.iter().all(|root| !root.starts_with(workspace)));
        #[cfg(unix)]
        assert_eq!(references.roots, vec![PathBuf::from("/reference files/invoices")]);
    }
}
