//! Validate a whole proposal before committing; archive/protocol events do not conflict with it.

use super::super::memory::environment;
use super::{LocalMemory, error, hex, rejected, sequence};
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::{
    ContextLimits, bind_context_content,
    working::{
        ObservationId, WorkingDelta, WorkingEntry, WorkingEntryKind, WorkingEntryStatus,
        WorkingEnvironment, WorkingEvent, WorkingLinks, WorkingValidity, apply_working_delta,
        apply_working_event,
    },
};
use serde::Deserialize;
use serde_json::Value;

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

pub(in crate::local_context) fn execute(
    memory: &mut LocalMemory,
    bytes: &[u8],
) -> Result<Value, DeveloperLoopError> {
    if !memory.derived_memory_allowed() {
        return Ok(rejected(memory, "role policy excludes derived memory"));
    }
    if bytes.len() > 262_144 {
        return Ok(rejected(memory, "update exceeds input bound"));
    }
    let Ok(update) = serde_json::from_slice::<Update>(bytes) else {
        return Ok(rejected(memory, "invalid update schema"));
    };
    // Refresh before revision validation; changed files must never receive stale conclusions.
    memory.refresh()?;
    let proposal = match prepare(memory, &update) {
        Ok(proposal) => proposal,
        Err(reason) => return Ok(rejected(memory, &reason.to_string())),
    };
    let (environment, files, delta) = proposal;
    if memory.state.environment() != &environment {
        memory.state_event(&WorkingEvent::Refresh {
            base_revision: memory.state.revision(),
            environment,
        })?;
    }
    if memory.transcript.files != files {
        memory.transcript.files = files;
        memory.persist_transcript()?;
    }
    memory.state_event(&WorkingEvent::Delta(delta))?;
    let ids = update
        .operations
        .iter()
        .map(|op| {
            Ok(Value::from_iter([
                ("label", Value::from(op.id.as_str())),
                ("id", Value::from(format!("entry:{}", hex(label(&op.id)?.as_bytes())))),
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

fn prepare(
    memory: &LocalMemory,
    update: &Update,
) -> Result<(WorkingEnvironment, Vec<String>, WorkingDelta), DeveloperLoopError> {
    if update.base_revision != memory.model_revision {
        return Err(error("working-model revision conflict; inspect current state"));
    }
    if update.operations.is_empty() || update.operations.len() > memory.limits.operations() {
        return Err(error("update operation capacity exceeded"));
    }
    let mut paths = memory.transcript.files.clone();
    for operation in &update.operations {
        if memory.workspace_scope.direct && matches!(operation.validity, Validity::Candidate) {
            return Err(error(
                "folder memory requires file, conversation or task validity; no Git candidate exists",
            ));
        }
        if operation.files.len() > memory.limits.links()
            || (!matches!(operation.validity, Validity::Files) && !operation.files.is_empty())
        {
            return Err(error("invalid file validity declaration"));
        }
        for path in &operation.files {
            if path.len() > 4096 {
                return Err(error("file path exceeds bound"));
            }
            crate::developer_tools::checked_protected_file_for_developer(
                &memory.workspace,
                path,
                &memory.task_contract,
                &memory.workspace_scope.protected,
            )?;
            paths.push(path.clone());
        }
    }
    paths.sort();
    paths.dedup();
    if paths.len() > memory.limits.entries() {
        return Err(error("file dependency capacity exceeded"));
    }
    let environment = environment::capture(
        &memory.workspace,
        memory.binding,
        &paths,
        &memory.task_contract,
        memory.limits,
        &memory.workspace_scope,
    )?;
    let state = if &environment == memory.state.environment() {
        memory.state.clone()
    } else {
        apply_working_event(
            &memory.state,
            &WorkingEvent::Refresh {
                base_revision: memory.state.revision(),
                environment: environment.clone(),
            },
        )
        .map_err(|_| error("invalid proposal environment"))?
    };
    let mut entries = update
        .operations
        .iter()
        .map(|op| entry(memory, &environment, op))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(WorkingEntry::id);
    let delta = WorkingDelta::new(state.binding(), state.revision(), entries, memory.limits)
        .map_err(|_| error("duplicate or invalid update operations"))?;
    apply_working_delta(&state, &delta)
        .map_err(|_| error("update references, dependencies, freshness, or capacity rejected"))?;
    Ok((environment, paths, delta))
}

fn label(value: &str) -> Result<peritus_context::ContextNodeId, DeveloperLoopError> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
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
        return peritus_context::ContextNodeId::new(bytes)
            .map_err(|_| error("invalid entry identity"));
    }
    environment::key(format!("agent-entry:{value}").as_bytes())
}

fn entry(
    memory: &LocalMemory,
    environment: &WorkingEnvironment,
    op: &Operation,
) -> Result<WorkingEntry, DeveloperLoopError> {
    if op.text.len() > memory.limits.entry_bytes() {
        return Err(error("entry text exceeds bound"));
    }
    if super::secret_text::contains_credential(&op.text) {
        return Err(error("credential material cannot be retained in derived entries"));
    }
    let limits = memory.limits;
    let content_limits =
        ContextLimits::new(limits.entries(), limits.entry_bytes(), limits.links(), 5)
            .map_err(|_| error("invalid entry limits"))?;
    let content = bind_context_content(
        op.text.as_bytes().to_vec(),
        sha256(op.text.as_bytes()),
        content_limits,
    )
    .map_err(|_| error("empty or oversized entry"))?;
    let sources = |handles: &[String]| -> Result<Vec<ObservationId>, DeveloperLoopError> {
        if handles.len() > limits.links() {
            return Err(error("source reference capacity exceeded"));
        }
        let mut ids = handles
            .iter()
            .map(|handle| {
                ObservationId::new(sequence(memory, handle)?)
                    .map_err(|_| error("invalid source reference"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        ids.sort();
        Ok(ids)
    };
    if op.depends_on.len() > limits.links() {
        return Err(error("dependency capacity exceeded"));
    }
    let mut dependencies =
        op.depends_on.iter().map(|value| label(value)).collect::<Result<Vec<_>, _>>()?;
    dependencies.sort();
    let links =
        WorkingLinks::new(sources(&op.supports)?, sources(&op.contradicts)?, dependencies, limits)
            .map_err(|_| error("duplicate, overlapping, or absent source links"))?;
    let files = op
        .files
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
    let mut files = files;
    files.sort_by_key(|file| file.key());
    if matches!(op.validity, Validity::Files) && files.is_empty() {
        return Err(error("file validity requires an observed file"));
    }
    let validity = WorkingValidity::new(
        matches!(op.validity, Validity::Conversation)
            .then_some(memory.binding.conversation_revision()),
        matches!(op.validity, Validity::Candidate).then_some(environment.candidate()),
        files,
        limits,
    )
    .map_err(|_| error("invalid validity declaration"))?;
    let kind = match op.kind {
        Kind::Observation => WorkingEntryKind::Observation,
        Kind::Assertion => WorkingEntryKind::Assertion,
        Kind::Hypothesis => WorkingEntryKind::Hypothesis,
        Kind::Decision => WorkingEntryKind::Decision,
        Kind::FailedApproach => WorkingEntryKind::FailedApproach,
        Kind::Plan => WorkingEntryKind::Plan,
        Kind::Glossary => WorkingEntryKind::Glossary,
    };
    let status = match op.status {
        Status::Open => WorkingEntryStatus::Open,
        Status::Contradicted => WorkingEntryStatus::Contradicted,
        Status::Resolved => WorkingEntryStatus::Resolved,
    };
    let entry = WorkingEntry::new(label(&op.id)?, kind, content, links, validity, limits)
        .and_then(|entry| entry.with_status(status))
        .map_err(|_| error("invalid entry"))?;
    match &op.supersedes {
        Some(previous) => {
            entry.with_supersedes(label(previous)?).map_err(|_| error("invalid supersession"))
        }
        None => Ok(entry),
    }
}
