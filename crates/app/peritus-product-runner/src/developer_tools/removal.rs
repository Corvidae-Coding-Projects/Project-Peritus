//! Exact single-target removal and durable user-authorized recursive removal transactions.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read as _,
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use super::{
    access_policy::WorkspaceAccessPolicy,
    effect::atomic_write,
    grounding::GroundingEvidence,
    path::{checked, protected_metadata, tool},
};
use crate::{ConversationView, WorkspaceMutationKind, file_metadata};

const PLAN_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedRemoval {
    state_root: PathBuf,
    plan: RemovalPlan,
    plan_sha256: String,
    checkpoint_required: bool,
}

impl PreparedRemoval {
    pub(super) fn paths(&self) -> impl Iterator<Item = &str> {
        self.plan.entries.iter().map(|entry| entry.path.as_str())
    }

    pub(super) fn checkpoints(&self) -> Vec<(String, WorkspaceMutationKind)> {
        checkpoint_entries(&self.plan)
    }

    pub(super) const fn checkpoint_required(&self) -> bool {
        self.checkpoint_required
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RemovalPlan {
    version: u16,
    transaction: String,
    path: String,
    authority_binding: String,
    authority_quote_sha256: String,
    entries: Vec<RemovalEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RemovalEntry {
    path: String,
    kind: RemovalEntryKind,
    identity: String,
    permissions: u32,
    bytes: u64,
    sha256: Option<String>,
    empty_preimage: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RemovalEntryKind {
    File,
    Directory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RemovalProgress {
    version: u16,
    plan_sha256: String,
    next: usize,
}

struct RecursiveAuthority {
    binding: String,
    quote_sha256: String,
}

pub(super) fn recursive(arguments: &Value) -> Result<bool, DeveloperLoopError> {
    match arguments.get("mode").and_then(Value::as_str) {
        None | Some("single") => Ok(false),
        Some("recursive") => Ok(true),
        Some(_) => Err(tool("workspace_remove mode is invalid")),
    }
}

pub(super) fn explicit_authority(arguments: &Value) -> bool {
    arguments.get("authority").is_some()
}

pub(super) fn transactional(arguments: &Value) -> Result<bool, DeveloperLoopError> {
    recursive(arguments).map(|recursive| recursive || explicit_authority(arguments))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_recursive(
    root: &Path,
    state_root: &Path,
    transaction: &str,
    grounding: &GroundingEvidence,
    access_policy: &WorkspaceAccessPolicy,
    view: &dyn ConversationView,
    arguments: &Value,
) -> Result<PreparedRemoval, DeveloperLoopError> {
    let relative = removal_path(arguments)?;
    grounding.ensure_recursive_removal_allowed(relative).map_err(tool)?;
    let authority = removal_authority(root, relative, true, view, arguments)?;
    let path = plan_path(state_root, transaction);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            return reopen_prepared(
                root,
                state_root,
                transaction,
                relative,
                &authority,
                access_policy,
                &path,
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(tool(format!("inspect recursive removal plan: {error}"))),
    }
    let target = checked(root, relative, false)?;
    let metadata = fs::symlink_metadata(&target).map_err(|error| tool(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(tool("recursive workspace_remove requires one ordinary directory"));
    }
    let entries = enumerate_tree(root, relative)?;
    access_policy
        .authorize_removal_tree(entries.iter().map(|entry| entry.path.as_str()))
        .map_err(tool)?;
    let plan = RemovalPlan {
        version: PLAN_VERSION,
        transaction: transaction.to_owned(),
        path: relative.to_owned(),
        authority_binding: authority.binding,
        authority_quote_sha256: authority.quote_sha256,
        entries,
    };
    let plan_sha256 = plan_digest(&plan)?;
    Ok(PreparedRemoval {
        state_root: state_root.to_owned(),
        plan,
        plan_sha256,
        checkpoint_required: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_authorized_file(
    root: &Path,
    state_root: &Path,
    transaction: &str,
    grounding: &GroundingEvidence,
    access_policy: &WorkspaceAccessPolicy,
    view: &dyn ConversationView,
    arguments: &Value,
) -> Result<PreparedRemoval, DeveloperLoopError> {
    let relative = removal_path(arguments)?;
    grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
    let authority = removal_authority(root, relative, false, view, arguments)?;
    let path = plan_path(state_root, transaction);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            return reopen_prepared(
                root,
                state_root,
                transaction,
                relative,
                &authority,
                access_policy,
                &path,
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(tool(format!("inspect authorized removal plan: {error}"))),
    }
    let target = checked(root, relative, false)?;
    let metadata = fs::symlink_metadata(&target).map_err(|error| tool(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(tool("authorized single workspace_remove requires one ordinary file"));
    }
    let entries = enumerate_tree(root, relative)?;
    if entries.len() != 1
        || entries[0].kind != RemovalEntryKind::File
        || entries[0].path != relative
    {
        return Err(tool("authorized single removal did not resolve to one exact file"));
    }
    access_policy
        .authorize_removal_tree(entries.iter().map(|entry| entry.path.as_str()))
        .map_err(tool)?;
    let plan = RemovalPlan {
        version: PLAN_VERSION,
        transaction: transaction.to_owned(),
        path: relative.to_owned(),
        authority_binding: authority.binding,
        authority_quote_sha256: authority.quote_sha256,
        entries,
    };
    let plan_sha256 = plan_digest(&plan)?;
    Ok(PreparedRemoval {
        state_root: state_root.to_owned(),
        plan,
        plan_sha256,
        checkpoint_required: true,
    })
}

#[allow(clippy::too_many_arguments)]
fn reopen_prepared(
    root: &Path,
    state_root: &Path,
    transaction: &str,
    relative: &str,
    authority: &RecursiveAuthority,
    access_policy: &WorkspaceAccessPolicy,
    path: &Path,
) -> Result<PreparedRemoval, DeveloperLoopError> {
    let plan = read_plan(path)?;
    let plan_sha256 = plan_digest(&plan)?;
    validate_plan_identity(&plan, transaction, relative, authority)?;
    access_policy
        .authorize_removal_tree(plan.entries.iter().map(|entry| entry.path.as_str()))
        .map_err(tool)?;
    let progress = read_progress(state_root, &plan, &plan_sha256)?;
    validate_scope(root, &plan, progress.next, true)?;
    Ok(PreparedRemoval {
        state_root: state_root.to_owned(),
        plan,
        plan_sha256,
        checkpoint_required: false,
    })
}

pub(super) fn remove_recursive(
    root: &Path,
    prepared: &PreparedRemoval,
) -> Result<Value, DeveloperLoopError> {
    fs::create_dir_all(&prepared.state_root).map_err(|error| tool(error.to_string()))?;
    let path = plan_path(&prepared.state_root, &prepared.plan.transaction);
    if path.exists() {
        let retained = read_plan(&path)?;
        if retained != prepared.plan || plan_digest(&retained)? != prepared.plan_sha256 {
            return Err(tool("recursive removal transaction plan changed before execution"));
        }
    } else {
        write_json(&path, &prepared.plan)?;
    }
    let mut progress = read_progress(
        &prepared.state_root,
        &prepared.plan,
        &prepared.plan_sha256,
    )?;
    let adjusted = validate_scope(root, &prepared.plan, progress.next, true)?;
    if adjusted != progress.next {
        progress.next = adjusted;
        write_progress(&prepared.state_root, &prepared.plan, &progress)?;
    }
    validate_scope(root, &prepared.plan, progress.next, false)?;
    while progress.next < prepared.plan.entries.len() {
        let entry = &prepared.plan.entries[progress.next];
        validate_entry(root, entry)?;
        let target = checked(root, &entry.path, false)?;
        match entry.kind {
            RemovalEntryKind::File => {
                fs::remove_file(&target).map_err(|error| tool(error.to_string()))?;
            }
            RemovalEntryKind::Directory => {
                if fs::read_dir(&target)
                    .map_err(|error| tool(error.to_string()))?
                    .next()
                    .transpose()
                    .map_err(|error| tool(error.to_string()))?
                    .is_some()
                {
                    return Err(tool(format!(
                        "recursive removal directory gained an unplanned child: {}",
                        entry.path,
                    )));
                }
                fs::remove_dir(&target).map_err(|error| tool(error.to_string()))?;
            }
        }
        progress.next = progress
            .next
            .checked_add(1)
            .ok_or_else(|| tool("recursive removal progress overflowed"))?;
        write_progress(&prepared.state_root, &prepared.plan, &progress)?;
    }
    match fs::symlink_metadata(checked(root, &prepared.plan.path, true)?) {
        Ok(_) => {
            return Err(tool(
                "recursive removal root still exists after transaction completion",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(tool(error.to_string())),
    }
    let files = prepared
        .plan
        .entries
        .iter()
        .filter(|entry| entry.kind == RemovalEntryKind::File)
        .count();
    let directories = prepared.plan.entries.len().saturating_sub(files);
    let kind = if prepared.plan.entries.len() == 1
        && prepared.plan.entries[0].kind == RemovalEntryKind::File
        && prepared.plan.entries[0].path == prepared.plan.path
    {
        "file"
    } else {
        "directory_tree"
    };
    Ok(object_result(vec![
        ("path", Value::String(prepared.plan.path.clone())),
        ("kind", Value::String(kind.to_owned())),
        ("transaction", Value::String(prepared.plan.transaction.clone())),
        ("scope_sha256", Value::String(prepared.plan_sha256.clone())),
        ("authority_binding", Value::String(prepared.plan.authority_binding.clone())),
        ("entries", Value::String(prepared.plan.entries.len().to_string())),
        ("files", Value::String(files.to_string())),
        ("directories", Value::String(directories.to_string())),
        ("progress", Value::String(progress.next.to_string())),
    ]))
}

pub(super) fn recovery_checkpoints(
    state_root: &Path,
    result: &Value,
) -> Result<Vec<(String, WorkspaceMutationKind)>, DeveloperLoopError> {
    let transaction = result
        .get("transaction")
        .and_then(Value::as_str)
        .ok_or_else(|| tool("recursive removal receipt has no transaction identity"))?;
    let expected = result
        .get("scope_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| tool("recursive removal receipt has no scope digest"))?;
    let plan = read_plan(&plan_path(state_root, transaction))?;
    let digest = plan_digest(&plan)?;
    if digest != expected
        || plan.transaction != transaction
        || result.get("path").and_then(Value::as_str) != Some(plan.path.as_str())
        || result.get("authority_binding").and_then(Value::as_str)
            != Some(plan.authority_binding.as_str())
    {
        return Err(tool("recursive removal receipt differs from its durable plan"));
    }
    let progress = read_progress(state_root, &plan, &digest)?;
    if progress.next != plan.entries.len() {
        return Err(tool("recursive removal receipt is not durably complete"));
    }
    Ok(checkpoint_entries(&plan))
}

pub(super) fn remove(
    root: &Path,
    grounding: &GroundingEvidence,
    ownership: &WorkspaceOwnership,
    arguments: &Value,
) -> Result<Value, DeveloperLoopError> {
    let relative = removal_path(arguments)?;
    let path = checked(root, relative, false)?;
    let metadata = fs::metadata(&path).map_err(|error| tool(error.to_string()))?;
    if metadata.is_dir() {
        grounding.ensure_empty_directory_removal_allowed(relative).map_err(tool)?;
        if fs::read_dir(&path)
            .map_err(|error| tool(error.to_string()))?
            .next()
            .transpose()
            .map_err(|error| tool(error.to_string()))?
            .is_some()
        {
            return Err(tool("workspace_remove only removes an empty directory in single mode"));
        }
        fs::remove_dir(&path).map_err(|error| tool(error.to_string()))?;
        return Ok(result(relative, "directory"));
    }
    if !metadata.is_file() {
        return Err(tool("workspace_remove requires one regular file or empty directory"));
    }
    grounding.ensure_mutation_allowed(relative, true).map_err(tool)?;
    ownership.ensure_removable(&path)?;
    fs::remove_file(&path).map_err(|error| tool(error.to_string()))?;
    Ok(result(relative, "file"))
}

fn removal_path(arguments: &Value) -> Result<&str, DeveloperLoopError> {
    let relative = arguments
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| tool("path must be text"))?;
    if relative.is_empty() || relative == "." {
        return Err(tool("workspace_remove cannot remove the workspace root"));
    }
    Ok(relative)
}

fn removal_authority(
    root: &Path,
    relative: &str,
    recursive: bool,
    view: &dyn ConversationView,
    arguments: &Value,
) -> Result<RecursiveAuthority, DeveloperLoopError> {
    let authority = arguments
        .get("authority")
        .and_then(Value::as_object)
        .ok_or_else(|| tool("workspace_remove requires typed user-request authority"))?;
    if authority.get("kind").and_then(Value::as_str) != Some("user_request") {
        return Err(tool("workspace_remove authority must be user_request"));
    }
    let binding = authority
        .get("binding")
        .and_then(Value::as_str)
        .ok_or_else(|| tool("workspace_remove authority has no request binding"))?;
    let expected = hex(view.request_source_binding());
    if binding != expected {
        return Err(tool(
            "workspace_remove authority belongs to another governing user-source set",
        ));
    }
    let quote = authority
        .get("quote")
        .and_then(Value::as_str)
        .ok_or_else(|| tool("workspace_remove authority has no exact user quote"))?;
    if quote.is_empty() || !view.reference_authority_context().contains(quote) {
        return Err(tool(
            "workspace_remove authority quote is not exact governing user-authored text",
        ));
    }
    if !quote_authorizes_removal(quote, relative, &root.join(relative), recursive) {
        return Err(tool(
            "the quoted user request does not explicitly authorize this exact removal scope",
        ));
    }
    Ok(RecursiveAuthority {
        binding: expected,
        quote_sha256: hex(Sha256::digest(quote.as_bytes()).into()),
    })
}

fn quote_authorizes_removal(
    quote: &str,
    relative: &str,
    absolute: &Path,
    recursive: bool,
) -> bool {
    let lower = quote.to_ascii_lowercase();
    if [
        "do not delete",
        "do not remove",
        "don't delete",
        "don't remove",
        "never delete",
        "never remove",
        "without deleting",
        "without removing",
    ]
    .iter()
    .any(|denial| lower.contains(denial))
    {
        return false;
    }
    let words = lower
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|word| !word.is_empty())
        .collect::<BTreeSet<_>>();
    let destructive = ["delete", "deleted", "remove", "removed", "erase", "purge"]
        .iter()
        .any(|word| words.contains(word));
    let recursive_scope = !recursive
        || ["recursive", "recursively", "entire", "whole", "contents", "everything"]
            .iter()
            .any(|word| words.contains(word));
    destructive
        && recursive_scope
        && (contains_exact_path(quote, relative)
            || absolute.to_str().is_some_and(|path| contains_exact_path(quote, path)))
}

fn contains_exact_path(text: &str, path: &str) -> bool {
    text.match_indices(path).any(|(start, matched)| {
        let before = text[..start].chars().next_back();
        let suffix = &text[start + matched.len()..];
        before.is_none_or(path_boundary) && trailing_path_boundary(suffix)
    })
}

fn trailing_path_boundary(suffix: &str) -> bool {
    let Some(after) = suffix.chars().next() else { return true };
    if path_boundary(after) {
        return true;
    }
    if after != '.' {
        return false;
    }
    suffix[after.len_utf8()..]
        .chars()
        .next()
        .is_none_or(|character| character.is_whitespace() || path_boundary(character))
}

fn path_boundary(character: char) -> bool {
    !character.is_ascii_alphanumeric()
        && !matches!(character, '_' | '-' | '.' | '/' | '\\')
}

fn enumerate_tree(
    root: &Path,
    relative: &str,
) -> Result<Vec<RemovalEntry>, DeveloperLoopError> {
    let mut pending = vec![PathBuf::from(relative)];
    let mut entries = Vec::new();
    while let Some(relative) = pending.pop() {
        if protected_metadata(&relative) {
            return Err(tool(format!(
                "recursive removal scope contains protected repository metadata: {}",
                relative.display(),
            )));
        }
        let relative_text = relative
            .to_str()
            .ok_or_else(|| tool("recursive removal path is not UTF-8"))?;
        let path = checked(root, relative_text, false)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| tool(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(tool(format!(
                "recursive removal scope contains a symbolic link: {relative_text}",
            )));
        }
        if metadata.is_file() {
            let (sha256, bytes, identity) = hash_file(&path, &metadata)?;
            entries.push(RemovalEntry {
                path: relative_text.to_owned(),
                kind: RemovalEntryKind::File,
                identity,
                permissions: file_metadata::permission_fingerprint(&metadata),
                bytes,
                sha256: Some(sha256),
                empty_preimage: false,
            });
            continue;
        }
        if !metadata.is_dir() {
            return Err(tool(format!(
                "recursive removal scope contains a special filesystem entry: {relative_text}",
            )));
        }
        let mut children = fs::read_dir(&path)
            .map_err(|error| tool(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| tool(error.to_string()))?;
        children.sort_by_key(fs::DirEntry::file_name);
        let empty = children.is_empty();
        for child in children.into_iter().rev() {
            let child_path = child.path();
            let child_relative = child_path
                .strip_prefix(root)
                .map_err(|_| tool("recursive removal child escaped the workspace"))?
                .to_path_buf();
            pending.push(child_relative);
        }
        entries.push(RemovalEntry {
            path: relative_text.to_owned(),
            kind: RemovalEntryKind::Directory,
            identity: file_identity(&metadata)?,
            permissions: file_metadata::permission_fingerprint(&metadata),
            bytes: 0,
            sha256: None,
            empty_preimage: empty,
        });
    }
    entries.sort_by(|left, right| {
        right
            .path
            .split(std::path::MAIN_SEPARATOR)
            .count()
            .cmp(&left.path.split(std::path::MAIN_SEPARATOR).count())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(entries)
}

fn validate_scope(
    root: &Path,
    plan: &RemovalPlan,
    next: usize,
    allow_one_gap: bool,
) -> Result<usize, DeveloperLoopError> {
    if next > plan.entries.len() {
        return Err(tool("recursive removal progress exceeds its exact scope"));
    }
    let current = match fs::symlink_metadata(checked(root, &plan.path, true)?) {
        Ok(_) => enumerate_tree(root, &plan.path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(tool(error.to_string())),
    };
    let current = current
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let planned = plan
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    if let Some(extra) = current.keys().find(|path| !planned.contains_key(path.as_str())) {
        return Err(tool(format!(
            "recursive removal scope gained an unplanned entry: {extra}",
        )));
    }
    for entry in &plan.entries[..next] {
        if current.contains_key(&entry.path) {
            return Err(tool(format!(
                "recursive removal progress says an entry is gone, but it still exists: {}",
                entry.path,
            )));
        }
    }
    let mut gap = None;
    for (index, entry) in plan.entries.iter().enumerate().skip(next) {
        match current.get(&entry.path) {
            Some(actual) if same_preimage(entry, actual) => {}
            Some(_) => {
                return Err(tool(format!(
                    "recursive removal preimage changed before deletion: {}",
                    entry.path,
                )));
            }
            None if allow_one_gap && index == next => gap = Some(index + 1),
            None => {
                return Err(tool(format!(
                    "recursive removal scope lost an entry outside durable progress: {}",
                    entry.path,
                )));
            }
        }
    }
    Ok(gap.unwrap_or(next))
}

fn validate_entry(root: &Path, expected: &RemovalEntry) -> Result<(), DeveloperLoopError> {
    let actual = enumerate_tree(root, &expected.path)?;
    let actual = actual
        .into_iter()
        .find(|entry| entry.path == expected.path)
        .ok_or_else(|| tool("recursive removal entry disappeared before deletion"))?;
    if !same_preimage(expected, &actual) {
        return Err(tool(format!(
            "recursive removal entry changed before deletion: {}",
            expected.path,
        )));
    }
    Ok(())
}

fn same_preimage(expected: &RemovalEntry, actual: &RemovalEntry) -> bool {
    expected.path == actual.path
        && expected.kind == actual.kind
        && expected.identity == actual.identity
        && expected.permissions == actual.permissions
        && expected.bytes == actual.bytes
        && expected.sha256 == actual.sha256
}

fn hash_file(
    path: &Path,
    before: &fs::Metadata,
) -> Result<(String, u64, String), DeveloperLoopError> {
    let expected_identity = file_identity(before)?;
    let mut file = File::open(path).map_err(|error| tool(error.to_string()))?;
    let opened = file.metadata().map_err(|error| tool(error.to_string()))?;
    if !opened.is_file() || file_identity(&opened)? != expected_identity {
        return Err(tool("recursive removal file identity changed while opening its preimage"));
    }
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| tool(error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes
            .checked_add(read as u64)
            .ok_or_else(|| tool("recursive removal file size overflowed"))?;
    }
    let after = fs::symlink_metadata(path).map_err(|error| tool(error.to_string()))?;
    if after.file_type().is_symlink()
        || !after.is_file()
        || file_identity(&after)? != expected_identity
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || before.permissions() != after.permissions()
        || bytes != after.len()
    {
        return Err(tool("recursive removal file changed while capturing its exact preimage"));
    }
    Ok((hex(hasher.finalize().into()), bytes, expected_identity))
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> Result<String, DeveloperLoopError> {
    use std::os::unix::fs::MetadataExt as _;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(metadata: &fs::Metadata) -> Result<String, DeveloperLoopError> {
    use std::os::windows::fs::MetadataExt as _;
    let volume = metadata
        .volume_serial_number()
        .ok_or_else(|| tool("filesystem entry has no stable volume identity"))?;
    let index = metadata
        .file_index()
        .ok_or_else(|| tool("filesystem entry has no stable file identity"))?;
    Ok(format!("windows:{volume}:{index}"))
}

#[cfg(not(any(unix, windows)))]
fn file_identity(metadata: &fs::Metadata) -> Result<String, DeveloperLoopError> {
    let created = metadata
        .created()
        .and_then(|created| {
            created.duration_since(std::time::UNIX_EPOCH).map_err(std::io::Error::other)
        })
        .map_err(|error| tool(error.to_string()))?;
    Ok(format!("portable:{}", created.as_nanos()))
}

fn validate_plan_identity(
    plan: &RemovalPlan,
    transaction: &str,
    path: &str,
    authority: &RecursiveAuthority,
) -> Result<(), DeveloperLoopError> {
    if plan.version != PLAN_VERSION
        || plan.transaction != transaction
        || plan.path != path
        || plan.authority_binding != authority.binding
        || plan.authority_quote_sha256 != authority.quote_sha256
    {
        return Err(tool("recursive removal transaction belongs to another exact request"));
    }
    Ok(())
}

fn checkpoint_entries(plan: &RemovalPlan) -> Vec<(String, WorkspaceMutationKind)> {
    plan.entries
        .iter()
        .filter_map(|entry| match entry.kind {
            RemovalEntryKind::File => Some((entry.path.clone(), WorkspaceMutationKind::File)),
            RemovalEntryKind::Directory if entry.empty_preimage => {
                Some((entry.path.clone(), WorkspaceMutationKind::EmptyDirectory))
            }
            RemovalEntryKind::Directory => None,
        })
        .collect()
}

fn read_plan(path: &Path) -> Result<RemovalPlan, DeveloperLoopError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| tool(format!("inspect recursive removal plan: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(tool("recursive removal plan is not an ordinary file"));
    }
    let bytes = fs::read(path).map_err(|error| tool(format!("read recursive removal plan: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| tool(format!("decode recursive removal plan: {error}")))
}

fn read_progress(
    state_root: &Path,
    plan: &RemovalPlan,
    plan_sha256: &str,
) -> Result<RemovalProgress, DeveloperLoopError> {
    let path = progress_path(state_root, &plan.transaction);
    let progress = match fs::read(&path) {
        Ok(bytes) => {
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| tool(format!("inspect recursive removal progress: {error}")))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(tool("recursive removal progress is not an ordinary file"));
            }
            serde_json::from_slice::<RemovalProgress>(&bytes)
                .map_err(|error| tool(format!("decode recursive removal progress: {error}")))?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => RemovalProgress {
            version: PLAN_VERSION,
            plan_sha256: plan_sha256.to_owned(),
            next: 0,
        },
        Err(error) => return Err(tool(format!("read recursive removal progress: {error}"))),
    };
    if progress.version != PLAN_VERSION
        || progress.plan_sha256 != plan_sha256
        || progress.next > plan.entries.len()
    {
        return Err(tool("recursive removal progress differs from its immutable plan"));
    }
    Ok(progress)
}

fn write_progress(
    state_root: &Path,
    plan: &RemovalPlan,
    progress: &RemovalProgress,
) -> Result<(), DeveloperLoopError> {
    write_json(&progress_path(state_root, &plan.transaction), progress)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), DeveloperLoopError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| tool(error.to_string()))?;
    }
    let bytes = serde_json::to_vec(value).map_err(|error| tool(error.to_string()))?;
    atomic_write(path, &bytes)?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| tool(error.to_string()))?;
    }
    Ok(())
}

fn plan_digest(plan: &RemovalPlan) -> Result<String, DeveloperLoopError> {
    let bytes = serde_json::to_vec(plan).map_err(|error| tool(error.to_string()))?;
    Ok(hex(Sha256::digest(bytes).into()))
}

fn plan_path(root: &Path, transaction: &str) -> PathBuf {
    root.join(format!("{transaction}.plan.json"))
}

fn progress_path(root: &Path, transaction: &str) -> PathBuf {
    root.join(format!("{transaction}.progress.json"))
}

fn result(path: &str, kind: &str) -> Value {
    object_result(vec![
        ("path", Value::String(path.to_owned())),
        ("kind", Value::String(kind.to_owned())),
    ])
}

fn object_result(entries: Vec<(&str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<Map<_, _>>(),
    )
}

fn hex(bytes: [u8; 32]) -> String {
    let mut value = String::with_capacity(64);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
}
