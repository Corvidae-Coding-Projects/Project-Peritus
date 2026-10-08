//! Validate one logical proposal before atomically publishing its complete successor.

use super::super::{memory::environment, record::PendingState};
use super::{LocalMemory, error, hex, rejected, sequence, source_handle};
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::{
    ContextLimits, ContextNodeId, bind_context_content,
    working::{
        ObservationId, WorkingDelta, WorkingEntry, WorkingEntryKind, WorkingEntryStatus,
        WorkingEnvironment, WorkingEvent, WorkingFileDigest, WorkingLimits, WorkingLinks,
        WorkingState,
        WorkingValidity, apply_working_delta, apply_working_event,
    },
};
use peritus_model_protocol::CompletedToolCall;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};

mod parser;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::local_context) struct Update {
    base_revision: u64,
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    id: String,
    kind: Kind,
    text: String,
    supports: Vec<String>,
    contradicts: Vec<String>,
    depends_on: Vec<String>,
    status: Status,
    validity: Validity,
    files: Vec<String>,
    supersedes: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Observation,
    Assertion,
    Hypothesis,
    Decision,
    FailedApproach,
    Plan,
    Glossary,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Open,
    Contradicted,
    Resolved,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Validity {
    Candidate,
    Files,
    Conversation,
    Task,
}

struct ArchivedIntent {
    source: ObservationId,
    call_id: String,
    arguments_digest: [u8; 32],
}

#[derive(Clone, Copy)]
struct LocalSemanticIntent {
    output_digest: [u8; 32],
    output_bytes: u64,
}

#[derive(Clone, Copy)]
enum UpdateSource<'a> {
    Archived(&'a ArchivedIntent),
    LocalSemantic(LocalSemanticIntent),
}

impl UpdateSource<'_> {
    const fn identity_digest(self) -> [u8; 32] {
        match self {
            Self::Archived(intent) => intent.arguments_digest,
            Self::LocalSemantic(intent) => intent.output_digest,
        }
    }
}

#[derive(Clone, Copy)]
enum PageEvidence {
    Supports(ObservationId),
    Contradicts(ObservationId),
}

struct PreparedUpdate {
    files: Vec<String>,
    events: Vec<WorkingEvent>,
}

struct AuxiliaryIds {
    primary: ContextNodeId,
    arguments_digest: [u8; 32],
    next: u64,
}

impl AuxiliaryIds {
    const fn new(primary: ContextNodeId, arguments_digest: [u8; 32]) -> Self {
        Self { primary, arguments_digest, next: 0 }
    }

    fn take(&mut self) -> Result<ContextNodeId, DeveloperLoopError> {
        let sequence = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| error("auxiliary entry sequence overflow"))?;
        environment::key(
            format!(
                "context-update-aux:{}:{}:{sequence}",
                hex(self.primary.as_bytes()),
                hex(&self.arguments_digest),
            )
            .as_bytes(),
        )
    }
}

pub(in crate::local_context) fn execute(
    memory: &mut LocalMemory,
    bytes: &[u8],
) -> Result<Value, DeveloperLoopError> {
    execute_inner(memory, bytes, None)
}

/// Streams a local semantic proposal without retaining its transport bytes.
///
/// Local inference has no archived tool-call source, so every accepted field must fit the same
/// logical working-state representation that `prepare` enforces. Physical reducer page sizes do
/// not cap the complete update; they only bound each committed delta.
pub(in crate::local_context) fn execute_reader(
    memory: &mut LocalMemory,
    reader: impl Read,
    output_digest: [u8; 32],
    output_bytes: u64,
) -> Result<Value, DeveloperLoopError> {
    if !memory.derived_memory_allowed() {
        return Ok(rejected(memory, "role policy excludes derived memory"));
    }
    let update = match parser::decode(reader, memory.limits) {
        Ok(update) => update,
        Err(parser::DecodeError::Read) => {
            return Err(error("read exact local update proposal"));
        }
        Err(parser::DecodeError::Authenticate) => {
            return Err(error("authenticate exact local update proposal"));
        }
        Err(parser::DecodeError::Backing) => {
            return Err(error("retain exact local update proposal fields"));
        }
        Err(parser::DecodeError::Invalid) => {
            return Ok(rejected(memory, "invalid update schema"));
        }
    };
    execute_update(
        memory,
        update,
        Some(UpdateSource::LocalSemantic(LocalSemanticIntent {
            output_digest,
            output_bytes,
        })),
    )
}

pub(in crate::local_context) fn execute_call(
    memory: &mut LocalMemory,
    call: &CompletedToolCall,
) -> Result<Value, DeveloperLoopError> {
    if !memory.derived_memory_allowed() {
        return Ok(rejected(memory, "role policy excludes derived memory"));
    }
    let intent = archived_intent(memory, call)?;
    execute_inner(
        memory,
        call.arguments().canonical_bytes(),
        Some(UpdateSource::Archived(&intent)),
    )
}

fn execute_inner(
    memory: &mut LocalMemory,
    bytes: &[u8],
    source: Option<UpdateSource<'_>>,
) -> Result<Value, DeveloperLoopError> {
    if !memory.derived_memory_allowed() {
        return Ok(rejected(memory, "role policy excludes derived memory"));
    }
    let Ok(update) = serde_json::from_slice::<Update>(bytes) else {
        return Ok(rejected(memory, "invalid update schema"));
    };
    execute_update(memory, update, source)
}

fn execute_update(
    memory: &mut LocalMemory,
    update: Update,
    source: Option<UpdateSource<'_>>,
) -> Result<Value, DeveloperLoopError> {
    let prepared = match prepare(memory, &update, source) {
        Ok(proposal) => proposal,
        Err(reason) => return Ok(rejected(memory, &reason.to_string())),
    };
    let mut transcript = memory.transcript.clone();
    transcript.files = prepared.files;
    memory.context_update(update.base_revision, &prepared.events, transcript)?;
    let ids = update
        .operations
        .iter()
        .map(|operation| {
            Ok(Value::from_iter([
                ("label", Value::from(operation.id.as_str())),
                (
                    "id",
                    Value::from(format!("entry:{}", hex(label(&operation.id)?.as_bytes()))),
                ),
            ]))
        })
        .collect::<Result<Vec<_>, DeveloperLoopError>>()?;
    Ok(Value::from_iter([
        ("base_revision", Value::from(memory.model_revision)),
        ("state_revision", Value::from(memory.state.revision())),
        ("entries", Value::from(ids)),
        ("authority", Value::from("none")),
    ]))
}

fn archived_intent(
    memory: &LocalMemory,
    call: &CompletedToolCall,
) -> Result<ArchivedIntent, DeveloperLoopError> {
    let call_id = call.id().expose_for_wire();
    let arguments_digest = sha256(call.arguments().canonical_bytes()).into_bytes();
    let pending = memory
        .transcript
        .pending
        .iter()
        .find(|pending| {
            pending.invocation == memory.transcript.invocation
                && pending.handle.is_none()
                && pending.state == PendingState::Proposed
                && pending.call.id == call_id
                && pending.call.name == call.name().as_str()
                && pending.call.arguments_digest == arguments_digest
        })
        .ok_or_else(|| error("context update lacks its exact archived proposal"))?;
    Ok(ArchivedIntent {
        source: ObservationId::new(pending.source)
            .map_err(|_| error("invalid context update proposal source"))?,
        call_id: call_id.to_owned(),
        arguments_digest,
    })
}

fn prepare(
    memory: &LocalMemory,
    update: &Update,
    source: Option<UpdateSource<'_>>,
) -> Result<PreparedUpdate, DeveloperLoopError> {
    if update.base_revision != memory.model_revision {
        return Err(error("working-model revision conflict; inspect current state"));
    }
    if update.operations.is_empty() {
        return Err(error("update has no operations"));
    }

    // Re-observe the predecessor before interpreting the proposal. A changed predecessor is not
    // partially adopted by a rejected update; the next model view publishes that refresh first.
    let current_paths =
        environment::projected_paths(&memory.state, &memory.transcript.files, &[])?;
    let current_environment = environment::capture(
        &memory.workspace,
        memory.binding,
        &current_paths,
        &memory.task_contract,
        memory.limits,
        &memory.workspace_scope,
        &memory.cancellation,
    )?;
    if &current_environment != memory.state.environment() {
        return Err(error("workspace bindings changed; inspect the refreshed state before updating"));
    }

    let mut files = memory.transcript.files.clone();
    let mut additions = Vec::new();
    for operation in &update.operations {
        validate_operation_paths(memory, operation, &mut files, &mut additions)?;
    }
    files.sort();
    files.dedup();
    let projected = environment::projected_paths(&memory.state, &files, &additions)?;
    let environment = environment::capture(
        &memory.workspace,
        memory.binding,
        &projected,
        &memory.task_contract,
        memory.limits,
        &memory.workspace_scope,
        &memory.cancellation,
    )?;
    let mut state = memory.state.clone();
    let mut events = Vec::new();
    if &environment != state.environment() {
        let refresh = WorkingEvent::Refresh {
            base_revision: state.revision(),
            environment: environment.clone(),
        };
        state = apply_working_event(&state, &refresh)
            .map_err(|_| error("invalid proposal environment"))?;
        events.push(refresh);
    }

    let mut entries = Vec::new();
    for (index, operation) in update.operations.iter().enumerate() {
        entries.extend(operation_entries(memory, &environment, operation, index, source)?);
    }
    events.extend(paged_deltas(memory, &mut state, entries)?);
    memory.ensure_required_state_fits(&state)?;
    Ok(PreparedUpdate { files, events })
}

fn validate_operation_paths(
    memory: &LocalMemory,
    operation: &Operation,
    files: &mut Vec<String>,
    additions: &mut Vec<String>,
) -> Result<(), DeveloperLoopError> {
    if memory.workspace_scope.direct && matches!(operation.validity, Validity::Candidate) {
        return Err(error(
            "folder memory requires file, conversation or task validity; no Git candidate exists",
        ));
    }
    if !matches!(operation.validity, Validity::Files) && !operation.files.is_empty() {
        return Err(error(
            "file paths require validity=files; candidate, conversation and task validity require empty files",
        ));
    }
    let mut canonical = BTreeSet::new();
    for path in &operation.files {
        if path.is_empty() || !canonical.insert(path.as_str()) {
            return Err(error("invalid or duplicate file validity declaration"));
        }
        crate::developer_tools::checked_protected_file_for_developer(
            &memory.workspace,
            path,
            &memory.task_contract,
            &memory.workspace_scope.protected,
        )?;
        files.push(path.clone());
        additions.push(path.clone());
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "one operation is expanded through source, dependency, file, and text reference pages"
)]
fn operation_entries(
    memory: &LocalMemory,
    environment: &WorkingEnvironment,
    operation: &Operation,
    operation_index: usize,
    source: Option<UpdateSource<'_>>,
) -> Result<Vec<WorkingEntry>, DeveloperLoopError> {
    let primary = label(&operation.id)?;
    let limits = memory.limits;
    let maximum = limits.links();
    let mut entries = Vec::new();
    let mut supports = source_ids(memory, &operation.supports)?;
    let mut contradicts = source_ids(memory, &operation.contradicts)?;
    if supports.is_empty() && contradicts.is_empty() {
        return Err(error("working entry requires supporting or contradicting evidence"));
    }
    if supports.iter().any(|source| contradicts.binary_search(source).is_ok()) {
        return Err(error("support and contradiction sources overlap"));
    }
    let page_evidence = supports
        .first()
        .copied()
        .map(PageEvidence::Supports)
        .or_else(|| contradicts.first().copied().map(PageEvidence::Contradicts))
        .ok_or_else(|| error("working entry requires page evidence"))?;
    let dependencies = dependency_ids(&operation.depends_on)?;
    let file_digests = file_digests(environment, &operation.files)?;
    if matches!(operation.validity, Validity::Files) && file_digests.is_empty() {
        return Err(error("file validity requires an observed file"));
    }

    let text_is_archived = operation.text.len() > limits.entry_bytes()
        || super::secret_text::contains_credential(&operation.text);
    let mut paginate_supports = supports.len() > maximum;
    let mut paginate_contradicts = contradicts.len() > maximum;
    let paginate_files = file_digests.len() > maximum;
    let needs_source = text_is_archived
        || paginate_supports
        || paginate_contradicts
        || paginate_files
        || dependencies.len() > maximum;
    if text_is_archived && !matches!(source, Some(UpdateSource::Archived(_))) {
        return Err(error(
            "large or credential-shaped text requires an archived tool-call source",
        ));
    }
    if text_is_archived {
        let Some(UpdateSource::Archived(archived)) = source else { unreachable!() };
        if contradicts.binary_search(&archived.source).is_ok() {
            paginate_contradicts = true;
        }
        if supports.binary_search(&archived.source).is_err() && supports.len() == maximum {
            paginate_supports = true;
        }
    }
    let source = if needs_source {
        Some(source.ok_or_else(|| {
            error("paged update fields require a durable update source")
        })?)
    } else {
        source
    };
    let mut auxiliary_ids = AuxiliaryIds::new(
        primary,
        source.map_or([0; 32], UpdateSource::identity_digest),
    );

    let mut dependency_roots = Vec::new();
    if paginate_supports {
        let source = source
            .ok_or_else(|| error("support pages lack an archived tool-call source"))?;
        dependency_roots.extend(source_pages(
            memory,
            operation_index,
            "supports",
            &operation.supports,
            &supports,
            false,
            source,
            &mut auxiliary_ids,
            &mut entries,
        )?);
        supports.clear();
    }
    if paginate_contradicts {
        let source = source
            .ok_or_else(|| error("contradiction pages lack an archived tool-call source"))?;
        dependency_roots.extend(source_pages(
            memory,
            operation_index,
            "contradicts",
            &operation.contradicts,
            &contradicts,
            true,
            source,
            &mut auxiliary_ids,
            &mut entries,
        )?);
        contradicts.clear();
    }
    if paginate_files {
        let source = source
            .ok_or_else(|| error("file pages lack an archived tool-call source"))?;
        dependency_roots.extend(file_pages(
            memory,
            operation_index,
            &operation.files,
            &file_digests,
            source,
            page_evidence,
            &mut auxiliary_ids,
            &mut entries,
        )?);
    }

    if text_is_archived || (supports.is_empty() && contradicts.is_empty()) {
        match source {
            Some(UpdateSource::Archived(archived)) => {
                if supports.binary_search(&archived.source).is_err() {
                    supports.push(archived.source);
                    supports.sort();
                }
            }
            Some(UpdateSource::LocalSemantic(_)) => match page_evidence {
                PageEvidence::Supports(id) => {
                    if supports.binary_search(&id).is_err() {
                        supports.push(id);
                        supports.sort();
                    }
                }
                PageEvidence::Contradicts(id) => {
                    if contradicts.binary_search(&id).is_err() {
                        contradicts.push(id);
                        contradicts.sort();
                    }
                }
            },
            None => {}
        }
    }
    if text_is_archived {
        let Some(UpdateSource::Archived(archived)) = source else { unreachable!() };
        if supports.binary_search(&archived.source).is_err() {
            supports.push(archived.source);
            supports.sort();
        }
    }
    if supports.iter().any(|id| contradicts.binary_search(id).is_ok()) {
        return Err(error("support and contradiction sources overlap after paging"));
    }
    let mut combined_dependencies = dependencies;
    combined_dependencies.extend(dependency_roots);
    combined_dependencies.sort();
    if combined_dependencies.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(error("duplicate update dependency"));
    }
    let dependencies = if combined_dependencies.len() > maximum {
        let source = source.ok_or_else(|| {
            error("large dependency sets require a durable update source")
        })?;
        dependency_pages(
            memory,
            operation_index,
            &operation.depends_on,
            combined_dependencies,
            source,
            page_evidence,
            &mut auxiliary_ids,
            &mut entries,
        )?
    } else {
        combined_dependencies
    };

    let text = if text_is_archived {
        let Some(UpdateSource::Archived(archived)) = source else { unreachable!() };
        reference_text(
            memory,
            UpdateSource::Archived(archived),
            &format!("/operations/{operation_index}/text"),
            operation.text.as_bytes(),
            None,
        )?
    } else {
        operation.text.clone()
    };
    let validity = if paginate_files {
        WorkingValidity::new(None, None, Vec::new(), limits)
    } else {
        validity(memory, environment, operation, file_digests)
    }
    .map_err(|_| error("invalid validity declaration"))?;
    let links = WorkingLinks::new(supports, contradicts, dependencies, limits)
        .map_err(|_| error("duplicate, overlapping, absent, or excessive source links"))?;
    let content = content(memory, &text)?;
    let kind = operation_kind(operation.kind);
    let status = operation_status(operation.status);
    let mut entry = WorkingEntry::new(primary, kind, content, links, validity, limits)
        .and_then(|entry| entry.with_status(status))
        .map_err(|_| error("invalid entry"))?;
    if let Some(previous) = &operation.supersedes {
        entry = entry
            .with_supersedes(label(previous)?)
            .map_err(|_| error("invalid supersession"))?;
    }
    entries.push(entry);
    Ok(entries)
}

fn source_pages(
    memory: &LocalMemory,
    operation_index: usize,
    field: &str,
    raw: &[String],
    sources: &[ObservationId],
    contradictions: bool,
    source: UpdateSource<'_>,
    ids: &mut AuxiliaryIds,
    entries: &mut Vec<WorkingEntry>,
) -> Result<Vec<ContextNodeId>, DeveloperLoopError> {
    let encoded = serde_json::to_vec(raw).map_err(|_| error("encode source reference list"))?;
    let mut roots = Vec::new();
    for (page, chunk) in sources.chunks(memory.limits.links()).enumerate() {
        let id = ids.take()?;
        let text = reference_text(
            memory,
            source,
            &format!("/operations/{operation_index}/{field}"),
            &encoded,
            Some(page),
        )?;
        let (supports, contradicts) = if contradictions {
            (Vec::new(), chunk.to_vec())
        } else {
            (chunk.to_vec(), Vec::new())
        };
        entries.push(auxiliary_entry(
            memory,
            id,
            text,
            supports,
            contradicts,
            Vec::new(),
            WorkingValidity::new(None, None, Vec::new(), memory.limits)
                .map_err(|_| error("invalid source page validity"))?,
        )?);
        roots.push(id);
    }
    Ok(roots)
}

#[allow(clippy::too_many_arguments)]
fn file_pages(
    memory: &LocalMemory,
    operation_index: usize,
    raw: &[String],
    files: &[WorkingFileDigest],
    source: UpdateSource<'_>,
    evidence: PageEvidence,
    ids: &mut AuxiliaryIds,
    entries: &mut Vec<WorkingEntry>,
) -> Result<Vec<ContextNodeId>, DeveloperLoopError> {
    let encoded = serde_json::to_vec(raw).map_err(|_| error("encode file reference list"))?;
    let mut roots = Vec::new();
    for (page, chunk) in files.chunks(memory.limits.links()).enumerate() {
        let id = ids.take()?;
        let text = reference_text(
            memory,
            source,
            &format!("/operations/{operation_index}/files"),
            &encoded,
            Some(page),
        )?;
        let (supports, contradicts) = page_evidence_links(source, evidence);
        entries.push(auxiliary_entry(
            memory,
            id,
            text,
            supports,
            contradicts,
            Vec::new(),
            WorkingValidity::new(None, None, chunk.to_vec(), memory.limits)
                .map_err(|_| error("invalid file page validity"))?,
        )?);
        roots.push(id);
    }
    Ok(roots)
}

#[allow(clippy::too_many_arguments)]
fn dependency_pages(
    memory: &LocalMemory,
    operation_index: usize,
    raw: &[String],
    mut dependencies: Vec<ContextNodeId>,
    source: UpdateSource<'_>,
    evidence: PageEvidence,
    ids: &mut AuxiliaryIds,
    entries: &mut Vec<WorkingEntry>,
) -> Result<Vec<ContextNodeId>, DeveloperLoopError> {
    let maximum = memory.limits.links();
    if maximum < 2 && dependencies.len() > maximum {
        return Err(error("configured link page cannot represent a dependency tree"));
    }
    let encoded = serde_json::to_vec(raw).map_err(|_| error("encode dependency list"))?;
    let mut level = 0_usize;
    while dependencies.len() > maximum {
        let mut roots = Vec::new();
        for (page, chunk) in dependencies.chunks(maximum).enumerate() {
            let id = ids.take()?;
            let page = level
                .checked_mul(maximum)
                .and_then(|base| base.checked_add(page))
                .ok_or_else(|| error("dependency page number overflow"))?;
            let text = reference_text(
                memory,
                source,
                &format!("/operations/{operation_index}/depends_on"),
                &encoded,
                Some(page),
            )?;
            let (supports, contradicts) = page_evidence_links(source, evidence);
            entries.push(auxiliary_entry(
                memory,
                id,
                text,
                supports,
                contradicts,
                chunk.to_vec(),
                WorkingValidity::new(None, None, Vec::new(), memory.limits)
                    .map_err(|_| error("invalid dependency page validity"))?,
            )?);
            roots.push(id);
        }
        dependencies = roots;
        level = level
            .checked_add(1)
            .ok_or_else(|| error("dependency page level overflow"))?;
    }
    Ok(dependencies)
}

fn page_evidence_links(
    source: UpdateSource<'_>,
    evidence: PageEvidence,
) -> (Vec<ObservationId>, Vec<ObservationId>) {
    match source {
        UpdateSource::Archived(intent) => (vec![intent.source], Vec::new()),
        UpdateSource::LocalSemantic(_) => match evidence {
            PageEvidence::Supports(id) => (vec![id], Vec::new()),
            PageEvidence::Contradicts(id) => (Vec::new(), vec![id]),
        },
    }
}

fn auxiliary_entry(
    memory: &LocalMemory,
    id: ContextNodeId,
    text: String,
    supports: Vec<ObservationId>,
    contradicts: Vec<ObservationId>,
    dependencies: Vec<ContextNodeId>,
    validity: WorkingValidity,
) -> Result<WorkingEntry, DeveloperLoopError> {
    let links = WorkingLinks::new(
        supports,
        contradicts,
        dependencies,
        memory.limits,
    )
    .map_err(|_| error("invalid update reference page"))?;
    WorkingEntry::new(
        id,
        WorkingEntryKind::Observation,
        content(memory, &text)?,
        links,
        validity,
        memory.limits,
    )
    .map_err(|_| error("invalid update reference entry"))
}

fn reference_text(
    memory: &LocalMemory,
    source: UpdateSource<'_>,
    pointer: &str,
    bytes: &[u8],
    page: Option<usize>,
) -> Result<String, DeveloperLoopError> {
    let mut reference = serde_json::Map::from_iter([
        ("field_sha256".to_owned(), Value::from(hex(sha256(bytes).as_bytes()))),
        ("pointer".to_owned(), Value::from(pointer)),
        (
            "field_bytes".to_owned(),
            Value::from(u64::try_from(bytes.len()).map_err(|_| error("field size overflow"))?),
        ),
    ]);
    let name = match source {
        UpdateSource::Archived(intent) => {
            reference.insert(
                "arguments_sha256".to_owned(),
                Value::from(hex(&intent.arguments_digest)),
            );
            reference.insert("call_id".to_owned(), Value::from(intent.call_id.as_str()));
            reference.insert(
                "source".to_owned(),
                Value::from(source_handle(memory, intent.source.get())),
            );
            "archived_context_update"
        }
        UpdateSource::LocalSemantic(intent) => {
            reference.insert(
                "output_sha256".to_owned(),
                Value::from(hex(&intent.output_digest)),
            );
            reference.insert("output_bytes".to_owned(), Value::from(intent.output_bytes));
            "local_semantic_update"
        }
    };
    if let Some(page) = page {
        reference.insert(
            "page".to_owned(),
            Value::from(u64::try_from(page).map_err(|_| error("field page overflow"))?),
        );
    }
    let text = Value::from_iter([(name, Value::Object(reference))]).to_string();
    if text.len() > memory.limits.entry_bytes() {
        return Err(error("update field reference exceeds the inline entry envelope"));
    }
    Ok(text)
}

fn paged_deltas(
    memory: &LocalMemory,
    state: &mut WorkingState,
    entries: Vec<WorkingEntry>,
) -> Result<Vec<WorkingEvent>, DeveloperLoopError> {
    let expected = entries.len();
    let mut entries = entries
        .into_iter()
        .map(|entry| (entry.id(), entry))
        .collect::<BTreeMap<_, _>>();
    if expected == 0 {
        return Err(error("update has no entries"));
    }
    if entries.len() != expected {
        return Err(error("duplicate primary or archived-page entry identity"));
    }
    let mut incoming = entries.keys().map(|id| (*id, 0_usize)).collect::<BTreeMap<_, _>>();
    let mut outgoing = BTreeMap::<ContextNodeId, Vec<ContextNodeId>>::new();
    for entry in entries.values() {
        let mut dependencies = entry.links().depends_on().to_vec();
        if let Some(previous) = entry.supersedes() {
            dependencies.push(previous);
        }
        for dependency in dependencies {
            if incoming.contains_key(&dependency) {
                let count = incoming
                    .get_mut(&entry.id())
                    .ok_or_else(|| error("missing proposed entry"))?;
                *count = count
                    .checked_add(1)
                    .ok_or_else(|| error("proposal dependency count overflow"))?;
                outgoing.entry(dependency).or_default().push(entry.id());
            }
        }
    }
    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(expected);
    while let Some(id) = ready.iter().next().copied() {
        ready.remove(&id);
        ordered.push(id);
        if let Some(dependents) = outgoing.get(&id) {
            for dependent in dependents {
                let count = incoming
                    .get_mut(dependent)
                    .ok_or_else(|| error("missing dependent entry"))?;
                *count = count
                    .checked_sub(1)
                    .ok_or_else(|| error("proposal dependency count underflow"))?;
                if *count == 0 {
                    ready.insert(*dependent);
                }
            }
        }
    }
    if ordered.len() != expected {
        return Err(error("update dependency graph is cyclic"));
    }

    let mut events = Vec::new();
    for page in ordered.chunks(memory.limits.operations()) {
        let mut proposed = page
            .iter()
            .map(|id| entries.remove(id).ok_or_else(|| error("missing paged entry")))
            .collect::<Result<Vec<_>, _>>()?;
        proposed.sort_by_key(WorkingEntry::id);
        let delta = WorkingDelta::new(state.binding(), state.revision(), proposed, memory.limits)
            .map_err(|_| error("duplicate or invalid update operations"))?;
        *state = apply_working_delta(state, &delta)
            .map_err(|_| error("update references, dependencies, freshness, or capacity rejected"))?;
        events.push(WorkingEvent::Delta(delta));
    }
    Ok(events)
}

fn label(value: &str) -> Result<ContextNodeId, DeveloperLoopError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(error("invalid working-entry label"));
    }
    if let Some(id) = value.strip_prefix("entry:") {
        if id.len() != 32
            || !id.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(error("invalid explicit working-entry identity"));
        }
        let mut bytes = [0; 16];
        for (index, pair) in id.as_bytes().chunks_exact(2).enumerate() {
            let text =
                std::str::from_utf8(pair).map_err(|_| error("invalid entry identity encoding"))?;
            bytes[index] = u8::from_str_radix(text, 16)
                .map_err(|_| error("invalid entry identity encoding"))?;
        }
        return ContextNodeId::new(bytes).map_err(|_| error("invalid entry identity"));
    }
    environment::key(format!("agent-entry:{value}").as_bytes())
}

fn source_ids(
    memory: &LocalMemory,
    handles: &[String],
) -> Result<Vec<ObservationId>, DeveloperLoopError> {
    let mut ids = handles
        .iter()
        .map(|handle| {
            ObservationId::new(sequence(memory, handle)?)
                .map_err(|_| error("invalid source reference"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ids.sort();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(error("duplicate source reference"));
    }
    Ok(ids)
}

fn dependency_ids(values: &[String]) -> Result<Vec<ContextNodeId>, DeveloperLoopError> {
    let mut dependencies = values.iter().map(|value| label(value)).collect::<Result<Vec<_>, _>>()?;
    dependencies.sort();
    if dependencies.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(error("duplicate update dependency"));
    }
    Ok(dependencies)
}

fn file_digests(
    environment: &WorkingEnvironment,
    paths: &[String],
) -> Result<Vec<WorkingFileDigest>, DeveloperLoopError> {
    let mut files = paths
        .iter()
        .map(|path| {
            let key = environment::key(path.as_bytes())?;
            environment
                .files()
                .iter()
                .find(|file| file.key() == key)
                .copied()
                .ok_or_else(|| error("file dependency unavailable"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    files.sort_by_key(|file| file.key());
    if files.windows(2).any(|pair| pair[0].key() == pair[1].key()) {
        return Err(error("duplicate file dependency"));
    }
    Ok(files)
}

fn validity(
    memory: &LocalMemory,
    environment: &WorkingEnvironment,
    operation: &Operation,
    files: Vec<WorkingFileDigest>,
) -> Result<WorkingValidity, peritus_context::working::WorkingError> {
    WorkingValidity::new(
        matches!(operation.validity, Validity::Conversation)
            .then_some(memory.binding.conversation_revision()),
        matches!(operation.validity, Validity::Candidate).then_some(environment.candidate()),
        files,
        memory.limits,
    )
}

fn content(
    memory: &LocalMemory,
    text: &str,
) -> Result<peritus_context::ContextContent, DeveloperLoopError> {
    let limits = ContextLimits::new(
        memory.limits.entries(),
        memory.limits.entry_bytes(),
        memory.limits.links(),
        5,
    )
    .map_err(|_| error("invalid entry limits"))?;
    bind_context_content(text.as_bytes().to_vec(), sha256(text.as_bytes()), limits)
        .map_err(|_| error("empty or oversized entry"))
}

const fn operation_kind(kind: Kind) -> WorkingEntryKind {
    match kind {
        Kind::Observation => WorkingEntryKind::Observation,
        Kind::Assertion => WorkingEntryKind::Assertion,
        Kind::Hypothesis => WorkingEntryKind::Hypothesis,
        Kind::Decision => WorkingEntryKind::Decision,
        Kind::FailedApproach => WorkingEntryKind::FailedApproach,
        Kind::Plan => WorkingEntryKind::Plan,
        Kind::Glossary => WorkingEntryKind::Glossary,
    }
}

const fn operation_status(status: Status) -> WorkingEntryStatus {
    match status {
        Status::Open => WorkingEntryStatus::Open,
        Status::Contradicted => WorkingEntryStatus::Contradicted,
        Status::Resolved => WorkingEntryStatus::Resolved,
    }
}
