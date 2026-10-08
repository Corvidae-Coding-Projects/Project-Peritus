//! Incremental typed reviewer submissions retained by the existing tool-observation lineage.

use std::{collections::BTreeMap, fmt::Write as _};

use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecution, DeveloperToolExecutor,
    DeveloperToolObservation,
};
use peritus_model_protocol::{
    BoundedText, CanonicalJson, CompletedToolCall, JsonBounds, JsonSchema, Message,
    ProtocolLimits, SchemaDialect, ToolDefinition, ToolName,
};
use peritus_review::ProductReviewSubmission;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

use crate::review;

pub(super) const APPEND_TOOL: &str = "review_submission_append";
pub(super) const READ_TOOL: &str = "review_submission_read";
pub(super) const FINISH_TOOL: &str = "review_submission_finish";

const PAGE_BYTES: usize = 65_536;
// DeveloperContextPort::source_reference is bounded to 1024 encoded bytes. ContextSession adds
// `,"local_context":` to this module's nonempty object after ordinary tool-output projection.
const LOCAL_CONTEXT_ANNOTATION_BYTES: usize = 1_024 + 17;
const FINDING_FIELD_COUNT: usize = 7;

#[derive(Clone)]
pub(super) struct SubmissionDraft {
    handle: String,
    review_cycle: u32,
    expected_coverage_cursor: Option<String>,
    fields: BTreeMap<FieldPath, CommittedField>,
    pending: Option<PendingField>,
    frontier: usize,
    finished: Option<FinishedSubmission>,
}

#[derive(Clone)]
struct CommittedField {
    generation: u64,
    text: String,
}

#[derive(Clone)]
struct PendingField {
    path: FieldPath,
    generation: u64,
    text: String,
}

#[derive(Clone)]
struct FinishedSubmission {
    finding_count: usize,
    coverage_cursor: Option<String>,
    digest: String,
    submission: ProductReviewSubmission,
}

impl SubmissionDraft {
    pub(super) fn new(
        handle: String,
        review_cycle: u32,
        expected_coverage_cursor: Option<&str>,
    ) -> Self {
        Self {
            handle,
            review_cycle,
            expected_coverage_cursor: expected_coverage_cursor.map(str::to_owned),
            fields: BTreeMap::new(),
            pending: None,
            frontier: 0,
            finished: None,
        }
    }

    pub(super) fn finished(&self) -> Option<ProductReviewSubmission> {
        self.finished.as_ref().map(|finished| finished.submission.clone())
    }

    pub(super) fn catalog(&self) -> String {
        let accepted_findings = self.frontier.saturating_sub(1) / FINDING_FIELD_COUNT;
        let next = self.next_path().as_string();
        let mut catalog = format!(
            "\n\nIncremental typed review submission: handle={} accepted_fields={} accepted_findings={} next_path={next}. Use {APPEND_TOOL} to append decoded UTF-8 field text at the exact byte frontier. Complete summary first, then each finding's category, severity, title, description, location, reproduction and remediation in index order. At a finding boundary, either begin the next finding or call {FINISH_TOOL} with the exact finding_count. Use generation 0 for initial fields and the next generation only to atomically replace one completed field after a validation error. Use {READ_TOOL} to retrieve any retained field without re-emitting it. Physical calls may be repeated across turns and segments; they do not impose a cumulative submission allowance. A complete legacy JSON response remains accepted.",
            self.handle, self.frontier,
        );
        if let Some(cursor) = &self.expected_coverage_cursor {
            let _ = write!(
                catalog,
                " To assert this bounded subset, call {FINISH_TOOL} with coverage_cursor={cursor}. Omitting coverage_cursor retains the legacy full-coverage assertion.",
            );
        }
        if let Some(pending) = &self.pending {
            let _ = write!(
                catalog,
                " Pending field path={} generation={} bytes={} sha256={} must continue at offset={} before any other field.",
                pending.path.as_string(),
                pending.generation,
                pending.text.len(),
                digest(&pending.text),
                pending.text.len(),
            );
        }
        if let Some(finished) = &self.finished {
            let _ = write!(
                catalog,
                " The staged submission is complete with finding_count={} sha256={}; after satisfying any remaining repository-grounding requirement, repeat {FINISH_TOOL} with that count to hand control to the host.",
                finished.finding_count,
                finished.digest,
            );
        }
        catalog
    }

    fn append(&mut self, arguments: &AppendWire) -> Result<AppendAck, String> {
        self.require_handle(&arguments.handle)?;
        if self.finished.is_some() {
            return Err("the staged review is already complete".to_owned());
        }
        let path = FieldPath::parse(&arguments.path)?;
        let generation = decimal_u64(&arguments.generation, "generation")?;
        let offset = decimal_usize(&arguments.offset, "offset")?;
        let ordinal = path.ordinal()?;

        if let Some(pending) = &mut self.pending {
            if pending.path != path || pending.generation != generation {
                return Err(format!(
                    "continue pending field {} generation {} at offset {} before another field",
                    pending.path.as_string(),
                    pending.generation,
                    pending.text.len(),
                ));
            }
            let end = append_exact(&mut pending.text, offset, &arguments.text)?;
            if arguments.complete && end != pending.text.len() {
                return Err("a pending-field completion must name its exact retained end".to_owned());
            }
            let prefix_sha256 = digest(&pending.text[..end]);
            if arguments.complete {
                let completed = self.pending.take()
                    .ok_or_else(|| "pending field disappeared before completion".to_owned())?;
                self.promote(completed)?;
            }
            return Ok(self.ack(path, generation, offset, end, prefix_sha256, arguments.complete));
        }

        if let Some(committed) = self.fields.get(&path) {
            if generation == committed.generation {
                let end = duplicate_exact(&committed.text, offset, &arguments.text)?;
                if arguments.complete && end != committed.text.len() {
                    return Err("a completed-field replay may finish only at its exact end".to_owned());
                }
                return Ok(self.ack(
                    path,
                    generation,
                    offset,
                    end,
                    digest(&committed.text[..end]),
                    arguments.complete,
                ));
            }
            if generation != committed.generation.checked_add(1)
                .ok_or_else(|| "field generation overflow".to_owned())?
            {
                return Err(format!(
                    "field {} requires generation {} for a replacement",
                    path.as_string(),
                    committed.generation.saturating_add(1),
                ));
            }
            if offset != 0 {
                return Err("a replacement generation must begin at offset 0".to_owned());
            }
        } else {
            if generation != 0 {
                return Err("an initial field requires generation 0".to_owned());
            }
            if ordinal != self.frontier {
                return Err(format!(
                    "initial fields are ordered; expected {}",
                    FieldPath::from_ordinal(self.frontier).as_string(),
                ));
            }
            if offset != 0 {
                return Err("an initial field must begin at offset 0".to_owned());
            }
        }

        let mut pending = PendingField { path: path.clone(), generation, text: String::new() };
        let end = append_exact(&mut pending.text, offset, &arguments.text)?;
        let prefix_sha256 = digest(&pending.text[..end]);
        self.pending = Some(pending);
        if arguments.complete {
            let completed = self.pending.take()
                .ok_or_else(|| "new pending field disappeared before completion".to_owned())?;
            self.promote(completed)?;
        }
        Ok(self.ack(path, generation, offset, end, prefix_sha256, arguments.complete))
    }

    fn promote(&mut self, completed: PendingField) -> Result<(), String> {
        let ordinal = completed.path.ordinal()?;
        let initial = !self.fields.contains_key(&completed.path);
        if initial && ordinal != self.frontier {
            return Err("completed initial field does not match the submission frontier".to_owned());
        }
        self.fields.insert(
            completed.path,
            CommittedField { generation: completed.generation, text: completed.text },
        );
        if initial {
            self.frontier = self.frontier.checked_add(1)
                .ok_or_else(|| "submission field frontier overflow".to_owned())?;
        }
        Ok(())
    }

    fn ack(
        &self,
        path: FieldPath,
        generation: u64,
        offset: usize,
        end: usize,
        prefix_sha256: String,
        complete: bool,
    ) -> AppendAck {
        AppendAck {
            handle: self.handle.clone(),
            path: path.as_string(),
            generation: generation.to_string(),
            offset: offset.to_string(),
            end_offset: end.to_string(),
            prefix_sha256,
            complete,
            next_path: self.next_path().as_string(),
        }
    }

    fn finish(&mut self, arguments: &FinishWire) -> Result<FinishAck, String> {
        self.require_handle(&arguments.handle)?;
        let finding_count = decimal_usize(&arguments.finding_count, "finding_count")?;
        if arguments.coverage_cursor.as_deref().is_some_and(|cursor| {
            self.expected_coverage_cursor.as_deref() != Some(cursor)
        }) {
            return Err("coverage_cursor does not match the current finding page".to_owned());
        }
        if let Some(finished) = &self.finished {
            if finished.finding_count != finding_count
                || finished.coverage_cursor != arguments.coverage_cursor
            {
                return Err(
                    "finished review was replayed with different completion arguments".to_owned(),
                );
            }
            return Ok(FinishAck {
                handle: self.handle.clone(),
                finding_count: arguments.finding_count.clone(),
                submission_sha256: finished.digest.clone(),
                state: "complete".to_owned(),
            });
        }
        if let Some(pending) = &self.pending {
            return Err(format!(
                "field {} generation {} is incomplete at offset {}",
                pending.path.as_string(), pending.generation, pending.text.len(),
            ));
        }
        let expected = finding_count.checked_mul(FINDING_FIELD_COUNT)
            .and_then(|fields| fields.checked_add(1))
            .ok_or_else(|| "finding count exceeds the native field address representation".to_owned())?;
        if self.frontier != expected {
            return Err(format!(
                "finding_count {finding_count} requires {expected} completed fields; the staged frontier is {} with next_path={}",
                self.frontier,
                self.next_path().as_string(),
            ));
        }
        let encoded = self.encode_submission(finding_count, arguments.coverage_cursor.as_deref())?;
        let submission = review::parse(&encoded, self.review_cycle)
            .map_err(|error| format!("{}: {}", error.operation(), error.detail()))?;
        let submission_sha256 = digest(&encoded);
        self.finished = Some(FinishedSubmission {
            finding_count,
            coverage_cursor: arguments.coverage_cursor.clone(),
            digest: submission_sha256.clone(),
            submission,
        });
        Ok(FinishAck {
            handle: self.handle.clone(),
            finding_count: arguments.finding_count.clone(),
            submission_sha256,
            state: "complete".to_owned(),
        })
    }

    fn encode_submission(
        &self,
        finding_count: usize,
        coverage_cursor: Option<&str>,
    ) -> Result<String, String> {
        let summary = self.field(&FieldPath::Summary)?.text.clone();
        let mut findings = Vec::new();
        findings.try_reserve(finding_count)
            .map_err(|_| "allocate staged review findings".to_owned())?;
        for index in 0..finding_count {
            let mut finding = Map::new();
            for field in FindingField::ALL {
                let path = FieldPath::Finding { index, field };
                finding.insert(field.as_str().to_owned(), Value::String(self.field(&path)?.text.clone()));
            }
            findings.push(Value::Object(finding));
        }
        match coverage_cursor {
            Some(cursor) => serde_json::to_string(&json!({
                "summary": summary,
                "findings": findings,
                "coverage_cursor": cursor,
            })),
            None => serde_json::to_string(&json!({ "summary": summary, "findings": findings })),
        }
        .map_err(|error| error.to_string())
    }

    fn read(&self, arguments: &ReadWire, page_bytes: usize) -> Result<CanonicalJson, String> {
        self.require_handle(&arguments.handle)?;
        let path = FieldPath::parse(&arguments.path)?;
        let generation = decimal_u64(&arguments.generation, "generation")?;
        let start = decimal_usize(&arguments.offset, "offset")?;
        let text = if let Some(pending) = &self.pending
            && pending.path == path && pending.generation == generation
        {
            &pending.text
        } else {
            let committed = self.field(&path)?;
            if committed.generation != generation {
                return Err("requested field generation is not retained as the current value".to_owned());
            }
            &committed.text
        };
        if start > text.len() || !text.is_char_boundary(start) {
            return Err("field offset is outside the source or splits UTF-8".to_owned());
        }
        if arguments.max_bytes == 0 || arguments.max_bytes > PAGE_BYTES {
            return Err("max_bytes must select a nonzero encoded page of at most 65536 bytes".to_owned());
        }
        let maximum = arguments.max_bytes.min(page_bytes);
        let mut end = text.floor_char_boundary(start.saturating_add(maximum).min(text.len()));
        loop {
            let value = json!({
                "handle": self.handle,
                "path": path.as_string(),
                "generation": arguments.generation,
                "sha256": digest(text),
                "bytes": text.len().to_string(),
                "offset": arguments.offset,
                "end_offset": end.to_string(),
                "next_offset": (end < text.len()).then(|| end.to_string()),
                "text": &text[start..end],
            });
            let encoded = serde_json::to_string(&value).map_err(|error| error.to_string())?;
            if encoded.len() <= maximum {
                if end == start && start < text.len() {
                    return Err("selected page cannot fit metadata plus the next UTF-8 character; increase max_bytes".to_owned());
                }
                return CanonicalJson::parse(
                    &encoded,
                    JsonBounds::value(ProtocolLimits::PRODUCTION),
                ).map_err(|error| error.to_string());
            }
            if end == start {
                return Err("selected page cannot fit its exact metadata; increase max_bytes".to_owned());
            }
            end = text.floor_char_boundary(start + (end - start) / 2);
        }
    }

    fn recover_append(
        &mut self,
        call: &CompletedToolCall,
        output: &CanonicalJson,
    ) -> Result<(), DeveloperLoopError> {
        let arguments = serde_json::from_slice::<AppendWire>(call.arguments().canonical_bytes())
            .map_err(|_| recovery_error("decode retained review append arguments"))?;
        if arguments.handle != self.handle {
            return Ok(());
        }
        let expected = serde_json::from_slice::<AppendAck>(output.canonical_bytes())
            .map_err(|_| recovery_error("decode retained review append acknowledgement"))?;
        let mut candidate = self.clone();
        let actual = candidate.append(&arguments)
            .map_err(|_| recovery_error("retained review append conflicts with its staged frontier"))?;
        if actual != expected {
            return Err(recovery_error("retained review append acknowledgement conflicts with exact field state"));
        }
        *self = candidate;
        Ok(())
    }

    fn recover_finish(
        &mut self,
        call: &CompletedToolCall,
        output: &CanonicalJson,
    ) -> Result<(), DeveloperLoopError> {
        let arguments = serde_json::from_slice::<FinishWire>(call.arguments().canonical_bytes())
            .map_err(|_| recovery_error("decode retained review finish arguments"))?;
        if arguments.handle != self.handle {
            return Ok(());
        }
        let expected = serde_json::from_slice::<FinishAck>(output.canonical_bytes())
            .map_err(|_| recovery_error("decode retained review finish acknowledgement"))?;
        let mut candidate = self.clone();
        let actual = candidate.finish(&arguments)
            .map_err(|_| recovery_error("retained review finish conflicts with its staged fields"))?;
        if actual != expected {
            return Err(recovery_error("retained review finish acknowledgement conflicts with exact submission state"));
        }
        *self = candidate;
        Ok(())
    }

    fn field(&self, path: &FieldPath) -> Result<&CommittedField, String> {
        self.fields.get(path).ok_or_else(|| format!("field {} is not complete", path.as_string()))
    }

    fn require_handle(&self, handle: &str) -> Result<(), String> {
        if handle == self.handle { Ok(()) }
        else { Err("submission handle does not bind this exact reviewer request".to_owned()) }
    }

    fn next_path(&self) -> FieldPath {
        self.pending.as_ref().map_or_else(
            || FieldPath::from_ordinal(self.frontier),
            |pending| pending.path.clone(),
        )
    }
}

pub(super) fn recover_observation(
    call: &CompletedToolCall,
    output: &CanonicalJson,
    draft: &mut SubmissionDraft,
) -> Result<(), DeveloperLoopError> {
    match call.name().as_str() {
        APPEND_TOOL => draft.recover_append(call, output),
        FINISH_TOOL => draft.recover_finish(call, output),
        _ => Ok(()),
    }
}

pub(super) struct SubmissionTools<'tool, 'draft> {
    base: &'tool mut dyn DeveloperToolExecutor,
    draft: &'draft mut SubmissionDraft,
    annotation_bytes: usize,
    page_bytes: usize,
    yield_now: bool,
}

impl<'tool, 'draft> SubmissionTools<'tool, 'draft> {
    pub(super) fn new(
        base: &'tool mut dyn DeveloperToolExecutor,
        draft: &'draft mut SubmissionDraft,
        annotated: bool,
    ) -> Self {
        let annotation_bytes = if annotated { LOCAL_CONTEXT_ANNOTATION_BYTES } else { 0 };
        Self {
            base,
            draft,
            annotation_bytes,
            page_bytes: PAGE_BYTES.saturating_sub(annotation_bytes),
            yield_now: false,
        }
    }

    pub(super) fn finished_this_invocation(&self) -> Option<ProductReviewSubmission> {
        self.yield_now.then(|| self.draft.finished()).flatten()
    }

    fn execute_submission(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let result = match call.name().as_str() {
            APPEND_TOOL => {
                let mut candidate = self.draft.clone();
                let result = (|| {
                    let arguments = serde_json::from_slice::<AppendWire>(
                        call.arguments().canonical_bytes(),
                    ).map_err(|error| error.to_string())?;
                    let ack = candidate.append(&arguments)?;
                    encode(&ack)
                })();
                if result.is_ok() {
                    *self.draft = candidate;
                }
                result
            }
            READ_TOOL => serde_json::from_slice::<ReadWire>(call.arguments().canonical_bytes())
                .map_err(|error| error.to_string())
                .and_then(|arguments| self.draft.read(&arguments, self.page_bytes)),
            FINISH_TOOL => {
                let mut candidate = self.draft.clone();
                let result = (|| {
                    let arguments = serde_json::from_slice::<FinishWire>(
                        call.arguments().canonical_bytes(),
                    ).map_err(|error| error.to_string())?;
                    let ack = candidate.finish(&arguments)?;
                    encode(&ack)
                })();
                if result.is_ok() {
                    *self.draft = candidate;
                    self.yield_now = true;
                }
                result
            }
            _ => Err("unknown incremental review submission tool".to_owned()),
        };
        match result {
            Ok(output) => Ok(DeveloperToolObservation { output, is_error: false }),
            Err(error) => observation_error(error),
        }
    }
}

impl DeveloperToolExecutor for SubmissionTools<'_, '_> {
    fn observe_provider_profile(
        &mut self,
        profile: &peritus_model_protocol::ProviderProfile,
    ) -> Result<(), DeveloperLoopError> {
        self.base.observe_provider_profile(profile)?;
        self.page_bytes = peritus_agent::developer_tool_page_bytes(
            profile.limits().max_input_tokens(),
            ProtocolLimits::PRODUCTION,
        ).saturating_sub(self.annotation_bytes);
        Ok(())
    }

    fn observe_model_context(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.base.observe_model_context(messages)
    }

    fn effect(&self, call: &CompletedToolCall) -> DeveloperToolEffect {
        if matches!(call.name().as_str(), APPEND_TOOL | READ_TOOL | FINISH_TOOL) {
            DeveloperToolEffect::ReadOnly
        } else {
            self.base.effect(call)
        }
    }

    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        if matches!(call.name().as_str(), APPEND_TOOL | READ_TOOL | FINISH_TOOL) {
            self.execute_submission(call)
        } else {
            self.base.execute(call)
        }
    }

    fn execute_async<'a>(&'a mut self, call: &'a CompletedToolCall) -> DeveloperToolExecution<'a> {
        if matches!(call.name().as_str(), APPEND_TOOL | READ_TOOL | FINISH_TOOL) {
            Box::pin(async move { self.execute(call) })
        } else {
            self.base.execute_async(call)
        }
    }

    fn required_tool_name(&self) -> Option<&str> { self.base.required_tool_name() }
    fn completion_blocker(&self) -> Option<String> { self.base.completion_blocker() }
    fn yields_to_host(&self) -> bool { self.yield_now || self.base.yields_to_host() }
    fn take_progress_feedback(&mut self) -> Option<String> { self.base.take_progress_feedback() }
    fn continuation_blocker(&self) -> Option<String> { self.base.continuation_blocker() }
}

pub(super) fn definitions() -> Result<Vec<ToolDefinition>, DeveloperLoopError> {
    let limits = ProtocolLimits::PRODUCTION;
    [
        (
            APPEND_TOOL,
            "Append one exact decoded UTF-8 chunk to a typed reviewer field. Fields use a monotonic byte offset and generation. Set complete only on the final chunk of that field. Generation 0 creates the field; a later generation atomically replaces a completed field only after all replacement chunks arrive. This physical call does not limit cumulative review size.",
            r#"{"type":"object","additionalProperties":false,"required":["handle","path","generation","offset","text","complete"],"properties":{"handle":{"type":"string"},"path":{"type":"string","pattern":"^(summary|findings/(0|[1-9][0-9]*)/(category|severity|title|description|location|reproduction|remediation))$"},"generation":{"type":"string","pattern":"^(0|[1-9][0-9]*)$"},"offset":{"type":"string","pattern":"^(0|[1-9][0-9]*)$"},"text":{"type":"string"},"complete":{"type":"boolean"}}}"#,
        ),
        (
            READ_TOOL,
            "Read an exact UTF-8 byte page from a retained staged reviewer field. Use the handle, path and generation reported by the submission protocol. max_bytes bounds one encoded observation, not total review work; follow next_offset until null.",
            r#"{"type":"object","additionalProperties":false,"required":["handle","path","generation","offset","max_bytes"],"properties":{"handle":{"type":"string"},"path":{"type":"string","pattern":"^(summary|findings/(0|[1-9][0-9]*)/(category|severity|title|description|location|reproduction|remediation))$"},"generation":{"type":"string","pattern":"^(0|[1-9][0-9]*)$"},"offset":{"type":"string","pattern":"^(0|[1-9][0-9]*)$"},"max_bytes":{"type":"integer","minimum":1,"maximum":65536}}}"#,
        ),
        (
            FINISH_TOOL,
            "Validate and finish the retained typed review without re-emitting its fields. finding_count is an exact canonical decimal count, not a quota. Supply the current coverage_cursor to assert omission only for the indexed subset; omitting it preserves the legacy full-coverage assertion. The host applies the same typed categories, severities, required content, duplicate identity and finding-conservation semantics as a complete JSON submission.",
            r#"{"type":"object","additionalProperties":false,"required":["handle","finding_count"],"properties":{"handle":{"type":"string"},"finding_count":{"type":"string","pattern":"^(0|[1-9][0-9]*)$"},"coverage_cursor":{"type":"string","minLength":1}}}"#,
        ),
    ].into_iter().map(|(name, description, schema)| {
        Ok(ToolDefinition::new(
            ToolName::new(name.to_owned())?,
            Some(BoundedText::new(description.to_owned(), limits)?),
            JsonSchema::parse(
                schema,
                SchemaDialect::Draft202012,
                JsonBounds::schema(limits),
            )?,
            true,
        ))
    }).collect()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppendWire {
    handle: String,
    path: String,
    generation: String,
    offset: String,
    text: String,
    complete: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadWire {
    handle: String,
    path: String,
    generation: String,
    offset: String,
    max_bytes: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishWire {
    handle: String,
    finding_count: String,
    #[serde(default)]
    coverage_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct AppendAck {
    handle: String,
    path: String,
    generation: String,
    offset: String,
    end_offset: String,
    prefix_sha256: String,
    complete: bool,
    next_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FinishAck {
    handle: String,
    finding_count: String,
    submission_sha256: String,
    state: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FindingField {
    Category,
    Severity,
    Title,
    Description,
    Location,
    Reproduction,
    Remediation,
}

impl FindingField {
    const ALL: [Self; FINDING_FIELD_COUNT] = [
        Self::Category,
        Self::Severity,
        Self::Title,
        Self::Description,
        Self::Location,
        Self::Reproduction,
        Self::Remediation,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Severity => "severity",
            Self::Title => "title",
            Self::Description => "description",
            Self::Location => "location",
            Self::Reproduction => "reproduction",
            Self::Remediation => "remediation",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.as_str() == value)
    }

    const fn ordinal(self) -> usize {
        match self {
            Self::Category => 0,
            Self::Severity => 1,
            Self::Title => 2,
            Self::Description => 3,
            Self::Location => 4,
            Self::Reproduction => 5,
            Self::Remediation => 6,
        }
    }

    const fn from_ordinal(value: usize) -> Self {
        match value {
            0 => Self::Category,
            1 => Self::Severity,
            2 => Self::Title,
            3 => Self::Description,
            4 => Self::Location,
            5 => Self::Reproduction,
            _ => Self::Remediation,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FieldPath {
    Summary,
    Finding { index: usize, field: FindingField },
}

impl FieldPath {
    fn parse(value: &str) -> Result<Self, String> {
        if value == "summary" {
            return Ok(Self::Summary);
        }
        let Some(tail) = value.strip_prefix("findings/") else {
            return Err("unknown reviewer submission field path".to_owned());
        };
        let Some((index, field)) = tail.split_once('/') else {
            return Err("finding field path is incomplete".to_owned());
        };
        if field.contains('/') {
            return Err("finding field path has an unexpected component".to_owned());
        }
        let index = decimal_usize(index, "finding index")?;
        let field = FindingField::parse(field)
            .ok_or_else(|| "unknown reviewer finding field".to_owned())?;
        Ok(Self::Finding { index, field })
    }

    fn ordinal(&self) -> Result<usize, String> {
        match self {
            Self::Summary => Ok(0),
            Self::Finding { index, field } => index.checked_mul(FINDING_FIELD_COUNT)
                .and_then(|value| value.checked_add(field.ordinal()))
                .and_then(|value| value.checked_add(1))
                .ok_or_else(|| "finding field path exceeds the native address representation".to_owned()),
        }
    }

    fn from_ordinal(value: usize) -> Self {
        if value == 0 {
            Self::Summary
        } else {
            let tail = value - 1;
            Self::Finding {
                index: tail / FINDING_FIELD_COUNT,
                field: FindingField::from_ordinal(tail % FINDING_FIELD_COUNT),
            }
        }
    }

    fn as_string(&self) -> String {
        match self {
            Self::Summary => "summary".to_owned(),
            Self::Finding { index, field } => format!("findings/{index}/{}", field.as_str()),
        }
    }
}

fn append_exact(target: &mut String, offset: usize, value: &str) -> Result<usize, String> {
    if offset > target.len() || !target.is_char_boundary(offset) {
        return Err("field offset is outside the staged prefix or splits UTF-8".to_owned());
    }
    if offset < target.len() {
        return duplicate_exact(target, offset, value);
    }
    target.try_reserve(value.len()).map_err(|_| "allocate staged reviewer field".to_owned())?;
    target.push_str(value);
    Ok(target.len())
}

fn duplicate_exact(target: &str, offset: usize, value: &str) -> Result<usize, String> {
    let end = offset.checked_add(value.len()).ok_or_else(|| "field offset overflow".to_owned())?;
    if end > target.len() || !target.is_char_boundary(offset) || !target.is_char_boundary(end)
        || &target[offset..end] != value
    {
        return Err("field replay conflicts with the retained exact prefix".to_owned());
    }
    Ok(end)
}

fn decimal_u64(value: &str, label: &str) -> Result<u64, String> {
    if value.is_empty() || (value != "0" && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("{label} must be a canonical nonnegative decimal"));
    }
    value.parse().map_err(|_| format!("{label} exceeds its representation"))
}

fn decimal_usize(value: &str, label: &str) -> Result<usize, String> {
    let value = decimal_u64(value, label)?;
    usize::try_from(value).map_err(|_| format!("{label} exceeds the native address representation"))
}

fn digest(value: &str) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(value.as_bytes()) {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn encode(value: &impl Serialize) -> Result<CanonicalJson, String> {
    let encoded = serde_json::to_string(value).map_err(|error| error.to_string())?;
    CanonicalJson::parse(&encoded, JsonBounds::value(ProtocolLimits::PRODUCTION))
        .map_err(|error| error.to_string())
}

fn observation_error(
    detail: impl Into<String>,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    let encoded = serde_json::to_string(&json!({ "error": detail.into() }))
        .map_err(|error| DeveloperLoopError::Tool(error.to_string()))?;
    Ok(DeveloperToolObservation {
        output: CanonicalJson::parse(
            &encoded,
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?,
        is_error: true,
    })
}

fn recovery_error(detail: &str) -> DeveloperLoopError {
    DeveloperLoopError::Trace(detail.to_owned())
}
