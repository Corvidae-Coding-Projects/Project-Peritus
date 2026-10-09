//! Scoped immutable source captures, exact review artifacts, and replayable init selections.

use super::{SELECTED_SOURCES, app_error, observed_mode, patch_mode};
use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, InitCommand, InitCommandKind, InitCommandVerification,
    InitDiscoveryRequest, InitFileMode, InitProposal, InitSourceKind, InitSourceObservation,
};
use peritus_app_protocol::{
    InitArtifactDiscovery, InitArtifactPage, InitArtifactPageRequest, InitArtifactProposal,
    InitContentReference,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_patch::{
    FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage, WorkspacePath,
};
use peritus_types::{EventId, Sha256Digest};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};
use peritus_workspace::{FolderIdentity, FolderInspection};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::cell::RefCell;
use std::io::{Read as _, Seek as _, Write as _};
use std::{fs, path::Path};

mod commands;
use commands::commands_from_source;

const CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Content {
    digest: [u8; 32],
    bytes: u64,
    chunks: Vec<Object>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Object {
    digest: [u8; 32],
    bytes: u64,
}
impl Object {
    const fn reference(&self) -> InitContentReference {
        InitContentReference::new(Sha256Digest::new(self.digest), self.bytes)
    }
}
impl Content {
    const fn reference(&self) -> InitContentReference {
        InitContentReference::new(Sha256Digest::new(self.digest), self.bytes)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Source {
    path: String,
    kind: u8,
    explicit: bool,
    content: Option<Content>,
    diagnostic: Option<String>,
    mode: u8,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Command {
    kind: u8,
    source: String,
    executable: String,
    arguments: Vec<String>,
    selected: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Manifest {
    version: u8,
    conversation: [u8; 16],
    workspace: [u8; 16],
    revision: u64,
    folder: [u8; 32],
    sources: Vec<Source>,
    commands: Vec<Command>,
    original: Option<Content>,
    proposed: Content,
    diff: Content,
    review: Content,
    mode: u8,
}

struct Archive {
    config: StoreConfig,
    store: ArtifactStore,
    _owner: fs::File,
}
impl Archive {
    fn open(path: &Path) -> Result<Self, AppProtocolError> {
        fs::create_dir_all(path).map_err(unavailable)?;
        let owner = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join("owner.lock"))
            .map_err(unavailable)?;
        owner.lock().map_err(unavailable)?;
        let config = config(path)?;
        let store = ArtifactStore::open(config.clone()).map_err(unavailable)?;
        Ok(Self { config, store, _owner: owner })
    }
    fn object(&self, bytes: &[u8]) -> Result<Object, AppProtocolError> {
        let digest = peritus_codec::sha256(bytes);
        let mut id = [0; 16];
        id.copy_from_slice(&digest.as_bytes()[..16]);
        id[0] |= 1;
        let request = WriteRequest::new(
            ArtifactDigest::from_sha256(digest),
            bytes.len() as u64,
            (bytes.len() as u64).max(1),
            MediaType::new("application/octet-stream").map_err(unavailable)?,
            EncryptionMetadata::unencrypted(),
            EventId::new(id).map_err(unavailable)?,
        );
        let mut writer = self.store.begin_owned_write(request).map_err(unavailable)?;
        writer.write_chunk(bytes).map_err(unavailable)?;
        self.store.complete_write(writer).map_err(unavailable)?;
        Ok(Object { digest: digest.into_bytes(), bytes: bytes.len() as u64 })
    }
    fn content(&self, bytes: &[u8]) -> Result<Content, AppProtocolError> {
        let chunks =
            bytes.chunks(CHUNK_BYTES).map(|chunk| self.object(chunk)).collect::<Result<_, _>>()?;
        Ok(Content {
            digest: peritus_codec::sha256(bytes).into_bytes(),
            bytes: bytes.len() as u64,
            chunks,
        })
    }
    fn source(
        &self,
        reader: &FolderInspection,
        root: &Path,
        path: &str,
        kind: u8,
        explicit: bool,
    ) -> Result<Source, AppProtocolError> {
        let mut result = Source {
            path: path.to_owned(),
            kind,
            explicit,
            content: None,
            diagnostic: None,
            mode: 1,
        };
        let metadata = match fs::symlink_metadata(root.join(path)) {
            Ok(metadata) if metadata.file_type().is_file() => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if explicit {
                    result.diagnostic =
                        Some("explicitly selected source does not exist".to_owned());
                }
                return Ok(result);
            }
            _ => {
                result.diagnostic =
                    Some("source is unavailable or is not a no-follow regular file".to_owned());
                return Ok(result);
            }
        };
        result.mode = mode_code(observed_mode(&metadata));
        let path =
            WorkspacePath::new(path).map_err(|_| app_error(AppErrorCode::InvalidIdentifier))?;
        let failure = RefCell::new(None);
        let mut chunks = Vec::new();
        let scanned = reader.scan_file_chunks(
            &path,
            || failure.borrow().is_some(),
            |_, bytes| match self.object(bytes) {
                Ok(object) => chunks.push(object),
                Err(error) => {
                    *failure.borrow_mut() = Some(error);
                }
            },
        );
        if let Some(error) = failure.into_inner() {
            return Err(error);
        }
        match scanned {
            Ok(Some((bytes, digest))) => {
                result.content = Some(Content { digest: digest.into_bytes(), bytes, chunks });
            }
            _ => {
                result.diagnostic = Some(
                    "source changed during capture or could not be inspected safely".to_owned(),
                );
            }
        }
        Ok(result)
    }
}

/// Captures or extends one revision-bound selection without returning complete source bodies.
/// # Errors
/// Rejects stale scope, corrupt prior artifacts, unsafe instruction preimages, or storage failure.
pub fn discover_init_artifacts_checked(
    root: &Path,
    archive_root: &Path,
    request: &InitArtifactDiscovery,
    read_allowed: &dyn Fn(&str) -> bool,
) -> Result<InitArtifactProposal, AppProtocolError> {
    let identity = FolderIdentity::observe(root).map_err(unavailable)?;
    let reader = FolderInspection::open(&identity).map_err(unavailable)?;
    let archive = Archive::open(archive_root)?;
    let previous = request
        .previous()
        .map(|reference| load_manifest(&archive.config, request.request(), reference))
        .transpose()?;
    if previous.as_ref().is_some_and(|previous| previous.folder != identity.digest().into_bytes()) {
        return Err(app_error(AppErrorCode::StaleRevision));
    }
    let mut sources =
        selected_sources(&archive, &reader, &identity, request, previous.as_ref(), read_allowed)?;
    let commands = selected_commands(&archive.config, &mut sources, request, previous.as_ref())?;
    let instructions = sources
        .iter()
        .find(|source| source.path == peritus_app_protocol::INIT_INSTRUCTION_PATH)
        .ok_or_else(|| app_error(AppErrorCode::MalformedFrame))?;
    if instructions.diagnostic.is_some() {
        return Err(app_error(AppErrorCode::NotReady));
    }
    let original = instructions.content.clone();
    let original_text = original
        .as_ref()
        .map(|content| {
            read_content(&archive.config, content)
                .and_then(|bytes| String::from_utf8(bytes).map_err(unavailable))
        })
        .transpose()?;
    let mode = instructions.mode;
    let observations = observations(&sources)?;
    let selected = commands
        .iter()
        .filter(|command| command.selected)
        .map(protocol_command)
        .collect::<Result<Vec<_>, _>>()?;
    let proposal = InitProposal::from_discovery(
        request.request(),
        identity.digest(),
        observations,
        original_text.map(|text| (text, file_mode(mode))),
        selected,
    )?;
    let proposed = archive.content(proposal.patch().proposed_content().as_bytes())?;
    let diff = archive.content(proposal.patch().diff().as_bytes())?;
    let review = review_content(&archive, &sources, &commands, proposal.patch().diff())?;
    let scope = request.request();
    let manifest = Manifest {
        version: 1,
        conversation: scope.query().conversation().into_bytes(),
        workspace: scope.query().workspace().into_bytes(),
        revision: scope.revision(),
        folder: identity.digest().into_bytes(),
        sources,
        commands,
        original,
        proposed,
        diff,
        review,
        mode,
    };
    let encoded = serde_json::to_vec(&manifest).map_err(unavailable)?;
    let object = archive.object(&encoded)?;
    Ok(InitArtifactProposal::new(scope, object.reference(), manifest.review.reference()))
}

/// Reads one exact artifact page after the caller authenticates its workspace and conversation.
/// # Errors
/// Rejects altered proposal bindings, corrupt objects, invalid ranges, and unavailable archives.
pub fn init_artifact_page_checked(
    archive_root: &Path,
    request: InitArtifactPageRequest,
    read_allowed: &dyn Fn(&str) -> bool,
) -> Result<InitArtifactPage, AppProtocolError> {
    let config = config(archive_root)?;
    let proposal = request.proposal();
    let manifest = load_manifest(&config, proposal.request(), proposal.manifest())?;
    if manifest.sources.iter().any(|source| source.content.is_some() && !read_allowed(&source.path))
    {
        return Err(app_error(AppErrorCode::ReadOnly));
    }
    if manifest.review.reference() != proposal.review() {
        return Err(app_error(AppErrorCode::StaleRevision));
    }
    let bytes =
        content_range(&config, &manifest.review, request.offset(), u64::from(request.maximum()))?;
    InitArtifactPage::new(request, bytes)
}

/// Revalidates archived source observations and builds the existing exact authenticated patch.
/// # Errors
/// Rejects source drift, changed mode, corrupt content, altered selection, or proposal mismatch.
pub fn prepare_init_artifact_patch_checked(
    root: &Path,
    archive_root: &Path,
    workspace: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    proposal: InitArtifactProposal,
    read_allowed: &dyn Fn(&str) -> bool,
) -> Result<PatchSet, AppProtocolError> {
    if proposal.query().workspace() != workspace {
        return Err(app_error(AppErrorCode::SessionMismatch));
    }
    let selection =
        InitArtifactDiscovery::new(proposal.request(), Some(proposal.manifest()), None, None)?;
    let current = discover_init_artifacts_checked(root, archive_root, &selection, read_allowed)?;
    if current != proposal {
        return Err(app_error(AppErrorCode::StaleRevision));
    }
    let config = config(archive_root)?;
    let manifest = load_manifest(&config, proposal.request(), proposal.manifest())?;
    let final_file = FinalFile::new(
        read_content(&config, &manifest.proposed)?,
        patch_mode(file_mode(manifest.mode)),
        LineEndingPolicy::Preserve,
    )
    .map_err(unavailable)?;
    let path =
        WorkspacePath::new(peritus_app_protocol::INIT_INSTRUCTION_PATH).map_err(unavailable)?;
    let operation = if let Some(original) = &manifest.original {
        PatchOperation::replace(
            path,
            Preimage::present(
                Sha256Digest::new(original.digest),
                original.bytes,
                patch_mode(file_mode(manifest.mode)),
            ),
            final_file,
        )
        .map_err(unavailable)?
    } else {
        PatchOperation::create(path, final_file)
    };
    PatchSet::new(workspace, generation, revision, vec![operation]).map_err(unavailable)
}

fn config(root: &Path) -> Result<StoreConfig, AppProtocolError> {
    StoreConfig::new_with_quota_policy(root.join("objects"), i64::MAX as u64, None)
        .map_err(unavailable)
}
fn load_manifest(
    config: &StoreConfig,
    request: InitDiscoveryRequest,
    reference: InitContentReference,
) -> Result<Manifest, AppProtocolError> {
    let bytes = read_object(
        config,
        &Object { digest: reference.digest().into_bytes(), bytes: reference.bytes() },
    )?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(unavailable)?;
    if manifest.version != 1
        || manifest.conversation != request.query().conversation().into_bytes()
        || manifest.workspace != request.query().workspace().into_bytes()
        || manifest.revision != request.revision()
    {
        return Err(app_error(AppErrorCode::StaleRevision));
    }
    Ok(manifest)
}
fn read_object(config: &StoreConfig, object: &Object) -> Result<Vec<u8>, AppProtocolError> {
    let bytes = ArtifactStore::read_existing(
        config,
        ArtifactDigest::from_sha256(Sha256Digest::new(object.digest)),
        object.bytes.max(1),
    )
    .map_err(unavailable)?;
    if bytes.len() as u64 != object.bytes {
        return Err(app_error(AppErrorCode::MalformedFrame));
    }
    Ok(bytes)
}
fn read_content(config: &StoreConfig, content: &Content) -> Result<Vec<u8>, AppProtocolError> {
    let bytes = content_range(config, content, 0, content.bytes)?;
    if bytes.len() as u64 != content.bytes
        || peritus_codec::sha256(&bytes).into_bytes() != content.digest
    {
        return Err(app_error(AppErrorCode::MalformedFrame));
    }
    Ok(bytes)
}
fn content_range(
    config: &StoreConfig,
    content: &Content,
    offset: u64,
    maximum: u64,
) -> Result<Vec<u8>, AppProtocolError> {
    let end = offset.saturating_add(maximum).min(content.bytes);
    let mut bytes = Vec::new();
    let mut cursor = 0_u64;
    for object in &content.chunks {
        let next = cursor
            .checked_add(object.bytes)
            .ok_or_else(|| app_error(AppErrorCode::MalformedFrame))?;
        if next > offset && cursor < end {
            let chunk = read_object(config, object)?;
            let start = usize::try_from(offset.saturating_sub(cursor)).map_err(unavailable)?;
            let stop = usize::try_from(end.min(next) - cursor).map_err(unavailable)?;
            bytes.try_reserve(stop - start).map_err(unavailable)?;
            bytes.extend_from_slice(&chunk[start..stop]);
        }
        cursor = next;
    }
    if cursor != content.bytes || offset > cursor || bytes.len() as u64 != end - offset {
        return Err(app_error(AppErrorCode::MalformedFrame));
    }
    Ok(bytes)
}
fn observations(sources: &[Source]) -> Result<Vec<InitSourceObservation>, AppProtocolError> {
    sources
        .iter()
        .filter_map(|source| {
            source.content.as_ref().map(|content| {
                InitSourceObservation::new(
                    source.path.clone(),
                    source_kind(source.kind),
                    Sha256Digest::new(content.digest),
                    content.bytes,
                )
            })
        })
        .collect()
}
fn review_content(
    archive: &Archive,
    sources: &[Source],
    commands: &[Command],
    diff: &str,
) -> Result<Content, AppProtocolError> {
    let mut temporary = tempfile::tempfile().map_err(unavailable)?;
    for source in sources {
        if let Some(content) = &source.content {
            writeln!(
                temporary,
                "source {}: {} bytes, SHA-256 {:?}",
                source.path,
                content.bytes,
                Sha256Digest::new(content.digest)
            )
            .map_err(unavailable)?;
        }
        if let Some(diagnostic) = &source.diagnostic {
            writeln!(temporary, "source {}: {diagnostic}", source.path).map_err(unavailable)?;
        }
    }
    writeln!(temporary, "\nCommands are unverified and are never executed by initialization:")
        .map_err(unavailable)?;
    for (index, command) in commands.iter().enumerate() {
        writeln!(
            temporary,
            "{} [{}] {} {} (from {})",
            index + 1,
            if command.selected { "selected" } else { "excluded" },
            command.executable,
            command.arguments.join(" "),
            command.source
        )
        .map_err(unavailable)?;
    }
    writeln!(temporary, "\nExact instruction-file diff:\n{diff}").map_err(unavailable)?;
    temporary.rewind().map_err(unavailable)?;
    let mut buffer = vec![0; CHUNK_BYTES];
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut chunks = Vec::new();
    loop {
        let read = temporary.read(&mut buffer).map_err(unavailable)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes += read as u64;
        chunks.push(archive.object(&buffer[..read])?);
    }
    Ok(Content { digest: hasher.finalize().into(), bytes, chunks })
}
fn command_key(command: &Command) -> (u8, &str, &str, &[String]) {
    (command.kind, &command.source, &command.executable, &command.arguments)
}
fn command_record(command: &InitCommand, selected: bool) -> Command {
    Command {
        kind: command_kind_code(command.kind()),
        source: command.source().to_owned(),
        executable: command.executable().to_owned(),
        arguments: command.arguments().to_vec(),
        selected,
    }
}
fn protocol_command(command: &Command) -> Result<InitCommand, AppProtocolError> {
    InitCommand::new(
        command_kind(command.kind),
        command.source.clone(),
        command.executable.clone(),
        command.arguments.clone(),
        InitCommandVerification::Unverified,
    )
}
const fn kind_code(kind: InitSourceKind) -> u8 {
    match kind {
        InitSourceKind::Manifest => 1,
        InitSourceKind::Documentation => 2,
        InitSourceKind::Instructions => 3,
        InitSourceKind::CommandConfig => 4,
    }
}
const fn source_kind(kind: u8) -> InitSourceKind {
    match kind {
        1 => InitSourceKind::Manifest,
        2 => InitSourceKind::Documentation,
        3 => InitSourceKind::Instructions,
        _ => InitSourceKind::CommandConfig,
    }
}
const fn mode_code(mode: InitFileMode) -> u8 {
    match mode {
        InitFileMode::Regular => 1,
        InitFileMode::Executable => 2,
    }
}
const fn file_mode(mode: u8) -> InitFileMode {
    if mode == 2 { InitFileMode::Executable } else { InitFileMode::Regular }
}
const fn command_kind_code(kind: InitCommandKind) -> u8 {
    match kind {
        InitCommandKind::Build => 1,
        InitCommandKind::Test => 2,
        InitCommandKind::Lint => 3,
        InitCommandKind::Launch => 4,
    }
}
const fn command_kind(kind: u8) -> InitCommandKind {
    match kind {
        2 => InitCommandKind::Test,
        3 => InitCommandKind::Lint,
        4 => InitCommandKind::Launch,
        _ => InitCommandKind::Build,
    }
}
fn unavailable<E>(_error: E) -> AppProtocolError {
    app_error(AppErrorCode::NotReady)
}

#[cfg(test)]
mod tests;

fn selected_sources(
    archive: &Archive,
    reader: &FolderInspection,
    identity: &FolderIdentity,
    request: &InitArtifactDiscovery,
    previous: Option<&Manifest>,
    read_allowed: &dyn Fn(&str) -> bool,
) -> Result<Vec<Source>, AppProtocolError> {
    let mut selections = previous.map_or_else(
        || {
            SELECTED_SOURCES
                .iter()
                .map(|(path, kind)| ((*path).to_owned(), kind_code(*kind), false))
                .collect::<Vec<_>>()
        },
        |manifest| {
            manifest
                .sources
                .iter()
                .map(|source| (source.path.clone(), source.kind, source.explicit))
                .collect()
        },
    );
    if let Some(source) = request.source() {
        WorkspacePath::new(source.path())
            .map_err(|_| app_error(AppErrorCode::InvalidIdentifier))?;
        if source.path() == peritus_app_protocol::INIT_INSTRUCTION_PATH
            && source.kind() != InitSourceKind::Instructions
        {
            return Err(app_error(AppErrorCode::MalformedFrame));
        }
        selections.retain(|(path, _, _)| path != source.path());
        selections.push((source.path().to_owned(), kind_code(source.kind()), true));
    }
    selections.sort();
    let sources = selections
        .iter()
        .map(|(path, kind, explicit)| {
            if read_allowed(path) {
                archive.source(reader, identity.root(), path, *kind, *explicit)
            } else {
                Ok(Source {
                    path: path.clone(),
                    kind: *kind,
                    explicit: *explicit,
                    content: None,
                    diagnostic: Some("source is outside the current read policy".to_owned()),
                    mode: 1,
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(sources)
}
fn selected_commands(
    config: &StoreConfig,
    sources: &mut [Source],
    request: &InitArtifactDiscovery,
    previous: Option<&Manifest>,
) -> Result<Vec<Command>, AppProtocolError> {
    let mut commands = Vec::new();
    for source in sources {
        if matches!(source.kind, 1 | 4)
            && let Some(content) = &source.content
        {
            let bytes = read_content(config, content)?;
            match commands_from_source(&source.path, &bytes) {
                Ok(discovered) => commands.extend(discovered),
                Err(detail) => source.diagnostic = Some(detail),
            }
        }
    }
    commands.sort();
    commands.dedup();
    let mut commands =
        commands.iter().map(|command| command_record(command, true)).collect::<Vec<_>>();
    if let Some(previous) = previous {
        let choices = previous
            .commands
            .iter()
            .map(|command| (command_key(command), command.selected))
            .collect::<std::collections::BTreeMap<_, _>>();
        for command in &mut commands {
            if let Some(selected) = choices.get(&command_key(command)) {
                command.selected = *selected;
            }
        }
        if let Some(ordinal) = request.command() {
            let old = usize::try_from(ordinal)
                .ok()
                .and_then(|index| previous.commands.get(index))
                .ok_or_else(|| app_error(AppErrorCode::InvalidIdentifier))?;
            let command = commands
                .iter_mut()
                .find(|command| command_key(command) == command_key(old))
                .ok_or_else(|| app_error(AppErrorCode::StaleRevision))?;
            command.selected = !old.selected;
        }
    }
    Ok(commands)
}
