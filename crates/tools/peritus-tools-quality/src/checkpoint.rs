//! Durable ownership checkpoint for one accepted `quality.run` execution.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use peritus_artifact_store::ArtifactStore;
use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError,
    CodecErrorKind, CodecLimits, decode_message, encode_message,
};
use peritus_patch::WorkspacePath;
use peritus_policy::AuthorityInstant;
use peritus_process::{ExecutionPlan, ProcessCursor, ProcessStore, TerminalResult};
use peritus_tool_protocol::{
    FailureCategory, PreparedToolCall, RecoveryRoute, ResponsibleSubsystem, ResultStatus,
    Retryability,
};
use peritus_tool_router::DispatchFailure;
use peritus_types::{ActionId, EventId, GateId, Generation, ProcessId, Sha256Digest};

use crate::{
    CheckDefinition, CheckRequirement, CheckSource, CleanQualitySnapshot, EnvironmentProfile,
    ExpectedSuccess, OutputParser, dispatcher::dispatch_failure,
    snapshot::SnapshotExecutionBinding,
};

const DIRECTORY: &str = "quality-checkpoints-v1";
const SUFFIX: &str = ".checkpoint";
const MAX_CHECKPOINT_BYTES: usize = 16 * 1_024 * 1_024;
const CODEC_LIMITS: CodecLimits = CodecLimits::LEGACY_V1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CursorState {
    pub(crate) cursor: ProcessCursor,
    pub(crate) next_progress: u64,
    pub(crate) progress_truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Settlement {
    pub(crate) finished_at: AuthorityInstant,
    pub(crate) progress_frontier: u64,
    pub(crate) progress_truncated: bool,
    pub(crate) parser_complete: bool,
    pub(crate) predicate_satisfied: bool,
    pub(crate) result_digest: Option<Sha256Digest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct State {
    pub(crate) last_observed_at: AuthorityInstant,
    pub(crate) committed: CursorState,
    pub(crate) pending: Option<CursorState>,
    pub(crate) launch_accepted: bool,
    pub(crate) terminal: Option<TerminalResult>,
    pub(crate) settlement: Option<Settlement>,
}

impl State {
    pub(crate) const fn initial(started_at: AuthorityInstant) -> Self {
        Self {
            last_observed_at: started_at,
            committed: CursorState {
                cursor: ProcessCursor::after(0),
                next_progress: 0,
                progress_truncated: false,
            },
            pending: None,
            launch_accepted: false,
            terminal: None,
            settlement: None,
        }
    }
}

#[derive(Clone)]
struct Binding {
    action_id: ActionId,
    prepared_bytes: Vec<u8>,
    prepared_digest: Sha256Digest,
    replay_identity: Sha256Digest,
    definition: CheckDefinition,
    snapshot: Option<SnapshotExecutionBinding>,
    plan_digest: Sha256Digest,
    process_id: ProcessId,
    artifact_root_digest: Sha256Digest,
    creating_event: EventId,
    started_at: AuthorityInstant,
}

pub(crate) struct Owner {
    directory: PathBuf,
    generation: u64,
    binding: Binding,
}

impl Owner {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        process_store: &ProcessStore,
        prepared: &PreparedToolCall,
        definition: &CheckDefinition,
        plan: &ExecutionPlan,
        snapshot: Option<&CleanQualitySnapshot>,
        artifacts: &ArtifactStore,
        creating_event: EventId,
        started_at: AuthorityInstant,
    ) -> Result<Self, DispatchFailure> {
        let snapshot = snapshot
            .map(|snapshot| snapshot.execution_binding(definition, plan))
            .transpose()
            .map_err(checkpoint_quality)?;
        if let Some(snapshot) = &snapshot {
            snapshot
                .validate_write_roots(process_store.root(), artifacts.root())
                .map_err(checkpoint_quality)?;
        } else if definition.working_directory().is_some() {
            return Err(checkpoint_failure(
                "declared quality subdirectory has no durable snapshot binding",
            ));
        }
        let directory = checkpoint_directory(process_store.root(), prepared.call().action_id());
        create_directory(&directory)?;
        let binding = Binding {
            action_id: prepared.call().action_id(),
            prepared_bytes: prepared.canonical_bytes(),
            prepared_digest: prepared.prepared_digest(),
            replay_identity: prepared.replay_identity().digest(),
            definition: definition.clone(),
            snapshot,
            plan_digest: plan.digest(),
            process_id: plan.identity().process_id(),
            artifact_root_digest: path_digest(artifacts.root()),
            creating_event,
            started_at,
        };
        let mut owner = Self { directory, generation: 0, binding };
        owner.save(&State::initial(started_at))?;
        Ok(owner)
    }

    pub(crate) fn recover(
        process_store: &ProcessStore,
        prepared: &PreparedToolCall,
        artifacts: &ArtifactStore,
    ) -> Result<(Self, State), DispatchFailure> {
        let directory = checkpoint_directory(process_store.root(), prepared.call().action_id());
        let candidate = latest_candidate(&directory)?;
        let bytes = read_candidate(&candidate.path)?;
        if peritus_codec::sha256(&bytes) != candidate.digest {
            return Err(checkpoint_failure("quality checkpoint content digest is invalid"));
        }
        let frame = match decode_message::<CheckpointFrame>(&bytes, CODEC_LIMITS) {
            Ok(frame) => frame,
            Err(current_error) => {
                match decode_message::<LegacyCheckpointFrameV2>(&bytes, CODEC_LIMITS) {
                    Ok(legacy) => legacy.upgrade(),
                    Err(_) => {
                        match decode_message::<LegacyCheckpointFrame>(&bytes, CODEC_LIMITS) {
                            Ok(legacy) => legacy.upgrade(),
                            Err(_) => {
                                return Err(checkpoint_codec(
                                    "decode quality checkpoint",
                                    current_error,
                                ));
                            }
                        }
                    }
                }
            }
        };
        if frame.generation != candidate.generation
            || frame.action_id != prepared.call().action_id()
            || frame.prepared_bytes != prepared.canonical_bytes()
            || frame.prepared_digest != prepared.prepared_digest()
            || frame.replay_identity != prepared.replay_identity().digest()
            || frame.artifact_root_digest != path_digest(artifacts.root())
        {
            return Err(checkpoint_failure(
                "quality checkpoint differs from its file, prepared call, or artifact store",
            ));
        }
        let definition = frame.definition.into_definition()?;
        let snapshot = frame
            .snapshot
            .map(|snapshot| snapshot.into_binding(&definition))
            .transpose()?;
        if let Some(snapshot) = &snapshot {
            snapshot
                .validate_write_roots(process_store.root(), artifacts.root())
                .map_err(checkpoint_quality)?;
        }
        let binding = Binding {
            action_id: frame.action_id,
            prepared_bytes: frame.prepared_bytes,
            prepared_digest: frame.prepared_digest,
            replay_identity: frame.replay_identity,
            definition,
            snapshot,
            plan_digest: frame.plan_digest,
            process_id: frame.process_id,
            artifact_root_digest: frame.artifact_root_digest,
            creating_event: frame.creating_event,
            started_at: frame.started_at,
        };
        let owner = Self { directory, generation: frame.generation, binding };
        Ok((owner, frame.state))
    }

    pub(crate) fn save(&mut self, state: &State) -> Result<(), DispatchFailure> {
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            checkpoint_failure("quality checkpoint generation overflowed")
        })?;
        let frame = CheckpointFrame::new(generation, &self.binding, state.clone());
        let bytes = encode_message(&frame, CODEC_LIMITS)
            .map_err(|error| checkpoint_codec("encode quality checkpoint", error))?;
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(checkpoint_failure("quality checkpoint exceeds its durable byte bound"));
        }
        let digest = peritus_codec::sha256(&bytes);
        let final_path = self.directory.join(file_name(generation, digest));
        if let Some(retained_candidate) = candidate_at_generation(&self.directory, generation)? {
            let retained = read_candidate(&retained_candidate.path)?;
            if peritus_codec::sha256(&retained) != retained_candidate.digest {
                return Err(checkpoint_failure("quality checkpoint content digest is invalid"));
            }
            sync_directory(&self.directory)?;
            self.generation = generation;
            remove_older_candidates(&self.directory, generation);
            return if retained == bytes {
                Ok(())
            } else {
                Err(checkpoint_failure(
                    "an ambiguous checkpoint commit resolved to its earlier durable state",
                ))
            };
        }
        let temporary = self.directory.join(format!(".{generation:020}.pending"));
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(checkpoint_failure("stale quality checkpoint cannot be removed")),
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| checkpoint_failure("quality checkpoint temporary file cannot be created"))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| checkpoint_failure("quality checkpoint cannot be durably written"))?;
        drop(file);
        fs::rename(&temporary, &final_path)
            .map_err(|_| checkpoint_failure("quality checkpoint cannot be atomically published"))?;
        sync_directory(&self.directory)?;
        self.generation = generation;
        remove_older_candidates(&self.directory, generation);
        Ok(())
    }

    pub(crate) const fn definition(&self) -> &CheckDefinition {
        &self.binding.definition
    }

    pub(crate) const fn plan_digest(&self) -> Sha256Digest {
        self.binding.plan_digest
    }

    pub(crate) const fn process_id(&self) -> ProcessId {
        self.binding.process_id
    }

    pub(crate) const fn creating_event(&self) -> EventId {
        self.binding.creating_event
    }

    pub(crate) const fn started_at(&self) -> AuthorityInstant {
        self.binding.started_at
    }

    pub(crate) fn validate_plan(&self, plan: &ExecutionPlan) -> Result<(), DispatchFailure> {
        if let Some(snapshot) = &self.binding.snapshot {
            snapshot
                .validate_plan(plan, &self.binding.definition)
                .map_err(checkpoint_quality)?;
        }
        Ok(())
    }
}

struct CheckpointFrame {
    generation: u64,
    action_id: ActionId,
    prepared_bytes: Vec<u8>,
    prepared_digest: Sha256Digest,
    replay_identity: Sha256Digest,
    definition: DefinitionFrame,
    snapshot: Option<SnapshotFrame>,
    plan_digest: Sha256Digest,
    process_id: ProcessId,
    artifact_root_digest: Sha256Digest,
    creating_event: EventId,
    started_at: AuthorityInstant,
    state: State,
}

impl CheckpointFrame {
    fn new(generation: u64, binding: &Binding, state: State) -> Self {
        Self {
            generation,
            action_id: binding.action_id,
            prepared_bytes: binding.prepared_bytes.clone(),
            prepared_digest: binding.prepared_digest,
            replay_identity: binding.replay_identity,
            definition: DefinitionFrame::from_definition(&binding.definition),
            snapshot: binding.snapshot.as_ref().map(SnapshotFrame::from_binding),
            plan_digest: binding.plan_digest,
            process_id: binding.process_id,
            artifact_root_digest: binding.artifact_root_digest,
            creating_event: binding.creating_event,
            started_at: binding.started_at,
            state,
        }
    }
}

impl CanonicalEncode for CheckpointFrame {
    const FAMILY: u16 = 0xc451;
    const SCHEMA_VERSION: u16 = 3;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_u64(self.generation)?;
        writer.write_fixed(self.action_id.as_bytes())?;
        writer.write_bytes(&self.prepared_bytes)?;
        writer.write_fixed(self.prepared_digest.as_bytes())?;
        writer.write_fixed(self.replay_identity.as_bytes())?;
        self.definition.encode(writer)?;
        writer.write_option_tag(self.snapshot.is_some())?;
        if let Some(snapshot) = &self.snapshot {
            snapshot.encode(writer)?;
        }
        writer.write_fixed(self.plan_digest.as_bytes())?;
        writer.write_fixed(self.process_id.as_bytes())?;
        writer.write_fixed(self.artifact_root_digest.as_bytes())?;
        writer.write_fixed(self.creating_event.as_bytes())?;
        encode_instant(writer, self.started_at)?;
        encode_state(writer, &self.state)
    }
}

impl CanonicalDecode for CheckpointFrame {
    const FAMILY: u16 = 0xc451;
    const SCHEMA_VERSION: u16 = 3;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let generation = reader.read_u64()?;
        if generation == 0 {
            return Err(domain(reader));
        }
        let action_id = ActionId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let prepared_bytes = reader.read_bytes_owned()?;
        if prepared_bytes.is_empty() {
            return Err(domain(reader));
        }
        let prepared_digest = Sha256Digest::new(reader.read_fixed()?);
        let replay_identity = Sha256Digest::new(reader.read_fixed()?);
        let definition = DefinitionFrame::decode(reader)?;
        let snapshot = if reader.read_option_tag()? {
            Some(SnapshotFrame::decode(reader)?)
        } else {
            None
        };
        let plan_digest = Sha256Digest::new(reader.read_fixed()?);
        let process_id = ProcessId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let artifact_root_digest = Sha256Digest::new(reader.read_fixed()?);
        let creating_event = EventId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let started_at = decode_instant(reader)?;
        let state = decode_state(reader)?;
        if state.last_observed_at.epoch() != started_at.epoch()
            || state.last_observed_at.tick_millis() < started_at.tick_millis()
            || state.pending.is_some_and(|pending| {
                pending.next_progress < state.committed.next_progress
            })
            || state.terminal.is_some() && !state.launch_accepted
            || state.settlement.is_some_and(|settlement| {
                settlement.finished_at.epoch() != started_at.epoch()
                    || settlement.finished_at.tick_millis() < started_at.tick_millis()
                    || settlement.progress_frontier < state.committed.next_progress
                    || state.terminal.is_none()
                    || state.pending.is_some_and(|pending| {
                        pending.next_progress > settlement.progress_frontier
                    })
            })
        {
            return Err(domain(reader));
        }
        Ok(Self {
            generation,
            action_id,
            prepared_bytes,
            prepared_digest,
            replay_identity,
            definition,
            snapshot,
            plan_digest,
            process_id,
            artifact_root_digest,
            creating_event,
            started_at,
            state,
        })
    }
}

struct SnapshotFrame {
    snapshot_digest: Sha256Digest,
    root: PathBuf,
    working_directory: PathBuf,
}

impl SnapshotFrame {
    fn from_binding(binding: &SnapshotExecutionBinding) -> Self {
        Self {
            snapshot_digest: binding.snapshot_digest(),
            root: binding.root().to_owned(),
            working_directory: binding.working_directory().to_owned(),
        }
    }

    fn into_binding(
        self,
        definition: &CheckDefinition,
    ) -> Result<SnapshotExecutionBinding, DispatchFailure> {
        SnapshotExecutionBinding::restore(
            self.snapshot_digest,
            self.root,
            self.working_directory,
            definition,
        )
        .map_err(checkpoint_quality)
    }

    fn encode(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let root = self
            .root
            .to_str()
            .ok_or_else(|| CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len()))?;
        let working_directory = self.working_directory.to_str().ok_or_else(|| {
            CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len())
        })?;
        writer.write_fixed(self.snapshot_digest.as_bytes())?;
        writer.write_str(root)?;
        writer.write_str(working_directory)
    }

    fn decode(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            snapshot_digest: Sha256Digest::new(reader.read_fixed()?),
            root: PathBuf::from(read_owned(reader)?),
            working_directory: PathBuf::from(read_owned(reader)?),
        })
    }
}

struct LegacyCheckpointFrameV2 {
    generation: u64,
    action_id: ActionId,
    prepared_bytes: Vec<u8>,
    prepared_digest: Sha256Digest,
    replay_identity: Sha256Digest,
    definition: DefinitionFrame,
    plan_digest: Sha256Digest,
    process_id: ProcessId,
    artifact_root_digest: Sha256Digest,
    creating_event: EventId,
    started_at: AuthorityInstant,
    state: State,
}

impl LegacyCheckpointFrameV2 {
    fn upgrade(self) -> CheckpointFrame {
        CheckpointFrame {
            generation: self.generation,
            action_id: self.action_id,
            prepared_bytes: self.prepared_bytes,
            prepared_digest: self.prepared_digest,
            replay_identity: self.replay_identity,
            definition: self.definition,
            snapshot: None,
            plan_digest: self.plan_digest,
            process_id: self.process_id,
            artifact_root_digest: self.artifact_root_digest,
            creating_event: self.creating_event,
            started_at: self.started_at,
            state: self.state,
        }
    }
}

impl CanonicalDecode for LegacyCheckpointFrameV2 {
    const FAMILY: u16 = 0xc451;
    const SCHEMA_VERSION: u16 = 2;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let generation = reader.read_u64()?;
        if generation == 0 {
            return Err(domain(reader));
        }
        let action_id = ActionId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let prepared_bytes = reader.read_bytes_owned()?;
        if prepared_bytes.is_empty() {
            return Err(domain(reader));
        }
        let prepared_digest = Sha256Digest::new(reader.read_fixed()?);
        let replay_identity = Sha256Digest::new(reader.read_fixed()?);
        let definition = DefinitionFrame::decode(reader)?;
        let plan_digest = Sha256Digest::new(reader.read_fixed()?);
        let process_id = ProcessId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let artifact_root_digest = Sha256Digest::new(reader.read_fixed()?);
        let creating_event = EventId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let started_at = decode_instant(reader)?;
        let state = decode_state(reader)?;
        if state.last_observed_at.epoch() != started_at.epoch()
            || state.last_observed_at.tick_millis() < started_at.tick_millis()
            || state.pending.is_some_and(|pending| {
                pending.next_progress < state.committed.next_progress
            })
            || state.terminal.is_some() && !state.launch_accepted
            || state.settlement.is_some_and(|settlement| {
                settlement.finished_at.epoch() != started_at.epoch()
                    || settlement.finished_at.tick_millis() < started_at.tick_millis()
                    || settlement.progress_frontier < state.committed.next_progress
                    || state.terminal.is_none()
                    || state.pending.is_some_and(|pending| {
                        pending.next_progress > settlement.progress_frontier
                    })
            })
        {
            return Err(domain(reader));
        }
        Ok(Self {
            generation,
            action_id,
            prepared_bytes,
            prepared_digest,
            replay_identity,
            definition,
            plan_digest,
            process_id,
            artifact_root_digest,
            creating_event,
            started_at,
            state,
        })
    }
}

struct LegacyCheckpointFrame {
    generation: u64,
    action_id: ActionId,
    prepared_bytes: Vec<u8>,
    prepared_digest: Sha256Digest,
    replay_identity: Sha256Digest,
    definition: DefinitionFrame,
    plan_digest: Sha256Digest,
    process_id: ProcessId,
    artifact_root_digest: Sha256Digest,
    creating_event: EventId,
    started_at: AuthorityInstant,
    state: LegacyState,
}

impl LegacyCheckpointFrame {
    fn upgrade(self) -> CheckpointFrame {
        let launch_accepted = self.state.committed.next_progress != 0
            || self.state.pending.is_some()
            || self.state.settlement.is_some();
        CheckpointFrame {
            generation: self.generation,
            action_id: self.action_id,
            prepared_bytes: self.prepared_bytes,
            prepared_digest: self.prepared_digest,
            replay_identity: self.replay_identity,
            definition: self.definition,
            snapshot: None,
            plan_digest: self.plan_digest,
            process_id: self.process_id,
            artifact_root_digest: self.artifact_root_digest,
            creating_event: self.creating_event,
            started_at: self.started_at,
            state: State {
                last_observed_at: self.state.last_observed_at,
                committed: self.state.committed,
                pending: self.state.pending,
                launch_accepted,
                terminal: None,
                settlement: self.state.settlement,
            },
        }
    }
}

impl CanonicalDecode for LegacyCheckpointFrame {
    const FAMILY: u16 = 0xc451;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let generation = reader.read_u64()?;
        if generation == 0 {
            return Err(domain(reader));
        }
        let action_id = ActionId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let prepared_bytes = reader.read_bytes_owned()?;
        if prepared_bytes.is_empty() {
            return Err(domain(reader));
        }
        let prepared_digest = Sha256Digest::new(reader.read_fixed()?);
        let replay_identity = Sha256Digest::new(reader.read_fixed()?);
        let definition = DefinitionFrame::decode(reader)?;
        let plan_digest = Sha256Digest::new(reader.read_fixed()?);
        let process_id = ProcessId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let artifact_root_digest = Sha256Digest::new(reader.read_fixed()?);
        let creating_event = EventId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let started_at = decode_instant(reader)?;
        let state = decode_legacy_state(reader)?;
        if state.last_observed_at.epoch() != started_at.epoch()
            || state.last_observed_at.tick_millis() < started_at.tick_millis()
            || state.pending.is_some_and(|pending| {
                pending.next_progress < state.committed.next_progress
            })
            || state.settlement.is_some_and(|settlement| {
                settlement.finished_at.epoch() != started_at.epoch()
                    || settlement.finished_at.tick_millis() < started_at.tick_millis()
                    || settlement.progress_frontier < state.committed.next_progress
                    || state.pending.is_some_and(|pending| {
                        pending.next_progress > settlement.progress_frontier
                    })
            })
        {
            return Err(domain(reader));
        }
        Ok(Self {
            generation,
            action_id,
            prepared_bytes,
            prepared_digest,
            replay_identity,
            definition,
            plan_digest,
            process_id,
            artifact_root_digest,
            creating_event,
            started_at,
            state,
        })
    }
}

struct LegacyState {
    last_observed_at: AuthorityInstant,
    committed: CursorState,
    pending: Option<CursorState>,
    settlement: Option<Settlement>,
}

struct DefinitionFrame {
    gate_name: String,
    gate_id: GateId,
    source: CheckSource,
    requirement: CheckRequirement,
    executable: String,
    arguments: Vec<String>,
    working_directory: Option<String>,
    environment_profile: String,
    timeout_millis: Option<u64>,
    output_limit: Option<u64>,
    parser: OutputParser,
    expected_success: ExpectedSuccess,
}

impl DefinitionFrame {
    fn from_definition(definition: &CheckDefinition) -> Self {
        Self {
            gate_name: definition.gate_name().to_owned(),
            gate_id: definition.gate_id(),
            source: definition.source().clone(),
            requirement: definition.requirement(),
            executable: definition.executable().to_owned(),
            arguments: definition.arguments().to_vec(),
            working_directory: definition
                .working_directory()
                .map(|path| path.as_str().to_owned()),
            environment_profile: definition.environment_profile().as_str().to_owned(),
            timeout_millis: definition.timeout_millis(),
            output_limit: definition.output_limit(),
            parser: definition.parser(),
            expected_success: definition.expected_success(),
        }
    }

    fn into_definition(self) -> Result<CheckDefinition, DispatchFailure> {
        let working_directory = self
            .working_directory
            .map(WorkspacePath::new)
            .transpose()
            .map_err(|_| checkpoint_failure("quality checkpoint working directory is invalid"))?;
        let environment_profile = EnvironmentProfile::new(self.environment_profile)
            .map_err(|_| checkpoint_failure("quality checkpoint environment profile is invalid"))?;
        CheckDefinition::with_optional_limits(
            self.gate_name,
            self.gate_id,
            self.source,
            self.requirement,
            self.executable,
            self.arguments,
            working_directory,
            environment_profile,
            self.timeout_millis,
            self.output_limit,
            self.parser,
            self.expected_success,
        )
        .map_err(|_| checkpoint_failure("quality checkpoint definition is invalid"))
    }

    fn encode(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_str(&self.gate_name)?;
        writer.write_fixed(self.gate_id.as_bytes())?;
        match &self.source {
            CheckSource::Explicit(label) => {
                writer.write_u8(1)?;
                writer.write_str(label)?;
            }
            CheckSource::CargoManifest => writer.write_u8(2)?,
            CheckSource::JustfileRecipe(label) => {
                writer.write_u8(3)?;
                writer.write_str(label)?;
            }
        }
        writer.write_u8(match self.requirement {
            CheckRequirement::Required => 1,
            CheckRequirement::Optional => 2,
            CheckRequirement::Discovered => 3,
        })?;
        writer.write_str(&self.executable)?;
        writer.write_collection_len(self.arguments.len())?;
        for argument in &self.arguments {
            writer.write_str(argument)?;
        }
        writer.write_option_tag(self.working_directory.is_some())?;
        if let Some(path) = &self.working_directory {
            writer.write_str(path)?;
        }
        writer.write_str(&self.environment_profile)?;
        encode_optional_u64(writer, self.timeout_millis)?;
        encode_optional_u64(writer, self.output_limit)?;
        encode_parser(writer, self.parser)?;
        match self.expected_success {
            ExpectedSuccess::ExitCode(code) => {
                writer.write_u8(1)?;
                writer.write_fixed(&code.to_be_bytes())?;
            }
        }
        Ok(())
    }

    fn decode(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let gate_name = read_owned(reader)?;
        let gate_id = GateId::new(reader.read_fixed()?).map_err(|_| domain(reader))?;
        let source = match reader.read_u8()? {
            1 => CheckSource::Explicit(read_owned(reader)?),
            2 => CheckSource::CargoManifest,
            3 => CheckSource::JustfileRecipe(read_owned(reader)?),
            _ => return Err(unknown_tag(reader)),
        };
        let requirement = match reader.read_u8()? {
            1 => CheckRequirement::Required,
            2 => CheckRequirement::Optional,
            3 => CheckRequirement::Discovered,
            _ => return Err(unknown_tag(reader)),
        };
        let executable = read_owned(reader)?;
        let count = reader.read_collection_len(4)?;
        let mut arguments = reader.reserve_collection(count)?;
        for _ in 0..count {
            arguments.push(read_owned(reader)?);
        }
        let working_directory = if reader.read_option_tag()? {
            Some(read_owned(reader)?)
        } else {
            None
        };
        let environment_profile = read_owned(reader)?;
        let timeout_millis = decode_optional_u64(reader)?;
        let output_limit = decode_optional_u64(reader)?;
        let parser = decode_parser(reader)?;
        let expected_success = match reader.read_u8()? {
            1 => ExpectedSuccess::ExitCode(i32::from_be_bytes(reader.read_fixed()?)),
            _ => return Err(unknown_tag(reader)),
        };
        Ok(Self {
            gate_name,
            gate_id,
            source,
            requirement,
            executable,
            arguments,
            working_directory,
            environment_profile,
            timeout_millis,
            output_limit,
            parser,
            expected_success,
        })
    }
}

fn encode_state(writer: &mut CanonicalWriter, state: &State) -> Result<(), CodecError> {
    encode_instant(writer, state.last_observed_at)?;
    encode_cursor_state(writer, state.committed)?;
    writer.write_option_tag(state.pending.is_some())?;
    if let Some(pending) = state.pending {
        encode_cursor_state(writer, pending)?;
    }
    writer.write_bool(state.launch_accepted)?;
    writer.write_option_tag(state.terminal.is_some())?;
    if let Some(terminal) = &state.terminal {
        let bytes = terminal
            .encode_retained_owner()
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, writer.len()))?;
        writer.write_bytes(&bytes)?;
    }
    writer.write_option_tag(state.settlement.is_some())?;
    if let Some(settlement) = state.settlement {
        encode_instant(writer, settlement.finished_at)?;
        writer.write_u64(settlement.progress_frontier)?;
        writer.write_bool(settlement.progress_truncated)?;
        writer.write_bool(settlement.parser_complete)?;
        writer.write_bool(settlement.predicate_satisfied)?;
        writer.write_option_tag(settlement.result_digest.is_some())?;
        if let Some(digest) = settlement.result_digest {
            writer.write_fixed(digest.as_bytes())?;
        }
    }
    Ok(())
}

fn decode_state(reader: &mut CanonicalReader<'_>) -> Result<State, CodecError> {
    let last_observed_at = decode_instant(reader)?;
    let committed = decode_cursor_state(reader)?;
    let pending = if reader.read_option_tag()? {
        Some(decode_cursor_state(reader)?)
    } else {
        None
    };
    let launch_accepted = reader.read_bool()?;
    let terminal = if reader.read_option_tag()? {
        Some(
            TerminalResult::decode_retained_owner(reader.read_bytes()?)
                .map_err(|_| domain(reader))?,
        )
    } else {
        None
    };
    let settlement = if reader.read_option_tag()? {
        Some(Settlement {
            finished_at: decode_instant(reader)?,
            progress_frontier: reader.read_u64()?,
            progress_truncated: reader.read_bool()?,
            parser_complete: reader.read_bool()?,
            predicate_satisfied: reader.read_bool()?,
            result_digest: if reader.read_option_tag()? {
                Some(Sha256Digest::new(reader.read_fixed()?))
            } else {
                None
            },
        })
    } else {
        None
    };
    Ok(State {
        last_observed_at,
        committed,
        pending,
        launch_accepted,
        terminal,
        settlement,
    })
}

fn decode_legacy_state(reader: &mut CanonicalReader<'_>) -> Result<LegacyState, CodecError> {
    let last_observed_at = decode_instant(reader)?;
    let committed = decode_cursor_state(reader)?;
    let pending = if reader.read_option_tag()? {
        Some(decode_cursor_state(reader)?)
    } else {
        None
    };
    let settlement = if reader.read_option_tag()? {
        Some(Settlement {
            finished_at: decode_instant(reader)?,
            progress_frontier: reader.read_u64()?,
            progress_truncated: reader.read_bool()?,
            parser_complete: reader.read_bool()?,
            predicate_satisfied: reader.read_bool()?,
            result_digest: None,
        })
    } else {
        None
    };
    Ok(LegacyState { last_observed_at, committed, pending, settlement })
}

fn encode_cursor_state(
    writer: &mut CanonicalWriter,
    state: CursorState,
) -> Result<(), CodecError> {
    writer.write_bytes(&state.cursor.encode_retained_owner())?;
    writer.write_u64(state.next_progress)?;
    writer.write_bool(state.progress_truncated)
}

fn decode_cursor_state(reader: &mut CanonicalReader<'_>) -> Result<CursorState, CodecError> {
    let cursor = ProcessCursor::decode_retained_owner(reader.read_bytes()?)
        .map_err(|_| domain(reader))?;
    Ok(CursorState {
        cursor,
        next_progress: reader.read_u64()?,
        progress_truncated: reader.read_bool()?,
    })
}

fn encode_instant(
    writer: &mut CanonicalWriter,
    instant: AuthorityInstant,
) -> Result<(), CodecError> {
    writer.write_u64(instant.epoch().get())?;
    writer.write_u64(instant.tick_millis())
}

fn decode_instant(reader: &mut CanonicalReader<'_>) -> Result<AuthorityInstant, CodecError> {
    let epoch = Generation::new(reader.read_u64()?).map_err(|_| domain(reader))?;
    Ok(AuthorityInstant::new(epoch, reader.read_u64()?))
}

fn encode_optional_u64(
    writer: &mut CanonicalWriter,
    value: Option<u64>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        writer.write_u64(value)?;
    }
    Ok(())
}

fn decode_optional_u64(reader: &mut CanonicalReader<'_>) -> Result<Option<u64>, CodecError> {
    if reader.read_option_tag()? { Ok(Some(reader.read_u64()?)) } else { Ok(None) }
}

fn encode_parser(writer: &mut CanonicalWriter, parser: OutputParser) -> Result<(), CodecError> {
    match parser {
        OutputParser::None => writer.write_u8(1),
        OutputParser::Utf8 { maximum_bytes } => {
            writer.write_u8(2)?;
            writer.write_u32(maximum_bytes)
        }
        OutputParser::Json { maximum_bytes } => {
            writer.write_u8(3)?;
            writer.write_u32(maximum_bytes)
        }
        OutputParser::JsonSuccess { maximum_bytes } => {
            writer.write_u8(4)?;
            writer.write_u32(maximum_bytes)
        }
    }
}

fn decode_parser(reader: &mut CanonicalReader<'_>) -> Result<OutputParser, CodecError> {
    match reader.read_u8()? {
        1 => Ok(OutputParser::None),
        2 => Ok(OutputParser::Utf8 { maximum_bytes: reader.read_u32()? }),
        3 => Ok(OutputParser::Json { maximum_bytes: reader.read_u32()? }),
        4 => Ok(OutputParser::JsonSuccess { maximum_bytes: reader.read_u32()? }),
        _ => Err(unknown_tag(reader)),
    }
}

struct Candidate {
    path: PathBuf,
    generation: u64,
    digest: Sha256Digest,
}

fn latest_candidate(directory: &Path) -> Result<Candidate, DispatchFailure> {
    let entries = fs::read_dir(directory)
        .map_err(|_| checkpoint_failure("quality checkpoint directory is unavailable"))?;
    let mut latest: Option<Candidate> = None;
    for entry in entries {
        let entry = entry
            .map_err(|_| checkpoint_failure("quality checkpoint directory cannot be read"))?;
        let metadata = entry
            .metadata()
            .map_err(|_| checkpoint_failure("quality checkpoint metadata cannot be read"))?;
        if !metadata.is_file() {
            continue;
        }
        let Some((generation, digest)) = parse_file_name(&entry.file_name()) else {
            continue;
        };
        match &latest {
            Some(current) if current.generation > generation => {}
            Some(current) if current.generation == generation && current.digest != digest => {
                return Err(checkpoint_failure(
                    "quality checkpoint generation has conflicting durable values",
                ));
            }
            Some(current) if current.generation == generation => {}
            _ => latest = Some(Candidate { path: entry.path(), generation, digest }),
        }
    }
    latest.ok_or_else(|| checkpoint_failure("quality execution has no durable checkpoint"))
}

fn candidate_at_generation(
    directory: &Path,
    expected: u64,
) -> Result<Option<Candidate>, DispatchFailure> {
    let entries = fs::read_dir(directory)
        .map_err(|_| checkpoint_failure("quality checkpoint directory is unavailable"))?;
    let mut candidate: Option<Candidate> = None;
    for entry in entries {
        let entry = entry
            .map_err(|_| checkpoint_failure("quality checkpoint directory cannot be read"))?;
        let Some((generation, digest)) = parse_file_name(&entry.file_name()) else {
            continue;
        };
        if generation != expected {
            continue;
        }
        if candidate.as_ref().is_some_and(|current| current.digest != digest) {
            return Err(checkpoint_failure(
                "quality checkpoint generation has conflicting durable values",
            ));
        }
        candidate = Some(Candidate { path: entry.path(), generation, digest });
    }
    Ok(candidate)
}

fn read_candidate(path: &Path) -> Result<Vec<u8>, DispatchFailure> {
    let length = fs::metadata(path)
        .map_err(|_| checkpoint_failure("quality checkpoint metadata is unavailable"))?
        .len();
    if length == 0 || length > MAX_CHECKPOINT_BYTES as u64 {
        return Err(checkpoint_failure("quality checkpoint has an invalid byte length"));
    }
    let capacity = usize::try_from(length)
        .map_err(|_| checkpoint_failure("quality checkpoint exceeds native addressability"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| checkpoint_failure("quality checkpoint cannot be allocated"))?;
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| checkpoint_failure("quality checkpoint cannot be read"))?;
    if bytes.len() != capacity {
        return Err(checkpoint_failure("quality checkpoint changed while it was read"));
    }
    Ok(bytes)
}

fn create_directory(directory: &Path) -> Result<(), DispatchFailure> {
    fs::create_dir_all(directory)
        .map_err(|_| checkpoint_failure("quality checkpoint directory cannot be created"))?;
    if !fs::symlink_metadata(directory)
        .map_err(|_| checkpoint_failure("quality checkpoint directory cannot be inspected"))?
        .file_type()
        .is_dir()
    {
        return Err(checkpoint_failure("quality checkpoint path is not a directory"));
    }
    sync_directory(directory)?;
    if let Some(action_parent) = directory.parent() {
        sync_directory(action_parent)?;
        if let Some(registry_root) = action_parent.parent() {
            sync_directory(registry_root)?;
        }
    }
    Ok(())
}

fn checkpoint_directory(root: &Path, action_id: ActionId) -> PathBuf {
    root.join(DIRECTORY).join(hex(action_id.as_bytes()))
}

fn file_name(generation: u64, digest: Sha256Digest) -> String {
    format!("{generation:020}-{}{SUFFIX}", hex(digest.as_bytes()))
}

fn parse_file_name(value: &std::ffi::OsStr) -> Option<(u64, Sha256Digest)> {
    let value = value.to_str()?;
    let value = value.strip_suffix(SUFFIX)?;
    let (generation, digest) = value.split_once('-')?;
    if generation.len() != 20 || digest.len() != 64 {
        return None;
    }
    Some((generation.parse().ok()?, Sha256Digest::new(decode_hex(digest)?)))
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N.checked_mul(2)? {
        return None;
    }
    let mut bytes = [0; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(bytes)
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        value.push(char::from(DIGITS[usize::from(byte >> 4)]));
        value.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    value
}

fn path_digest(path: &Path) -> Sha256Digest {
    peritus_codec::sha256(path.as_os_str().as_encoded_bytes())
}

fn remove_older_candidates(directory: &Path, retained_generation: u64) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Some((generation, _)) = parse_file_name(&entry.file_name()) else {
            continue;
        };
        if generation < retained_generation {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn read_owned(reader: &mut CanonicalReader<'_>) -> Result<String, CodecError> {
    let offset = reader.offset();
    let value = reader.read_str()?;
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| CodecError::at(CodecErrorKind::AllocationUnavailable, offset))?;
    owned.push_str(value);
    Ok(owned)
}

fn domain(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}

fn unknown_tag(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::UnknownTag, reader.offset().saturating_sub(1))
}

fn checkpoint_codec(operation: &str, error: CodecError) -> DispatchFailure {
    checkpoint_error("quality-checkpoint-codec", &format!("{operation}: {error}"))
}

fn checkpoint_quality(error: crate::QualityError) -> DispatchFailure {
    checkpoint_error("quality-checkpoint-binding", error.detail())
}

fn checkpoint_failure(detail: &'static str) -> DispatchFailure {
    checkpoint_error("quality-checkpoint", detail)
}

fn checkpoint_error(code: &str, detail: &str) -> DispatchFailure {
    dispatch_failure(
        ResultStatus::Indeterminate,
        FailureCategory::Indeterminate,
        code,
        ResponsibleSubsystem::Tool,
        Retryability::AfterRecovery,
        RecoveryRoute::ReconcileProcess,
        detail,
    )
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), DispatchFailure> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| checkpoint_failure("quality checkpoint directory cannot be synchronized"))
}

#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "directory synchronization is not available on every supported host"
)]
const fn sync_directory(_path: &Path) -> Result<(), DispatchFailure> {
    Ok(())
}
