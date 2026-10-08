//! Optional auxiliary inference through the existing C2 authority and native C3 gateway.

mod backend;
#[cfg(test)]
mod tests;

use super::{CommandRuntime, authority, contract, identity, sandbox};
use crate::LocalProcessConfig;
use peritus_artifact_store::{ArtifactDigest, ArtifactReadHandle, ArtifactStore, StoreConfig};
use peritus_process::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionPlan, IoMode, NativeSandboxBackend,
    OsExitObservation, OutputArtifact, OutputCompleteness, OutputPolicy, OutputStream,
    ProcessResourcePolicy, StdinPolicy, TerminalDisposition, WorkingDirectory, WorkspaceAccess,
};
use peritus_provider_core::CancellationToken;
use peritus_sandbox::{AdmissionProfile, BackendAdmission, CheckedSandboxPlan, admit_backend};
use peritus_spec::AcceptanceContract;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

const COPY_CHUNK_BYTES: usize = 64 * 1024;
static SNAPSHOT_PUBLICATION: Mutex<()> = Mutex::new(());

struct SnapshotFile {
    source: PathBuf,
    object: PathBuf,
    path: PathBuf,
    digest: Sha256Digest,
    bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DispatchIdentityMode {
    ImmutableSnapshotBind,
    ConfiguredSourceRevalidated,
}

impl DispatchIdentityMode {
    const fn proposal_identity(self) -> &'static [u8] {
        match self {
            Self::ImmutableSnapshotBind => b"immutable_snapshot_bind/v1",
            Self::ConfiguredSourceRevalidated => b"configured_source_revalidated/v1",
        }
    }

    const fn binds_snapshot(self) -> bool {
        matches!(self, Self::ImmutableSnapshotBind)
    }
}

/// Content-bound local-inference proposal prepared before its durable optional receipt.
pub(crate) struct PreparedLocalCompactor {
    runtime: CommandRuntime,
    cancellation: CancellationToken,
    input_page_bytes: usize,
    directory: PathBuf,
    executable: SnapshotFile,
    weights: SnapshotFile,
    dispatch_mode: DispatchIdentityMode,
    ids: identity::CommandIds,
    contract: AcceptanceContract,
    backend: backend::LocalBackend,
    checked: CheckedSandboxPlan,
    admission: BackendAdmission,
    plan: ExecutionPlan,
    proposal_digest: Sha256Digest,
}

/// Complete stdout retained as ordered immutable artifact segments.
pub(crate) struct LocalCompactorOutput {
    artifacts: ArtifactStore,
    segments: Vec<OutputArtifact>,
    cancellation: CancellationToken,
    next_segment: usize,
    current: Option<ArtifactReadHandle>,
    total_bytes: u64,
    read_bytes: u64,
}

impl core::fmt::Debug for LocalCompactorOutput {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("LocalCompactorOutput")
            .field("segments", &self.segments.len())
            .field("total_bytes", &self.total_bytes)
            .field("read_bytes", &self.read_bytes)
            .finish_non_exhaustive()
    }
}

impl LocalCompactorOutput {
    fn new(
        artifacts: ArtifactStore,
        mut segments: Vec<OutputArtifact>,
        cancellation: CancellationToken,
    ) -> Result<Self, String> {
        segments.sort_by_key(|segment| segment.start_offset());
        let mut next_offset = 0_u64;
        for segment in &segments {
            let end = segment
                .start_offset()
                .checked_add(segment.size())
                .ok_or("local compactor output offset overflow")?;
            if segment.stream() != OutputStream::Stdout
                || segment.completeness() != OutputCompleteness::Complete
                || segment.start_offset() != next_offset
                || segment.end_offset() != end
            {
                return Err("local compactor output artifacts are incomplete or discontinuous"
                    .to_owned());
            }
            next_offset = end;
        }
        Ok(Self {
            artifacts,
            segments,
            cancellation,
            next_segment: 0,
            current: None,
            total_bytes: next_offset,
            read_bytes: 0,
        })
    }

    fn open_next_segment(&mut self) -> io::Result<bool> {
        let Some(segment) = self.segments.get(self.next_segment).copied() else {
            if self.read_bytes != self.total_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "local compactor output ended before its retained frontier",
                ));
            }
            return Ok(false);
        };
        let reader = self
            .artifacts
            .open_read(ArtifactDigest::from_sha256(segment.digest()))
            .map_err(io::Error::other)?;
        if reader.metadata().size() != segment.size() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "local compactor artifact metadata differs from terminal publication",
            ));
        }
        self.current = Some(reader);
        Ok(true)
    }
}

impl Read for LocalCompactorOutput {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            if self.cancellation.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "local compactor output transfer was cancelled",
                ));
            }
            if self.current.is_none() && !self.open_next_segment()? {
                return Ok(0);
            }
            let reader = self
                .current
                .as_mut()
                .ok_or_else(|| io::Error::other("local compactor reader owner is absent"))?;
            match reader.read_chunk(buffer.len()).map_err(io::Error::other)? {
                Some(chunk) => {
                    let expected = self
                        .segments
                        .get(self.next_segment)
                        .ok_or_else(|| io::Error::other("local compactor segment is absent"))?
                        .start_offset()
                        .checked_add(chunk.offset())
                        .ok_or_else(|| io::Error::other("local compactor read offset overflow"))?;
                    if expected != self.read_bytes || chunk.bytes().len() > buffer.len() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "local compactor artifact reader returned a discontinuous chunk",
                        ));
                    }
                    buffer[..chunk.bytes().len()].copy_from_slice(chunk.bytes());
                    self.read_bytes = self
                        .read_bytes
                        .checked_add(u64::try_from(chunk.bytes().len()).map_err(io::Error::other)?)
                        .ok_or_else(|| io::Error::other("local compactor read size overflow"))?;
                    return Ok(chunk.bytes().len());
                }
                None => {
                    self.current = None;
                    self.next_segment = self
                        .next_segment
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("local compactor segment overflow"))?;
                }
            }
        }
    }
}

impl PreparedLocalCompactor {
    /// Returns the content, dispatch mode, backend, sandbox, authority, and resource identity.
    #[must_use]
    pub(crate) const fn proposal_digest(&self) -> Sha256Digest {
        self.proposal_digest
    }

    /// Revalidates retained content before authority. Linux binds immutable references only when
    /// their state filesystem permits execution; fallback Linux and other native targets rehash
    /// the configured sources immediately before committing authority.
    pub(crate) fn execute(self, mut input: impl Read) -> Result<LocalCompactorOutput, String> {
        ensure_not_cancelled(&self.cancellation)?;
        let mut input_page = vec![0_u8; self.input_page_bytes];
        let mut input_read = input.read(&mut input_page).map_err(detail)?;
        if input_read == 0 {
            return Err("local compactor input is empty".to_owned());
        }
        let artifact_config = StoreConfig::for_available_space_without_artifact_limit(
            self.directory.join("output-artifacts"),
        )
        .map_err(detail)?;
        let artifacts = ArtifactStore::open(artifact_config).map_err(detail)?;
        let creating_event = self.ids.event("local-compactor-output")?;
        verify_snapshot(&self.executable, &self.cancellation)?;
        verify_snapshot(&self.weights, &self.cancellation)?;
        verify_dispatch_source(
            &self.executable,
            true,
            self.dispatch_mode,
            &self.cancellation,
        )?;
        verify_dispatch_source(
            &self.weights,
            false,
            self.dispatch_mode,
            &self.cancellation,
        )?;
        ensure_not_cancelled(&self.cancellation)?;
        let process_authority = authority::commit_process(
            &self.directory.join("authority.sqlite3"),
            &self.ids,
            &self.contract,
            &self.plan,
        )?;
        let authorization = process_authority.request(&self.ids, &self.plan);
        ensure_not_cancelled(&self.cancellation)?;
        let process = self
            .runtime
            .inner
            .gateway
            .launch_with_backend(
                &authorization,
                self.plan,
                &self.checked,
                &self.admission,
                self.backend,
            )
            .map_err(detail)?;
        let control = process.control();
        let mut cancelled = self.cancellation.is_cancelled();
        let mut input_failure = None;
        loop {
            if cancelled {
                break;
            }
            match control.write_stdin_while(input_page[..input_read].to_vec(), || {
                !self.cancellation.is_cancelled()
            }) {
                Ok(true) => {}
                Ok(false) => cancelled = true,
                Err(error) => {
                    cancelled = true;
                    input_failure = Some(detail(error));
                }
            }
            if cancelled {
                break;
            }
            match input.read(&mut input_page) {
                Ok(0) => break,
                Ok(read) => input_read = read,
                Err(error) => {
                    cancelled = true;
                    input_failure = Some(detail(error));
                }
            }
        }
        if !cancelled {
            match control.close_stdin_while(|| !self.cancellation.is_cancelled()) {
                Ok(true) => {}
                Ok(false) => cancelled = true,
                Err(error) => {
                    cancelled = true;
                    input_failure = Some(detail(error));
                }
            }
        }
        let terminal = process
            .wait_and_publish_cancellable(&artifacts, creating_event, || {
                cancelled || self.cancellation.is_cancelled()
            })
            .map_err(detail)?;
        if let Some(error) = input_failure {
            return Err(error);
        }
        ensure_not_cancelled(&self.cancellation)?;
        if terminal.disposition() != TerminalDisposition::Exited
            || terminal.os_exit() != &OsExitObservation::Code(0)
            || !terminal.tree_cleanup_complete()
            || !terminal.support_tasks_joined()
            || !terminal.output().is_complete()
            || !terminal.artifact_publication_complete()
        {
            return Err(format!(
                "local compactor did not produce a complete successful result: {:?}",
                terminal.disposition()
            ));
        }
        let stdout = terminal
            .artifacts()
            .iter()
            .copied()
            .filter(|artifact| artifact.stream() == OutputStream::Stdout)
            .collect();
        ensure_not_cancelled(&self.cancellation)?;
        LocalCompactorOutput::new(artifacts, stdout, self.cancellation)
    }
}

impl CommandRuntime {
    pub(crate) fn compact_local(
        &self,
        config: &LocalProcessConfig,
        input: &[u8],
    ) -> Result<LocalCompactorOutput, String> {
        self.prepare_local_compactor_cancellable(config, &CancellationToken::new())?
            .execute(input)
    }

    pub(crate) fn compact_local_cancellable(
        &self,
        config: &LocalProcessConfig,
        input: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<LocalCompactorOutput, String> {
        self.prepare_local_compactor_cancellable(config, cancellation)?.execute(input)
    }

    /// Resolves and snapshots exact dispatch inputs without consuming process authority.
    pub(crate) fn prepare_local_compactor(
        &self,
        config: &LocalProcessConfig,
    ) -> Result<PreparedLocalCompactor, String> {
        self.prepare_local_compactor_cancellable(config, &CancellationToken::new())
    }

    /// Resolves and snapshots exact dispatch inputs while observing caller cancellation.
    pub(crate) fn prepare_local_compactor_cancellable(
        &self,
        config: &LocalProcessConfig,
        cancellation: &CancellationToken,
    ) -> Result<PreparedLocalCompactor, String> {
        ensure_not_cancelled(cancellation)?;
        config.validate().map_err(|error| error.to_string())?;
        if !config.executable.is_file() || !config.model_path.is_file() {
            return Err(
                "local compactor executable or preinstalled weights unavailable".to_owned(),
            );
        }
        let executable =
            config.executable.canonicalize().map_err(|_| "resolve local compactor executable")?;
        let weights =
            config.model_path.canonicalize().map_err(|_| "resolve local compactor weights")?;
        ensure_not_cancelled(cancellation)?;
        if weights.starts_with(&self.inner.workspace_root)
            || executable.starts_with(&self.inner.workspace_root)
            || weights.starts_with(&self.inner.state_root)
            || executable.starts_with(&self.inner.state_root)
        {
            return Err(
                "installed inference inputs must be separate from editable workspace and run state"
                    .to_owned(),
            );
        }
        let after = {
            let state =
                self.inner.state.lock().map_err(|_| "local compactor command owner poisoned")?;
            state.next_ordinal
        };
        let ordinal = super::ordinal::reserve_cancellable(
            &self.inner.state_root,
            self.inner.run_id,
            after,
            cancellation,
        )
        .map_err(|error| match error {
            super::ordinal::ReserveError::Cancelled => {
                "local compactor cancelled before dispatch".to_owned()
            }
            super::ordinal::ReserveError::Storage(error) => error,
        })?;
        {
            let mut state =
                self.inner.state.lock().map_err(|_| "local compactor command owner poisoned")?;
            state.next_ordinal = state.next_ordinal.max(ordinal);
        };
        let contract = contract::command_contract(self.inner.run_id, ordinal)?;
        let ids = identity::CommandIds::new(self.inner.run_id, ordinal, &contract)?;
        let directory =
            self.inner.state_root.join("local-compactor").join(identity::action_hex(ids.action));
        std::fs::create_dir_all(&directory)
            .map_err(|_| "create local compactor scratch directory")?;
        let directory =
            directory.canonicalize().map_err(|_| "resolve local compactor scratch directory")?;
        let work = create_work_directory(&directory)?;
        let (executable, weights) = snapshot_inputs(
            &self.inner.state_root,
            &directory,
            &executable,
            &weights,
            cancellation,
        )?;
        ensure_not_cancelled(cancellation)?;
        let dispatch_mode = select_dispatch_mode(&executable.path);
        ensure_not_cancelled(cancellation)?;
        let backend = backend::open(
            config,
            &work,
            &executable.source,
            &weights.source,
            &executable.path,
            &weights.path,
            dispatch_mode.binds_snapshot(),
            cancellation,
        )?;
        ensure_not_cancelled(cancellation)?;
        let wall = config.wall_timeout_millis();
        let output_segment_bytes = u64::try_from(config.output_segment_bytes())
            .map_err(|_| "local output segment is too large")?;
        let input_bytes = u64::try_from(config.input_page_bytes())
            .map_err(|_| "local input page is too large")?;
        let resources = ProcessResourcePolicy::with_optional_limits(
            wall,
            None,
            config.memory_limit_bytes(),
            None,
            None,
            None,
            None,
            1,
        )
        .map_err(detail)?;
        let environment = EnvironmentPlan::cleared(Vec::new()).map_err(detail)?;
        let working = WorkingDirectory::open(
            &work,
            ids.workspace,
            ids.resource,
            ids.environment,
            ids.revision.workspace_generation(),
            ids.revision.workspace_revision(),
            WorkspaceAccess::Writable,
        )
        .map_err(detail)?;
        let output_chunk = u64::try_from(COPY_CHUNK_BYTES)
            .map_err(|_| "local output page is not representable")?
            .min(output_segment_bytes);
        let output = OutputPolicy::streaming(
            output_chunk,
            output_chunk,
            output_segment_bytes,
            64,
        )
        .map_err(detail)?;
        let input_chunk = u64::try_from(COPY_CHUNK_BYTES)
            .map_err(|_| "local input page is not representable")?
            .min(input_bytes);
        let stdin = StdinPolicy::streaming(input_chunk).map_err(detail)?;
        let command = CommandSpec::new(
            executable.source.as_os_str().to_os_string(),
            [
                OsString::from("--model"),
                weights.source.as_os_str().to_os_string(),
            ],
        )
        .map_err(detail)?;
        let checked = sandbox::local_compactor(
            &ids,
            &command,
            &work,
            &weights.source,
            &environment,
            IoMode::Pipes,
            stdin,
            resources,
        )?;
        ensure_not_cancelled(cancellation)?;
        let admission =
            admit_backend(&checked, backend.descriptor(), AdmissionProfile::Production)
                .map_err(detail)?;
        let plan = ExecutionPlan::new(
            ids.execution_identity(),
            command,
            working,
            environment,
            IoMode::Pipes,
            stdin,
            output,
            DeadlinePolicy::force(wall).map_err(detail)?,
            resources,
            &checked,
            &admission,
        )
        .map_err(detail)?;
        let proposal_digest = proposal_digest(
            config,
            &executable,
            &weights,
            dispatch_mode,
            &plan,
            &admission,
        )?;
        ensure_not_cancelled(cancellation)?;
        Ok(PreparedLocalCompactor {
            runtime: self.clone(),
            cancellation: cancellation.clone(),
            input_page_bytes: config.input_page_bytes().min(COPY_CHUNK_BYTES),
            directory,
            executable,
            weights,
            dispatch_mode,
            ids,
            contract,
            backend,
            checked,
            admission,
            plan,
            proposal_digest,
        })
    }
}

fn proposal_digest(
    config: &LocalProcessConfig,
    executable: &SnapshotFile,
    weights: &SnapshotFile,
    dispatch_mode: DispatchIdentityMode,
    plan: &ExecutionPlan,
    admission: &BackendAdmission,
) -> Result<Sha256Digest, String> {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, b"peritus-local-compactor-proposal/v3")?;
    hash_field(&mut hasher, b"streaming-output-segments/no-cumulative-resource-ceiling")?;
    hash_field(&mut hasher, dispatch_mode.proposal_identity())?;
    hash_snapshot(&mut hasher, executable)?;
    hash_snapshot(&mut hasher, weights)?;
    hash_field(
        &mut hasher,
        &serde_json::to_vec(&config.sandbox)
            .map_err(|_| "encode local compactor sandbox identity")?,
    )?;
    for value in [
        config.timeout_millis,
        u64::try_from(config.input_page_bytes()).map_err(|_| "encode local input policy")?,
        u64::try_from(config.output_segment_bytes()).map_err(|_| "encode local output policy")?,
        config.memory_bytes,
    ] {
        hasher.update(value.to_be_bytes());
    }
    for digest in [
        plan.digest(),
        plan.sandbox_digest(),
        admission.descriptor_digest(),
        admission.preparation_digest(),
    ] {
        hasher.update(digest.as_bytes());
    }
    Ok(Sha256Digest::new(hasher.finalize().into()))
}

fn hash_snapshot(hasher: &mut Sha256, snapshot: &SnapshotFile) -> Result<(), String> {
    hash_native_path(hasher, &snapshot.source)?;
    hash_native_path(hasher, &snapshot.object)?;
    hash_native_path(hasher, &snapshot.path)?;
    hasher.update(snapshot.bytes.to_be_bytes());
    hasher.update(snapshot.digest.as_bytes());
    Ok(())
}

fn hash_native_path(hasher: &mut Sha256, path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        hash_field(hasher, b"unix-path")?;
        hash_field(hasher, path.as_os_str().as_bytes())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        let mut bytes = Vec::new();
        for unit in path.as_os_str().encode_wide() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        hash_field(hasher, b"windows-path")?;
        hash_field(hasher, &bytes)
    }
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) -> Result<(), String> {
    let length = u64::try_from(bytes.len()).map_err(|_| "proposal field is too large")?;
    hasher.update(length.to_be_bytes());
    hasher.update(bytes);
    Ok(())
}

fn snapshot_inputs(
    state_root: &Path,
    directory: &Path,
    executable: &Path,
    weights: &Path,
    cancellation: &CancellationToken,
) -> Result<(SnapshotFile, SnapshotFile), String> {
    ensure_not_cancelled(cancellation)?;
    let object_root = fixed_snapshot_directory(
        state_root,
        "local-compactor-inputs",
        "open local inference object store",
    )?;
    let executable_objects = fixed_snapshot_directory(
        &object_root,
        "executable",
        "open local executable object store",
    )?;
    let data_objects = fixed_snapshot_directory(
        &object_root,
        "data",
        "open local weights object store",
    )?;
    let reference_root = create_snapshot_directory(
        directory,
        "input-references",
        "create local inference reference directory",
    )?;
    let executable_references = create_snapshot_directory(
        &reference_root,
        "executable",
        "create local executable reference directory",
    )?;
    let weights_references = create_snapshot_directory(
        &reference_root,
        "weights",
        "create local weights reference directory",
    )?;
    let executable_name = executable
        .file_name()
        .ok_or("local executable has no exact file name")?;
    let weights_name = weights.file_name().ok_or("local weights have no exact file name")?;
    let executable_object = snapshot_object(
        &executable_objects,
        directory,
        executable,
        true,
        cancellation,
    )?;
    let weights_object =
        snapshot_object(&data_objects, directory, weights, false, cancellation)?;
    let executable_reference = executable_references.join(executable_name);
    let weights_reference = weights_references.join(weights_name);
    materialize_snapshot_reference(
        &executable_object,
        &executable_reference,
        true,
        cancellation,
    )?;
    materialize_snapshot_reference(
        &weights_object,
        &weights_reference,
        false,
        cancellation,
    )?;
    ensure_not_cancelled(cancellation)?;
    synchronize_directory(&executable_references)?;
    synchronize_directory(&weights_references)?;
    synchronize_directory(&reference_root)?;
    synchronize_directory(directory)?;
    Ok((
        SnapshotFile {
            source: executable.to_path_buf(),
            object: executable_object.path,
            path: executable_reference,
            digest: executable_object.digest,
            bytes: executable_object.bytes,
        },
        SnapshotFile {
            source: weights.to_path_buf(),
            object: weights_object.path,
            path: weights_reference,
            digest: weights_object.digest,
            bytes: weights_object.bytes,
        },
    ))
}

struct SnapshotObject {
    path: PathBuf,
    digest: Sha256Digest,
    bytes: u64,
}

fn snapshot_object(
    object_root: &Path,
    action_directory: &Path,
    source: &Path,
    executable: bool,
    cancellation: &CancellationToken,
) -> Result<SnapshotObject, String> {
    ensure_not_cancelled(cancellation)?;
    let mut input = open_installed_input(source, executable)?;
    let (digest, bytes) = stream_digest(&mut input, cancellation)?;
    validate_open_installed_input(source, &input, executable)?;
    let object = object_root.join(ArtifactDigest::from_sha256(digest).to_hex());
    let action = action_directory
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or("local compactor action identity is unavailable")?;
    let temporary = object_root.join(format!(
        ".{}.{}.pending",
        ArtifactDigest::from_sha256(digest).to_hex(),
        action
    ));
    if recover_snapshot_path(&object, digest, bytes, cancellation)? {
        discard_snapshot_path(&temporary, digest, bytes, cancellation)?;
        return Ok(SnapshotObject { path: object, digest, bytes });
    }

    input
        .seek(SeekFrom::Start(0))
        .map_err(|_| "rewind installed local inference input")?;
    write_snapshot_candidate(
        &mut input,
        &temporary,
        digest,
        bytes,
        executable,
        cancellation,
    )?;
    publish_snapshot_candidate(
        object_root,
        &temporary,
        &object,
        digest,
        bytes,
        cancellation,
    )?;
    Ok(SnapshotObject { path: object, digest, bytes })
}

fn write_snapshot_candidate(
    input: &mut File,
    temporary: &Path,
    digest: Sha256Digest,
    bytes: u64,
    executable: bool,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    if recover_snapshot_path(temporary, digest, bytes, cancellation)? {
        return Ok(());
    }
    let outcome = write_snapshot_candidate_inner(
        input,
        temporary,
        digest,
        bytes,
        executable,
        cancellation,
    );
    if outcome.is_err() {
        let _ = remove_snapshot_entry(temporary);
    }
    outcome
}

fn write_snapshot_candidate_inner(
    input: &mut File,
    temporary: &Path,
    digest: Sha256Digest,
    bytes: u64,
    executable: bool,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    ensure_not_cancelled(cancellation)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .map_err(|_| "create local inference content candidate")?;
    if copy_and_digest(input, &mut output, cancellation)? != (digest, bytes) {
        return Err("installed local inference input changed while snapshotting".to_owned());
    }
    output
        .sync_all()
        .map_err(|_| "synchronize local inference content candidate")?;
    make_snapshot_read_only(temporary, executable)?;
    output
        .sync_all()
        .map_err(|_| "synchronize protected local inference content")?;
    drop(output);
    verify_snapshot_path(temporary, digest, bytes, cancellation)
}

fn publish_snapshot_candidate(
    object_root: &Path,
    temporary: &Path,
    object: &Path,
    digest: Sha256Digest,
    bytes: u64,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    publish_snapshot_candidate_inner(
        object_root,
        temporary,
        object,
        digest,
        bytes,
        cancellation,
    )
}

fn publish_snapshot_candidate_inner(
    object_root: &Path,
    temporary: &Path,
    object: &Path,
    digest: Sha256Digest,
    bytes: u64,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let _publication = loop {
        ensure_not_cancelled(cancellation)?;
        match SNAPSHOT_PUBLICATION.try_lock() {
            Ok(publication) => break publication,
            Err(std::sync::TryLockError::WouldBlock) => std::thread::yield_now(),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err("local inference snapshot publication owner poisoned".to_owned());
            }
        }
    };
    loop {
        ensure_not_cancelled(cancellation)?;
        if recover_snapshot_path(object, digest, bytes, cancellation)? {
            discard_snapshot_path(temporary, digest, bytes, cancellation)?;
            return Ok(());
        }
        match std::fs::rename(temporary, object) {
            Ok(()) => {
                synchronize_directory(object_root)?;
                verify_snapshot_path(object, digest, bytes, cancellation)?;
                return Ok(());
            }
            Err(_) => match std::fs::symlink_metadata(object) {
                Ok(_) => std::thread::yield_now(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err("publish local inference content object".to_owned());
                }
                Err(_) => return Err("inspect raced local inference content object".to_owned()),
            },
        }
    }
}

fn recover_snapshot_path(
    path: &Path,
    digest: Sha256Digest,
    bytes: u64,
    cancellation: &CancellationToken,
) -> Result<bool, String> {
    ensure_not_cancelled(cancellation)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => match verify_snapshot_path(path, digest, bytes, cancellation) {
            Ok(()) => Ok(true),
            Err(_) => {
                ensure_not_cancelled(cancellation)?;
                remove_snapshot_entry(path)
                    .map_err(|_| "remove invalid local inference cached content")?;
                Ok(false)
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("inspect local inference cached content".to_owned()),
    }
}

fn discard_snapshot_path(
    path: &Path,
    digest: Sha256Digest,
    bytes: u64,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    if recover_snapshot_path(path, digest, bytes, cancellation)? {
        ensure_not_cancelled(cancellation)?;
        remove_snapshot_entry(path)
            .map_err(|_| "remove redundant local inference content candidate")?;
    }
    Ok(())
}

fn remove_snapshot_entry(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() {
        std::fs::remove_dir(path)
    } else if metadata.file_type().is_file() {
        remove_snapshot_candidate(path)
    } else {
        std::fs::remove_file(path)
    }
}

#[cfg(not(windows))]
fn remove_snapshot_candidate(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path)
}

#[cfg(windows)]
fn remove_snapshot_candidate(path: &Path) -> std::io::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions)?;
    std::fs::remove_file(path)
}

#[cfg(not(windows))]
fn materialize_snapshot_reference(
    object: &SnapshotObject,
    target: &Path,
    executable: bool,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    ensure_not_cancelled(cancellation)?;
    if std::fs::hard_link(&object.path, target).is_ok() {
        return verify_snapshot_path(target, object.digest, object.bytes, cancellation);
    }
    copy_snapshot_reference(object, target, executable, cancellation)
}

#[cfg(windows)]
fn materialize_snapshot_reference(
    object: &SnapshotObject,
    target: &Path,
    executable: bool,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    copy_snapshot_reference(object, target, executable, cancellation)
}

fn copy_snapshot_reference(
    object: &SnapshotObject,
    target: &Path,
    executable: bool,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    ensure_not_cancelled(cancellation)?;
    let mut input = File::open(&object.path)
        .map_err(|_| "open local inference content object for owned reference")?;
    write_snapshot_candidate(
        &mut input,
        target,
        object.digest,
        object.bytes,
        executable,
        cancellation,
    )?;
    verify_snapshot_path(target, object.digest, object.bytes, cancellation)
}

fn open_installed_input(source: &Path, executable: bool) -> Result<File, String> {
    let input = File::open(source).map_err(|_| "open installed local inference input")?;
    validate_open_installed_input(source, &input, executable)?;
    Ok(input)
}

fn validate_open_installed_input(
    source: &Path,
    input: &File,
    executable: bool,
) -> Result<(), String> {
    let source_metadata = std::fs::symlink_metadata(source)
        .map_err(|_| "inspect installed local inference input")?;
    if !source_metadata.file_type().is_file()
        || std::fs::canonicalize(source).ok().as_deref() != Some(source)
    {
        return Err(
            "installed local inference input is not one canonical regular file".to_owned(),
        );
    }
    validate_executable_permission(&source_metadata, executable)?;
    let input_metadata = input.metadata().map_err(|_| "inspect opened local inference input")?;
    if !same_file_identity(&source_metadata, &input_metadata) {
        return Err("installed local inference input changed while opening".to_owned());
    }
    Ok(())
}

fn copy_and_digest(
    input: &mut File,
    output: &mut File,
    cancellation: &CancellationToken,
) -> Result<(Sha256Digest, u64), String> {
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; COPY_CHUNK_BYTES];
    loop {
        ensure_not_cancelled(cancellation)?;
        let count = input.read(&mut buffer).map_err(|_| "read installed local inference input")?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|_| "write immutable local inference snapshot")?;
        ensure_not_cancelled(cancellation)?;
        hasher.update(&buffer[..count]);
        bytes = bytes
            .checked_add(u64::try_from(count).map_err(|_| "snapshot byte count overflow")?)
            .ok_or("snapshot byte count overflow")?;
    }
    Ok((Sha256Digest::new(hasher.finalize().into()), bytes))
}

fn verify_snapshot(
    snapshot: &SnapshotFile,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    verify_snapshot_path(&snapshot.path, snapshot.digest, snapshot.bytes, cancellation)
}

fn verify_snapshot_path(
    path: &Path,
    digest: Sha256Digest,
    bytes: u64,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    ensure_not_cancelled(cancellation)?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "inspect immutable local inference snapshot")?;
    if !metadata.file_type().is_file() {
        return Err("local inference dispatch snapshot is not a regular file".to_owned());
    }
    let mut file = File::open(path).map_err(|_| "open local inference dispatch snapshot")?;
    let opened = file.metadata().map_err(|_| "inspect opened local inference snapshot")?;
    if !same_file_identity(&metadata, &opened) {
        return Err("local inference dispatch snapshot changed while opening".to_owned());
    }
    let (observed_digest, observed_bytes) = stream_digest(&mut file, cancellation)?;
    ensure_not_cancelled(cancellation)?;
    let current = std::fs::symlink_metadata(path)
        .map_err(|_| "reinspect immutable local inference snapshot")?;
    if !same_file_identity(&opened, &current)
        || !snapshot_is_read_only(&opened)
        || observed_digest != digest
        || observed_bytes != bytes
    {
        return Err("local inference dispatch snapshot changed after preparation".to_owned());
    }
    Ok(())
}

fn verify_dispatch_source(
    snapshot: &SnapshotFile,
    executable: bool,
    mode: DispatchIdentityMode,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    ensure_not_cancelled(cancellation)?;
    if mode.binds_snapshot() {
        return Ok(());
    }
    let mut source = open_installed_input(&snapshot.source, executable)?;
    let (digest, bytes) = stream_digest(&mut source, cancellation)?;
    validate_open_installed_input(&snapshot.source, &source, executable)?;
    if digest != snapshot.digest || bytes != snapshot.bytes {
        return Err("configured local inference dispatch input changed after preparation".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn select_dispatch_mode(executable_snapshot: &Path) -> DispatchIdentityMode {
    if snapshot_mount_allows_execution(executable_snapshot) {
        DispatchIdentityMode::ImmutableSnapshotBind
    } else {
        DispatchIdentityMode::ConfiguredSourceRevalidated
    }
}

#[cfg(not(target_os = "linux"))]
const fn select_dispatch_mode(_executable_snapshot: &Path) -> DispatchIdentityMode {
    DispatchIdentityMode::ConfiguredSourceRevalidated
}

#[cfg(target_os = "linux")]
fn snapshot_mount_allows_execution(snapshot: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt as _;

    let Ok(contents) = std::fs::read("/proc/self/mountinfo") else {
        return false;
    };
    let mut selected = None::<(usize, bool)>;
    for line in contents.split(|byte| *byte == b'\n') {
        let mut fields = line
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|field| !field.is_empty());
        let Some(encoded_mount) = fields.nth(4) else {
            continue;
        };
        let Some(options) = fields.next() else {
            continue;
        };
        let Some(mount) = decode_mountinfo_path(encoded_mount) else {
            continue;
        };
        if !mount.is_absolute() || !snapshot.starts_with(&mount) {
            continue;
        }
        let specificity = mount.as_os_str().as_bytes().len();
        let noexec = options.split(|byte| *byte == b',').any(|option| option == b"noexec");
        if selected.is_none_or(|(prior, _)| specificity >= prior) {
            selected = Some((specificity, !noexec));
        }
    }
    selected.is_some_and(|(_, executable)| executable)
}

#[cfg(target_os = "linux")]
fn decode_mountinfo_path(encoded: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;

    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if encoded[index] != b'\\' {
            decoded.push(encoded[index]);
            index += 1;
            continue;
        }
        let digits = encoded.get(index + 1..index + 4)?;
        let value = digits.iter().try_fold(0_u16, |value, digit| {
            if (b'0'..=b'7').contains(digit) {
                Some((value << 3) | u16::from(*digit - b'0'))
            } else {
                None
            }
        })?;
        decoded.push(u8::try_from(value).ok()?);
        index += 4;
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(decoded)))
}

fn stream_digest(
    file: &mut File,
    cancellation: &CancellationToken,
) -> Result<(Sha256Digest, u64), String> {
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; COPY_CHUNK_BYTES];
    loop {
        ensure_not_cancelled(cancellation)?;
        let count = file.read(&mut buffer).map_err(|_| "hash local inference snapshot")?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        bytes = bytes
            .checked_add(u64::try_from(count).map_err(|_| "snapshot byte count overflow")?)
            .ok_or("snapshot byte count overflow")?;
    }
    Ok((Sha256Digest::new(hasher.finalize().into()), bytes))
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), String> {
    if cancellation.is_cancelled() {
        Err("local compactor cancelled".to_owned())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    left.volume_serial_number().is_some()
        && left.volume_serial_number() == right.volume_serial_number()
        && left.file_index().is_some()
        && left.file_index() == right.file_index()
}

#[cfg(unix)]
fn validate_executable_permission(
    metadata: &std::fs::Metadata,
    executable: bool,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    if executable && metadata.permissions().mode() & 0o111 == 0 {
        return Err("configured local compactor is not executable".to_owned());
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_executable_permission(
    _metadata: &std::fs::Metadata,
    _executable: bool,
) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn make_snapshot_read_only(target: &Path, executable: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = if executable { 0o555 } else { 0o444 };
    std::fs::set_permissions(target, std::fs::Permissions::from_mode(mode))
        .map_err(|_| "protect local inference dispatch snapshot")
}

#[cfg(not(unix))]
fn make_snapshot_read_only(target: &Path, _executable: bool) -> Result<(), String> {
    let mut permissions = std::fs::metadata(target)
        .map_err(|_| "inspect local inference dispatch snapshot")?
        .permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(target, permissions)
        .map_err(|_| "protect local inference dispatch snapshot")
}

#[cfg(unix)]
fn snapshot_is_read_only(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o222 == 0
}

#[cfg(not(unix))]
fn snapshot_is_read_only(metadata: &std::fs::Metadata) -> bool {
    metadata.permissions().readonly()
}

fn detail(error: impl std::fmt::Display) -> String {
    format!("local compactor: {error}")
}

fn create_work_directory(directory: &Path) -> Result<PathBuf, String> {
    let work = directory.join("work");
    std::fs::create_dir(&work).map_err(|_| "create isolated compactor working directory")?;
    // C3 protects these metadata names even in scratch workspaces. Materialize them before
    // admitting descendant creation so Linux can mask them without weakening that invariant.
    for name in [".git", ".peritus", ".crosslink"] {
        std::fs::create_dir(work.join(name)).map_err(|_| "create protected compactor metadata")?;
    }
    Ok(work)
}

fn fixed_snapshot_directory(
    parent: &Path,
    name: &str,
    failure: &'static str,
) -> Result<PathBuf, String> {
    let path = parent.join(name);
    let created = match std::fs::create_dir(&path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => return Err(failure.to_owned()),
    };
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| failure.to_owned())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(failure.to_owned());
    }
    let canonical = std::fs::canonicalize(&path).map_err(|_| failure.to_owned())?;
    if canonical.parent() != Some(parent) {
        return Err(failure.to_owned());
    }
    if created {
        synchronize_directory(parent)?;
    }
    Ok(canonical)
}

fn create_snapshot_directory(
    parent: &Path,
    name: &str,
    failure: &'static str,
) -> Result<PathBuf, String> {
    let path = parent.join(name);
    std::fs::create_dir(&path).map_err(|_| failure.to_owned())?;
    let canonical = std::fs::canonicalize(&path).map_err(|_| failure.to_owned())?;
    if canonical.parent() != Some(parent) {
        return Err(failure.to_owned());
    }
    synchronize_directory(parent)?;
    Ok(canonical)
}

#[cfg(unix)]
fn synchronize_directory(directory: &Path) -> Result<(), String> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| "synchronize local inference snapshot directory".to_owned())
}

#[cfg(not(unix))]
const fn synchronize_directory(_directory: &Path) -> Result<(), String> {
    Ok(())
}
