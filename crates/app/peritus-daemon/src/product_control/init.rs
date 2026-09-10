//! Bounded local project discovery and exact reviewed initialization patch preparation.
//!
//! This module has no process or provider dependency. Discovery reads only a fixed source list;
//! preparation repeats discovery and returns inert patch data for the existing authority gateway.

use peritus_app_protocol::{
    AppErrorCode, AppProtocolError, InitCommand, InitCommandKind, InitCommandVerification,
    InitDiscoveryRequest, InitFileMode, InitProposal, InitSourceKind, InitSourceObservation,
    MAX_INIT_SOURCE_BYTES,
};
use peritus_patch::{
    FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage, WorkspacePath,
};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};
use peritus_workspace::{FileReadSelection, FolderIdentity, FolderInspection};
use std::{fs, path::Path};

#[cfg(test)]
mod tests;

const SELECTED_SOURCES: &[(&str, InitSourceKind)] = &[
    (".cargo/config", InitSourceKind::CommandConfig),
    (".cargo/config.toml", InitSourceKind::CommandConfig),
    (".github/copilot-instructions.md", InitSourceKind::Instructions),
    ("AGENTS.md", InitSourceKind::Instructions),
    ("CLAUDE.md", InitSourceKind::Instructions),
    ("Cargo.toml", InitSourceKind::Manifest),
    ("Makefile", InitSourceKind::CommandConfig),
    ("README", InitSourceKind::Documentation),
    ("README.md", InitSourceKind::Documentation),
    ("justfile", InitSourceKind::CommandConfig),
    ("package.json", InitSourceKind::Manifest),
    ("pyproject.toml", InitSourceKind::Manifest),
];

const fn app_error(code: AppErrorCode) -> AppProtocolError {
    AppProtocolError::new(code, None)
}

struct SelectedSource {
    observation: InitSourceObservation,
    bytes: Vec<u8>,
    mode: InitFileMode,
}

/// Discovers a fixed bounded set of root-local project sources without running commands.
///
/// Missing selected sources are simply absent from the proposal. Present sources are opened
/// handle-relatively without following symlinks, read completely under the 256 KiB ceiling, and
/// represented by exact digests. This function performs no writes, provider calls, package-script
/// execution, dependency installation, recursive scan, or network access.
///
/// # Errors
/// Rejects changed folder identity, unsafe source shapes, oversized files, non-UTF-8 instructions,
/// malformed managed-section markers, or invalid protocol bounds.
pub fn discover_init(
    root: &Path,
    request: InitDiscoveryRequest,
) -> Result<InitProposal, AppProtocolError> {
    let identity = FolderIdentity::observe(root).map_err(|_| app_error(AppErrorCode::NotReady))?;
    let reader =
        FolderInspection::open(&identity).map_err(|_| app_error(AppErrorCode::NotReady))?;
    let mut selected = Vec::new();
    for &(path, kind) in SELECTED_SOURCES {
        if let Some(source) = read_selected(&reader, identity.root(), path, kind)? {
            selected.push(source);
        }
    }

    let mut commands = Vec::new();
    for source in &selected {
        discover_commands(&source.observation, &source.bytes, &mut commands)?;
    }
    commands.sort_unstable();
    commands.dedup();

    let original = selected
        .iter()
        .find(|source| source.observation.path() == peritus_app_protocol::INIT_INSTRUCTION_PATH)
        .map(|source| {
            String::from_utf8(source.bytes.clone())
                .map(|content| (content, source.mode))
                .map_err(|_| app_error(AppErrorCode::MalformedFrame))
        })
        .transpose()?;
    let mut observations =
        selected.into_iter().map(|source| source.observation).collect::<Vec<_>>();
    observations.sort_unstable();
    InitProposal::from_discovery(request, identity.digest(), observations, original, commands)
}

/// Re-discovers the project and prepares only the exact currently reviewed patch.
///
/// The returned [`PatchSet`] is inert. The caller must pass it through the current
/// `peritus_workspace::WorkspaceGateway` with an exact authorization request. Generation and
/// revision are lower-layer patch bindings only and do not derive or imply authority.
///
/// # Errors
/// Rejects another workspace, changed folder/source bytes, a modified proposal, or invalid patch
/// construction. Any intervening `AGENTS.md` edit therefore fails before a write is attempted; the
/// patch transaction independently rechecks the same preimage at application time.
pub fn prepare_init_patch(
    root: &Path,
    workspace_id: WorkspaceId,
    generation: Generation,
    revision: RevisionNumber,
    proposal: &InitProposal,
) -> Result<PatchSet, AppProtocolError> {
    if proposal.query().workspace() != workspace_id {
        return Err(app_error(AppErrorCode::SessionMismatch));
    }
    let request = InitDiscoveryRequest::new(proposal.query(), proposal.revision())?;
    let current = discover_init(root, request)?;
    if &current != proposal {
        return Err(app_error(AppErrorCode::StaleRevision));
    }

    let reviewed = proposal.patch();
    let path = WorkspacePath::new(reviewed.path())
        .map_err(|_| app_error(AppErrorCode::InvalidIdentifier))?;
    let mode = patch_mode(reviewed.mode());
    let final_file = FinalFile::new(
        reviewed.proposed_content().as_bytes().to_vec(),
        mode,
        LineEndingPolicy::Preserve,
    )
    .map_err(|_| app_error(AppErrorCode::MalformedFrame))?;
    let operation = match (reviewed.original_content(), reviewed.precondition_digest()) {
        (None, None) => PatchOperation::create(path, final_file),
        (Some(_), Some(digest)) => PatchOperation::replace(
            path,
            Preimage::present(digest, reviewed.precondition_bytes(), mode),
            final_file,
        )
        .map_err(|_| app_error(AppErrorCode::MalformedFrame))?,
        _ => return Err(app_error(AppErrorCode::MalformedFrame)),
    };
    PatchSet::new(workspace_id, generation, revision, vec![operation])
        .map_err(|_| app_error(AppErrorCode::MalformedFrame))
}

fn read_selected(
    reader: &FolderInspection,
    root: &Path,
    path: &str,
    kind: InitSourceKind,
) -> Result<Option<SelectedSource>, AppProtocolError> {
    let absolute = root.join(path);
    let metadata = match fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(app_error(AppErrorCode::NotReady)),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(app_error(AppErrorCode::InvalidIdentifier));
    }
    if metadata.len() > MAX_INIT_SOURCE_BYTES as u64 {
        return Err(app_error(AppErrorCode::LimitExceeded));
    }
    let workspace_path =
        WorkspacePath::new(path).map_err(|_| app_error(AppErrorCode::InvalidIdentifier))?;
    let inspected = reader
        .read_file(&workspace_path, FileReadSelection::all(), MAX_INIT_SOURCE_BYTES as u64)
        .map_err(|_| app_error(AppErrorCode::NotReady))?;
    let observation = InitSourceObservation::new(
        path.to_owned(),
        kind,
        inspected.source_digest(),
        inspected.source_bytes(),
    )?;
    Ok(Some(SelectedSource {
        observation,
        bytes: inspected.bytes().to_vec(),
        mode: observed_mode(&metadata),
    }))
}

fn discover_commands(
    source: &InitSourceObservation,
    bytes: &[u8],
    commands: &mut Vec<InitCommand>,
) -> Result<(), AppProtocolError> {
    match source.path() {
        "Cargo.toml" => cargo_manifest_commands(source.path(), bytes, commands),
        ".cargo/config" | ".cargo/config.toml" => {
            cargo_alias_commands(source.path(), bytes, commands)
        }
        "package.json" => package_commands(source.path(), bytes, commands),
        "Makefile" => target_commands("make", source.path(), bytes, commands),
        "justfile" => target_commands("just", source.path(), bytes, commands),
        _ => Ok(()),
    }
}

fn cargo_manifest_commands(
    source: &str,
    bytes: &[u8],
    commands: &mut Vec<InitCommand>,
) -> Result<(), AppProtocolError> {
    let Ok(text) = std::str::from_utf8(bytes) else { return Ok(()) };
    let Ok(document) = text.parse::<toml::Table>() else { return Ok(()) };
    if document.get("package").is_none() && document.get("workspace").is_none() {
        return Ok(());
    }
    push_command(commands, InitCommandKind::Build, source, "cargo", &["build"])?;
    push_command(commands, InitCommandKind::Test, source, "cargo", &["test"])?;
    push_command(
        commands,
        InitCommandKind::Lint,
        source,
        "cargo",
        &["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"],
    )?;
    let has_launch_target = document.get("package").is_some()
        || document
            .get("workspace")
            .and_then(|workspace| workspace.get("default-members"))
            .and_then(toml::Value::as_array)
            .is_some_and(|members| !members.is_empty());
    if has_launch_target {
        push_command(commands, InitCommandKind::Launch, source, "cargo", &["run"])?;
    }
    Ok(())
}

fn cargo_alias_commands(
    source: &str,
    bytes: &[u8],
    commands: &mut Vec<InitCommand>,
) -> Result<(), AppProtocolError> {
    let Ok(text) = std::str::from_utf8(bytes) else { return Ok(()) };
    let Ok(document) = text.parse::<toml::Table>() else { return Ok(()) };
    let Some(aliases) = document.get("alias").and_then(toml::Value::as_table) else {
        return Ok(());
    };
    for (name, kind) in [
        ("build", InitCommandKind::Build),
        ("test", InitCommandKind::Test),
        ("lint", InitCommandKind::Lint),
        ("run", InitCommandKind::Launch),
    ] {
        if aliases.contains_key(name) {
            push_command(commands, kind, source, "cargo", &[name])?;
        }
    }
    Ok(())
}

fn package_commands(
    source: &str,
    bytes: &[u8],
    commands: &mut Vec<InitCommand>,
) -> Result<(), AppProtocolError> {
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Ok(());
    };
    let Some(scripts) = document.get("scripts").and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    for (name, kind) in [
        ("build", InitCommandKind::Build),
        ("test", InitCommandKind::Test),
        ("lint", InitCommandKind::Lint),
        ("start", InitCommandKind::Launch),
        ("dev", InitCommandKind::Launch),
    ] {
        if scripts
            .get(name)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
        {
            push_command(commands, kind, source, "npm", &["run", name])?;
        }
    }
    Ok(())
}

fn target_commands(
    executable: &str,
    source: &str,
    bytes: &[u8],
    commands: &mut Vec<InitCommand>,
) -> Result<(), AppProtocolError> {
    let Ok(text) = std::str::from_utf8(bytes) else { return Ok(()) };
    for (target, kind) in [
        ("build", InitCommandKind::Build),
        ("test", InitCommandKind::Test),
        ("lint", InitCommandKind::Lint),
        ("run", InitCommandKind::Launch),
        ("start", InitCommandKind::Launch),
    ] {
        if text.lines().any(|line| declared_target(line, target)) {
            push_command(commands, kind, source, executable, &[target])?;
        }
    }
    Ok(())
}

fn declared_target(line: &str, target: &str) -> bool {
    if line.starts_with(char::is_whitespace) || line.trim_start().starts_with('#') {
        return false;
    }
    line.strip_prefix(target).is_some_and(|rest| {
        rest.starts_with(':') || rest.starts_with(char::is_whitespace) && rest.contains(':')
    })
}

fn push_command(
    commands: &mut Vec<InitCommand>,
    kind: InitCommandKind,
    source: &str,
    executable: &str,
    arguments: &[&str],
) -> Result<(), AppProtocolError> {
    commands.push(InitCommand::new(
        kind,
        source.to_owned(),
        executable.to_owned(),
        arguments.iter().map(|argument| (*argument).to_owned()).collect(),
        InitCommandVerification::Unverified,
    )?);
    Ok(())
}

const fn patch_mode(mode: InitFileMode) -> FileMode {
    match mode {
        InitFileMode::Regular => FileMode::Regular,
        InitFileMode::Executable => FileMode::Executable,
    }
}

fn observed_mode(metadata: &fs::Metadata) -> InitFileMode {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o111 != 0 {
            return InitFileMode::Executable;
        }
    }
    InitFileMode::Regular
}
