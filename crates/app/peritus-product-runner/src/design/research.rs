//! Host-owned domain-assessment and retained-research state exposed to design roles.

use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::{
    BoundedText, CanonicalJson, CompletedToolCall, JsonBounds, JsonSchema, ProtocolLimits,
    SchemaDialect, ToolDefinition, ToolName,
};

use super::{
    DesignDocument, DesignExpansionOutcome, ExpansionReceiptIdentity, ExpansionRecords,
    ExpansionSnapshot, ExpansionStage, SourceDesignCaptureRequest, SourceDesignOrigin,
    SourceDesignSnapshot, advance_design_expansion, capture_design_expansion,
    capture_source_design,
    expansion::{
        answer_design_expansion_question, restore_design_expansion,
        restore_design_expansion_for_view, validate_direct_publication_records,
    },
    publish_partial_design_expansion, publish_ready_design_expansion, read_design_expansion,
};
use crate::{
    ProductRunInput, ProductRunnerError, ProductRunnerErrorKind,
    developer_tools::WorkspaceDeveloperTools,
};

pub const TOOL: &str = "design_research_state";

const FROM_CAPTURED_TO_REVIEW: &[ExpansionStage] = &[
    ExpansionStage::DomainAssessment,
    ExpansionStage::FocusedResearch,
    ExpansionStage::ExpansionDraft,
    ExpansionStage::ExpansionReview,
];
const FROM_ASSESSMENT_TO_REVIEW: &[ExpansionStage] = &[
    ExpansionStage::FocusedResearch,
    ExpansionStage::ExpansionDraft,
    ExpansionStage::ExpansionReview,
];
const FROM_RESEARCH_TO_REVIEW: &[ExpansionStage] =
    &[ExpansionStage::ExpansionDraft, ExpansionStage::ExpansionReview];
const FROM_DRAFT_TO_REVIEW: &[ExpansionStage] = &[ExpansionStage::ExpansionReview];
const ALREADY_REVIEWED: &[ExpansionStage] = &[];

fn publication_checkpoints(stage: ExpansionStage) -> Result<&'static [ExpansionStage], String> {
    match stage {
        ExpansionStage::Captured => Ok(FROM_CAPTURED_TO_REVIEW),
        ExpansionStage::DomainAssessment => Ok(FROM_ASSESSMENT_TO_REVIEW),
        ExpansionStage::FocusedResearch => Ok(FROM_RESEARCH_TO_REVIEW),
        ExpansionStage::ExpansionDraft => Ok(FROM_DRAFT_TO_REVIEW),
        ExpansionStage::ExpansionReview => Ok(ALREADY_REVIEWED),
        ExpansionStage::WaitingForUser => {
            Err("answer the retained material question before publishing the design".to_owned())
        }
        ExpansionStage::ResearchLimited => Err(
            "resolve or explicitly resume the retained research limitation before publishing the design"
                .to_owned(),
        ),
        ExpansionStage::Ready => Err("the design expansion is already published".to_owned()),
        ExpansionStage::Invalidated => {
            Err("an invalidated design expansion cannot be published".to_owned())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignView {
    Expanded,
    Source,
    Changes,
}

pub struct ResearchSession {
    source: Option<SourceDesignSnapshot>,
    expansion: Option<ExpansionSnapshot>,
    limitation: Option<String>,
    progress_feedback: Option<String>,
}

impl ResearchSession {
    /// Reads an already retained design for a deterministic presentation command.
    ///
    /// Presentation is non-mutating. It preserves the historical source/manifest even after the
    /// implementation changes repository evidence, while reuse and execution retain their normal
    /// current-evidence validation. A guided draft without an expansion remains readable but is
    /// never silently captured or promoted by this path.
    pub(crate) fn retained_view(
        input: &ProductRunInput,
        view: DesignView,
    ) -> Result<Option<serde_json::Value>, ProductRunnerError> {
        match restore_design_expansion_for_view(input) {
            Ok((source, expansion)) => {
                let session = Self {
                    source: Some(source),
                    expansion: Some(expansion),
                    limitation: None,
                    progress_feedback: None,
                };
                return session.view(input, view).map(Some).map_err(|detail| {
                    ProductRunnerError::new(
                        ProductRunnerErrorKind::Repository,
                        "read retained design view",
                        detail,
                    )
                });
            }
            Err(error) => {
                let (source_retained, expansion_retained) =
                    retained_artifact_state(&input.trace_path)?;
                if expansion_retained {
                    return Err(error);
                }
                if source_retained {
                    let source = SourceDesignSnapshot::open(
                        &input.trace_path,
                        input.run_id,
                        input.workspace_id,
                    )?;
                    return Ok(Some(match view {
                        DesignView::Source => serde_json::json!({
                            "view":"source",
                            "markdown":source.text(),
                            "source_sha256":crate::encode_hex(source.metadata().sha256().as_bytes()),
                            "source_input_revision":source.metadata().input_revision(),
                        }),
                        DesignView::Expanded => serde_json::json!({
                            "view":"expanded",
                            "markdown":source.text(),
                            "expansion_version":null,
                            "ready":false,
                        }),
                        DesignView::Changes => serde_json::json!({
                            "view":"changes",
                            "markdown":"# Design changes\n\nNo domain expansion has been recorded yet.\n",
                            "expansion_version":null,
                        }),
                    }));
                }
            }
        }
        let Some(draft) = crate::guided_design::load(input)? else { return Ok(None) };
        let value = match view {
            DesignView::Expanded => serde_json::json!({
                "view":"expanded",
                "markdown":draft.markdown,
                "expansion_version":null,
                "ready":false,
            }),
            DesignView::Source => serde_json::json!({
                "view":"source",
                "markdown":draft.markdown,
                "source_sha256":crate::encode_hex(peritus_codec::sha256(draft.markdown.as_bytes()).as_bytes()),
                "source_input_revision":draft.input_revision(),
            }),
            DesignView::Changes => serde_json::json!({
                "view":"changes",
                "markdown":"# Design changes\n\nNo domain expansion has been recorded yet.\n",
                "expansion_version":null,
            }),
        };
        Ok(Some(value))
    }

    pub fn automatic(input: &ProductRunInput) -> Result<Self, ProductRunnerError> {
        if let Ok((source, expansion)) = restore_design_expansion(input) {
            let revision = input.conversation.incorporated_revision();
            if expansion.input_revision() == revision
                || expansion.stage() == ExpansionStage::WaitingForUser
                    && revision > expansion.input_revision()
            {
                return Ok(Self {
                    source: Some(source),
                    expansion: Some(expansion),
                    limitation: None,
                    progress_feedback: None,
                });
            }
        }
        Self::capture(input, input.conversation.requests_ready_design())
    }

    pub fn interactive(input: &ProductRunInput) -> Self {
        let prefer_guided = crate::guided_design::load(input).ok().flatten().is_some();
        if let Ok((source, expansion)) = restore_design_expansion(input) {
            let retained_guided_source_is_current = source.metadata().origin()
                == SourceDesignOrigin::SavedGuidedDesign
                && crate::guided_design::load(input).ok().flatten().is_some_and(|draft| {
                    peritus_codec::sha256(draft.markdown.as_bytes()) == source.metadata().sha256()
                });
            if expansion.input_revision() == input.conversation.incorporated_revision()
                || retained_guided_source_is_current
            {
                return Self {
                    source: Some(source),
                    expansion: Some(expansion),
                    limitation: None,
                    progress_feedback: None,
                };
            }
        }
        Self::capture(input, prefer_guided).unwrap_or_else(|error| Self {
            source: None,
            expansion: None,
            limitation: Some(format!(
                "exact source-design capture is unavailable: {}",
                error.detail()
            )),
            progress_feedback: None,
        })
    }

    fn capture(input: &ProductRunInput, prefer_guided: bool) -> Result<Self, ProductRunnerError> {
        super::rollout::require_new_expansion(&input.trace_path)?;
        let revision = input.conversation.incorporated_revision();
        let request = if prefer_guided {
            SourceDesignCaptureRequest::saved_guided_design(revision)
        } else {
            SourceDesignCaptureRequest::direct_user_text(revision)
        };
        let source = capture_source_design(input, request)?;
        crate::efficiency::event(
            &input.trace_path,
            serde_json::json!({
                "event":"design-source-captured",
                "origin":source.metadata().origin(),
                "input_revision":source.metadata().input_revision(),
                "bytes":source.metadata().bytes(),
                "sha256":crate::encode_hex(source.metadata().sha256().as_bytes()),
            }),
        );
        let expansion = capture_design_expansion(input, &source)?;
        expansion_event(input, "design-expansion-captured", &expansion);
        Ok(Self {
            source: Some(source),
            expansion: Some(expansion),
            limitation: None,
            progress_feedback: None,
        })
    }

    pub fn refresh_interactive(&mut self, input: &ProductRunInput) {
        *self = Self::interactive(input);
    }

    pub fn context(&self) -> String {
        match (&self.expansion, &self.limitation) {
            (Some(expansion), _) => format!(
                "\n\nHost domain-expansion state (metadata and ledger only; source bodies remain tool evidence):\n{}",
                expansion.observation()
            ),
            (_, Some(limitation)) => format!(
                "\n\nHost domain-expansion limitation: {limitation}. Do not claim an exact retained research ledger is complete."
            ),
            _ => String::new(),
        }
    }

    pub(crate) fn pending_question(&self) -> Option<(String, u64)> {
        self.expansion.as_ref().and_then(|expansion| {
            expansion
                .material_question()
                .map(|question| (question.message(), expansion.input_revision()))
        })
    }

    pub(crate) fn view(
        &self,
        input: &ProductRunInput,
        view: DesignView,
    ) -> Result<serde_json::Value, String> {
        match view {
            DesignView::Source => {
                let source = self.source.as_ref().ok_or_else(|| {
                    self.limitation.clone().unwrap_or_else(|| "source unavailable".to_owned())
                })?;
                Ok(serde_json::json!({
                    "view":"source",
                    "markdown":source.text(),
                    "source_sha256":crate::encode_hex(source.metadata().sha256().as_bytes()),
                    "source_input_revision":source.metadata().input_revision(),
                }))
            }
            DesignView::Expanded => {
                if let Some(expansion) = &self.expansion
                    && let Some((bytes, digest)) = expansion.ready_design_identity()
                {
                    let markdown =
                        std::fs::read_to_string(input.trace_path.with_extension("design.md"))
                            .map_err(|error| error.to_string())?;
                    if markdown.len() as u64 != bytes
                        || peritus_codec::sha256(markdown.as_bytes()) != digest
                    {
                        return Err("expanded design changed after publication".to_owned());
                    }
                    return Ok(serde_json::json!({
                        "view":"expanded",
                        "markdown":markdown,
                        "expansion_version":expansion.version(),
                        "ready":true,
                    }));
                }
                let draft =
                    crate::guided_design::load(input)
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "no expanded or guided design is saved yet".to_owned())?;
                Ok(serde_json::json!({
                    "view":"expanded",
                    "markdown":draft.markdown,
                    "expansion_version":self.expansion.as_ref().map(ExpansionSnapshot::version),
                    "ready":false,
                }))
            }
            DesignView::Changes => {
                let expansion = self.expansion.as_ref().ok_or_else(|| {
                    self.limitation.clone().unwrap_or_else(|| "expansion unavailable".to_owned())
                })?;
                Ok(serde_json::json!({
                    "view":"changes",
                    "markdown":expansion.changes_markdown(),
                    "expansion_version":expansion.version(),
                }))
            }
        }
    }

    pub fn execute(
        &mut self,
        input: &ProductRunInput,
        call: &CompletedToolCall,
        grounding_markdown: &str,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let result = self.execute_inner(input, call, grounding_markdown);
        let is_error = result.is_err();
        let value = result.unwrap_or_else(|error| serde_json::json!({"error":error}));
        Ok(DeveloperToolObservation {
            output: CanonicalJson::parse(
                &value.to_string(),
                JsonBounds::value(ProtocolLimits::PRODUCTION),
            )?,
            is_error,
        })
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the state-tool transaction keeps argument, permission, CAS, and transition order visible"
    )]
    fn execute_inner(
        &mut self,
        input: &ProductRunInput,
        call: &CompletedToolCall,
        grounding_markdown: &str,
    ) -> Result<serde_json::Value, String> {
        crate::execution::check_cancelled(input).map_err(|error| error.to_string())?;
        if !input
            .conversation
            .effective_permissions()
            .allows(crate::control::PermissionCapability::Read)
        {
            return Err("design research is disabled by the current read permission".to_owned());
        }
        let arguments: Arguments = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| error.to_string())?;
        let source = self.source.as_ref().ok_or_else(|| {
            self.limitation.clone().unwrap_or_else(|| "source unavailable".into())
        })?;
        let current = self.expansion.as_ref().ok_or_else(|| "expansion unavailable".to_owned())?;
        if arguments.action == Action::Observe {
            if arguments.expected_version.is_some()
                || arguments.records.is_some()
                || arguments.markdown.is_some()
            {
                return Err(
                    "observe requires expected_version=null, records=null, and markdown=null"
                        .to_owned(),
                );
            }
            return Ok(current.observation());
        }
        if arguments.expected_version != Some(current.version()) {
            return Err("design expansion changed; observe the current version before updating it"
                .to_owned());
        }
        if arguments.action == Action::RestartFromCurrentInput {
            if arguments.records.is_some() || arguments.markdown.is_some() {
                return Err(
                    "restart_from_current_input requires records=null and markdown=null".to_owned()
                );
            }
            if current.stage() != ExpansionStage::WaitingForUser
                || input.conversation.incorporated_revision() <= current.input_revision()
            {
                return Err(
                    "restart_from_current_input requires newer steering while waiting for a material answer"
                        .to_owned(),
                );
            }
            let replacement = Self::capture(input, input.conversation.requests_ready_design())
                .map_err(|error| error.to_string())?;
            *self = replacement;
            if let Some(expansion) = &self.expansion {
                expansion_event(input, "design-expansion-restarted", expansion);
            }
            return Ok(self
                .expansion
                .as_ref()
                .expect("captured replacement expansion")
                .observation());
        }
        let records = arguments
            .records
            .ok_or_else(|| "a state transition requires complete structured records".to_owned())?;
        let records =
            serde_json::to_vec(&records).map_err(|error| error.to_string()).and_then(|bytes| {
                ExpansionRecords::from_json(&bytes).map_err(|error| error.to_string())
            })?;
        if arguments.action != Action::PublishReady && arguments.markdown.is_some() {
            return Err("markdown is accepted only by publish_ready".to_owned());
        }
        if arguments.action == Action::PublishReady {
            let markdown = arguments
                .markdown
                .as_deref()
                .ok_or_else(|| "publish_ready requires the complete base Markdown".to_owned())?;
            let markdown = super::normalize(markdown).map_err(|error| error.detail().to_owned())?;
            validate_direct_publication_records(input, source, current, &records)
                .map_err(|error| error.to_string())?;
            let mut predecessor = current.clone();
            for stage in publication_checkpoints(predecessor.stage())? {
                let next = advance_design_expansion(
                    input,
                    source,
                    &predecessor,
                    *stage,
                    None,
                    records.clone(),
                )
                .map_err(|error| error.to_string())?;
                self.expansion = Some(next.clone());
                expansion_event(input, "design-expansion-transition", &next);
                predecessor = next;
            }
            let next = publish_ready_design_expansion(
                input,
                source,
                &predecessor,
                records,
                &markdown,
                grounding_markdown,
            )
            .map_err(|error| error.to_string())?;
            self.expansion = Some(next);
            expansion_event(
                input,
                "design-expansion-ready",
                self.expansion.as_ref().expect("set above"),
            );
            return Ok(self.expansion.as_ref().expect("set above").observation());
        }
        let (stage, resume) = match arguments.action {
            Action::DomainAssessment => (ExpansionStage::DomainAssessment, None),
            Action::FocusedResearch => (ExpansionStage::FocusedResearch, None),
            Action::ExpansionDraft => (ExpansionStage::ExpansionDraft, None),
            Action::ExpansionReview => (ExpansionStage::ExpansionReview, None),
            Action::ResearchLimited => {
                let resume = if current.stage().is_resumable_primary() {
                    current.stage()
                } else {
                    current
                        .resume_stage()
                        .ok_or_else(|| "research limitation has no resumable stage".to_owned())?
                };
                (ExpansionStage::ResearchLimited, Some(resume))
            }
            Action::WaitForUser => {
                let resume = if current.stage().is_resumable_primary() {
                    current.stage()
                } else {
                    return Err(
                        "only a primary expansion stage can ask a material question".to_owned()
                    );
                };
                (ExpansionStage::WaitingForUser, Some(resume))
            }
            Action::Resume if current.stage() == ExpansionStage::WaitingForUser => {
                let revision = input.conversation.incorporated_revision();
                let answer = input
                    .conversation
                    .source_design_user_text(revision)
                    .map_err(|error| format!("material answer is unavailable: {error}"))?
                    .ok_or_else(|| "exact material answer is unavailable".to_owned())?;
                let next =
                    answer_design_expansion_question(input, source, current, records, &answer)
                        .map_err(|error| error.to_string())?;
                self.expansion = Some(next);
                expansion_event(
                    input,
                    "design-question-answered",
                    self.expansion.as_ref().expect("set above"),
                );
                return Ok(self.expansion.as_ref().expect("set above").observation());
            }
            Action::Resume => (
                current
                    .resume_stage()
                    .ok_or_else(|| "current expansion is not resumable".to_owned())?,
                None,
            ),
            Action::Observe | Action::PublishReady | Action::RestartFromCurrentInput => {
                unreachable!("handled above")
            }
        };
        let previous_counts = coverage(current.observation().get("records"));
        let next = advance_design_expansion(input, source, current, stage, resume, records)
            .map_err(|error| error.to_string())?;
        let next_counts = coverage(next.observation().get("records"));
        if next_counts <= previous_counts && stage == ExpansionStage::FocusedResearch {
            self.progress_feedback = Some(
                "The research transition added no obligation, evidence, decision, or limitation coverage. Refine the source strategy, synthesize supported obligations, or retain a precise gap instead of repeating reads.".to_owned(),
            );
        }
        self.expansion = Some(next);
        let event = if stage == ExpansionStage::WaitingForUser {
            "design-question-waiting"
        } else {
            "design-expansion-transition"
        };
        expansion_event(input, event, self.expansion.as_ref().expect("set above"));
        Ok(self.expansion.as_ref().expect("set above").observation())
    }

    pub const fn take_progress_feedback(&mut self) -> Option<String> {
        self.progress_feedback.take()
    }

    fn completion_blocker(&self, input: &ProductRunInput) -> Option<String> {
        let source = self.source.as_ref()?;
        if let Err(error) = read_design_expansion(input, source) {
            return Some(format!(
                "Revalidate or replace stale design research evidence before returning: {}",
                error.detail()
            ));
        }
        let stage = self.expansion.as_ref()?.stage();
        (!matches!(
            stage,
            ExpansionStage::Ready
                | ExpansionStage::WaitingForUser
                | ExpansionStage::ResearchLimited
        ))
        .then(|| {
            "Publish complete records and base Markdown with publish_ready before returning; the host validates and durably records any missing primary checkpoints in order. If critical evidence is unavailable, retain a precise research_limited state instead.".to_owned()
        })
    }

    fn finish(
        &mut self,
        input: &ProductRunInput,
        model_text: &str,
        grounding_markdown: &str,
    ) -> Result<DesignExpansionOutcome, ProductRunnerError> {
        let source = self.source.as_ref().ok_or_else(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "finish domain-aware design expansion",
                "exact source design is unavailable",
            )
        })?;
        let current = self.expansion.as_ref().ok_or_else(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "finish domain-aware design expansion",
                "design expansion is unavailable",
            )
        })?;
        match current.stage() {
            ExpansionStage::Ready => {
                let committed = read_design_expansion(input, source)?;
                let path = input.trace_path.with_extension("design.md");
                let markdown = std::fs::read_to_string(&path).map_err(|error| {
                    ProductRunnerError::new(
                        ProductRunnerErrorKind::Repository,
                        "read expanded implementation design",
                        error.to_string(),
                    )
                })?;
                let Some((bytes, digest)) = committed.ready_design_identity() else {
                    return Err(ProductRunnerError::new(
                        ProductRunnerErrorKind::Repository,
                        "read expanded implementation design",
                        "ready expansion omitted its design identity",
                    ));
                };
                if bytes != markdown.len() as u64
                    || digest != peritus_codec::sha256(markdown.as_bytes())
                {
                    return Err(ProductRunnerError::new(
                        ProductRunnerErrorKind::Repository,
                        "read expanded implementation design",
                        "expanded design changed after ready publication",
                    ));
                }
                let identity =
                    ExpansionReceiptIdentity::ready(input, source, &committed, &markdown)?;
                let document = DesignDocument::restored_expanded(
                    path,
                    markdown,
                    committed.input_revision(),
                    identity,
                );
                if let Err(error) = super::reuse::save(input, &document) {
                    eprintln!("peritus: expanded design reuse receipt unavailable: {error}");
                }
                Ok(DesignExpansionOutcome::Ready(Box::new(document)))
            }
            ExpansionStage::WaitingForUser => {
                let question = current.material_question().ok_or_else(|| {
                    ProductRunnerError::new(
                        ProductRunnerErrorKind::Repository,
                        "finish domain-aware design expansion",
                        "waiting expansion omitted its validated material question",
                    )
                })?;
                Ok(DesignExpansionOutcome::Waiting {
                    question: question.message(),
                    conversation_revision: current.input_revision(),
                })
            }
            ExpansionStage::ResearchLimited => {
                if !current.has_partial_design() {
                    let markdown = super::normalize(model_text)?;
                    let next = publish_partial_design_expansion(
                        input,
                        source,
                        current,
                        current.records(),
                        &markdown,
                        grounding_markdown,
                    )?;
                    self.expansion = Some(next);
                }
                Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::InvalidPrecondition,
                    "finish domain-aware design expansion",
                    format!(
                        "critical research remains unavailable; the inspectable partial design is {} and cannot enter implementation",
                        input.trace_path.with_extension("design.partial.md").display(),
                    ),
                ))
            }
            _ => Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "finish domain-aware design expansion",
                "designer returned before publishing a reviewed ready expansion",
            )),
        }
    }
}

fn retained_artifact_state(
    trace_path: &std::path::Path,
) -> Result<(bool, bool), ProductRunnerError> {
    let mut retained = [false; 2];
    for (index, path) in [
        trace_path.with_extension("design-source.json"),
        trace_path.with_extension("design-expansion.json"),
    ]
    .into_iter()
    .enumerate()
    {
        match std::fs::symlink_metadata(path) {
            Ok(_) => retained[index] = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::Repository,
                    "inspect retained design view",
                    error.to_string(),
                ));
            }
        }
    }
    Ok(retained.into())
}

fn expansion_event(input: &ProductRunInput, event: &str, expansion: &ExpansionSnapshot) {
    crate::efficiency::event(
        &input.trace_path,
        serde_json::json!({
            "event":event,
            "stage":expansion.stage(),
            "version":expansion.version(),
            "input_revision":expansion.input_revision(),
            "manifest_sha256":crate::encode_hex(expansion.digest().as_bytes()),
        }),
    );
}

fn coverage(records: Option<&serde_json::Value>) -> (usize, usize, usize, usize) {
    let length = |name: &str| {
        records
            .and_then(|records| records.get(name))
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len)
    };
    (length("domains"), length("evidence"), length("obligations"), length("limitations"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Observe,
    DomainAssessment,
    FocusedResearch,
    ExpansionDraft,
    ExpansionReview,
    PublishReady,
    ResearchLimited,
    WaitForUser,
    Resume,
    RestartFromCurrentInput,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: Action,
    expected_version: Option<u64>,
    records: Option<serde_json::Value>,
    markdown: Option<String>,
}

pub fn definition() -> Result<ToolDefinition, DeveloperLoopError> {
    let limits = ProtocolLimits::PRODUCTION;
    let schema = r#"{
      "type":"object",
      "properties":{
        "action":{"type":"string","enum":["observe","domain_assessment","focused_research","expansion_draft","expansion_review","publish_ready","research_limited","wait_for_user","resume","restart_from_current_input"]},
        "expected_version":{"anyOf":[{"type":"null"},{"type":"integer","minimum":1}]},
        "markdown":{"anyOf":[{"type":"null"},{"type":"string","minLength":1,"maxLength":1048576}]},
        "records":{"anyOf":[
          {"type":"null"},
          {"type":"object","properties":{
            "domains":{"type":"array","items":{"type":"object","properties":{
              "id":{"type":"string"},"discipline":{"type":"string"},"applicability":{"type":"string"},
              "expected_sources":{"type":"array","items":{"type":"string"}},"risks":{"type":"array","items":{"type":"string"}},
              "research_required":{"type":"boolean"},"external_authority_required":{"type":"boolean"},
              "internal_mechanical":{"anyOf":[{"type":"null"},{"type":"object","properties":{"rationale":{"type":"string"},"repository_evidence_ids":{"type":"array","items":{"type":"string"}}},"required":["rationale","repository_evidence_ids"],"additionalProperties":false}]},
              "research_skip_rationale":{"anyOf":[{"type":"null"},{"type":"string"}]}
            },"required":["id","discipline","applicability","expected_sources","risks","research_required","external_authority_required","internal_mechanical","research_skip_rationale"],"additionalProperties":false}},
            "evidence":{"type":"array","items":{"type":"object","properties":{
              "id":{"type":"string"},"domain_ids":{"type":"array","items":{"type":"string"}},"obligation_ids":{"type":"array","items":{"type":"string"}},
              "title":{"type":"string"},"version":{"anyOf":[{"type":"null"},{"type":"string"}]},
              "authority":{"type":"string","enum":["source_design","repository","primary","official","engineering_rationale"]},
              "reference":{"anyOf":[
                {"type":"object","properties":{"kind":{"type":"string","enum":["source_range"]},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}},"required":["kind","start","end"],"additionalProperties":false},
                {"type":"object","properties":{"kind":{"type":"string","enum":["repository"]},"path":{"type":"string"},"sha256":{"type":"string","minLength":64,"maxLength":64}},"required":["kind","path","sha256"],"additionalProperties":false},
                {"type":"object","properties":{"kind":{"type":"string","enum":["web"]},"source":{"type":"string","minLength":64,"maxLength":64},"requested_url":{"type":"string"},"final_url":{"type":"string"},"fetched_at_unix":{"type":"integer","minimum":1},"text_sha256":{"type":"string","minLength":64,"maxLength":64},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}},"required":["kind","source","requested_url","final_url","fetched_at_unix","text_sha256","start","end"],"additionalProperties":false},
                {"type":"object","properties":{"kind":{"type":"string","enum":["engineering_rationale"]},"invariant":{"type":"string"}},"required":["kind","invariant"],"additionalProperties":false}
              ]}
            },"required":["id","domain_ids","obligation_ids","title","version","authority","reference"],"additionalProperties":false}},
            "obligations":{"type":"array","items":{"type":"object","properties":{
              "id":{"type":"string"},"class":{"type":"string","enum":["user","mandatory_inferred","supporting_change","material_recommendation","optional","conflict","unknown"]},
              "statement":{"type":"string"},"origin":{"type":"string"},"rationale":{"type":"string"},"applicability":{"type":"string"},
              "affected_contracts":{"type":"array","items":{"type":"string"}},"failure_behavior":{"type":"string"},
              "evidence_ids":{"type":"array","items":{"type":"string"}},"acceptance_ids":{"type":"array","items":{"type":"string"}},
              "status":{"type":"string","enum":["proposed","supported","rejected","unresolved"]}
            },"required":["id","class","statement","origin","rationale","applicability","affected_contracts","failure_behavior","evidence_ids","acceptance_ids","status"],"additionalProperties":false}},
            "acceptance_mappings":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"criterion":{"type":"string"},"obligation_ids":{"type":"array","items":{"type":"string"}}},"required":["id","criterion","obligation_ids"],"additionalProperties":false}},
            "material_question":{"anyOf":[
              {"type":"null"},
              {"type":"object","properties":{
                "id":{"type":"string"},"source_sha256":{"type":"string","minLength":64,"maxLength":64},
                "expansion_version":{"type":"integer","minimum":1},"input_revision":{"type":"integer","minimum":1},
                "reason":{"type":"string","enum":["conflicting_explicit_requirements","material_product_outcome","irreversible_data_or_compatibility","material_external_cost","user_only_authority_or_policy"]},
                "unresolved_decision":{"type":"string"},"evidence_checked":{"type":"array","minItems":1,"items":{"type":"string"}},
                "why_no_supported_default":{"type":"string"},
                "alternatives_and_consequences":{"type":"array","minItems":2,"items":{"type":"string"}},
                "recommendation":{"anyOf":[{"type":"null"},{"type":"string"}]},"public_question":{"type":"string"}
              },"required":["id","source_sha256","expansion_version","input_revision","reason","unresolved_decision","evidence_checked","why_no_supported_default","alternatives_and_consequences","recommendation","public_question"],"additionalProperties":false}
            ]},"expanded_markdown":{"type":"null"},"partial_markdown":{"type":"null"},
            "limitations":{"type":"array","items":{"type":"string"}},
            "change_ledger":{"type":"array","items":{"type":"object","properties":{
              "id":{"type":"string"},"kind":{"type":"string","enum":["preserved","clarified","added","conflicting","recommended"]},
              "source_evidence_ids":{"type":"array","items":{"type":"string"}},"obligation_ids":{"type":"array","items":{"type":"string"}},"summary":{"type":"string"}
            },"required":["id","kind","source_evidence_ids","obligation_ids","summary"],"additionalProperties":false}}
          },"required":["domains","evidence","obligations","acceptance_mappings","material_question","expanded_markdown","partial_markdown","limitations","change_ledger"],"additionalProperties":false}
        ]}
      },
      "required":["action","expected_version","records","markdown"],"additionalProperties":false
    }"#;
    Ok(ToolDefinition::new(
        ToolName::new(TOOL.to_owned())?,
        Some(BoundedText::new(
            "Observe or advance the host-owned design-expansion ledger. Always send markdown=null except for publish_ready, and send the COMPLETE records object on every transition: {domains,evidence,obligations,acceptance_mappings,material_question,expanded_markdown,partial_markdown,limitations,change_ledger}. restart_from_current_input is the sole exception and requires records=null, markdown=null. expanded_markdown and partial_markdown are always null because artifact identities are host-owned. A domain is {id,discipline,applicability,expected_sources,risks,research_required,external_authority_required,internal_mechanical,research_skip_rationale}; internal_mechanical is null or {rationale,repository_evidence_ids}. Evidence is {id,domain_ids,obligation_ids,title,version,authority,reference}; authority is source_design, repository, primary, official, or engineering_rationale. References use kind=source_range with start/end; kind=repository with path/sha256; kind=web with source/requested_url/final_url/fetched_at_unix/text_sha256/start/end; or kind=engineering_rationale with invariant. SHA-256 values are lowercase 64-character strings. An obligation is {id,class,statement,origin,rationale,applicability,affected_contracts,failure_behavior,evidence_ids,acceptance_ids,status}; class is user, mandatory_inferred, supporting_change, material_recommendation, optional, conflict, or unknown; status is proposed, supported, rejected, or unresolved. Every user, mandatory_inferred, and supporting_change obligation must be supported, map bidirectionally to evidence and observable acceptance IDs, name affected contracts and failure behavior, and appear in the typed change ledger. Change entries are {id,kind,source_evidence_ids,obligation_ids,summary}; kind is preserved, clarified, added, conflicting, or recommended. User requirements require exact source-range evidence and preserved/clarified entries. Inferred requirements use added/clarified; recommendations use recommended and never become user requirements. Do not ask when repository evidence, authoritative guidance, or a safe reversible default resolves the design. Only wait_for_user for an evidence-linked unresolved conflict/unknown in one of the closed material reason classes; include at least two concrete alternatives and consequences, plus an optional recommendation. Set the question expansion_version to the next transition version. After the exact answer is incorporated, use resume with the complete records and material_question=null; the host binds the answer to the original source and exact question. If the newer user input revises the source or scope instead of answering, use restart_from_current_input so the prior question cannot consume unrelated steering. When complete records and the full base Markdown are available, call publish_ready directly; the host validates and durably records every missing primary checkpoint in order. Use individual primary transitions only to retain incomplete recoverable work. The host appends provenance and atomically binds the final Markdown. Search snippets are discovery only; externally derived material claims require an opened primary or official retained source. Use research_limited with nonempty limitations when critical evidence is unavailable; its final response is retained as a partial design but cannot enter implementation. This private state tool grants no workspace mutation, process, pipeline, or network authority.".to_owned(),
            limits,
        )?),
        JsonSchema::parse(schema, SchemaDialect::Draft202012, JsonBounds::schema(limits))?,
        true,
    ))
}

pub struct DesignTools<'a> {
    pub(crate) workspace: WorkspaceDeveloperTools,
    research: ResearchSession,
    input: &'a ProductRunInput,
}

impl<'a> DesignTools<'a> {
    pub fn new(
        input: &'a ProductRunInput,
        workspace: WorkspaceDeveloperTools,
    ) -> Result<Self, ProductRunnerError> {
        Ok(Self { workspace, research: ResearchSession::automatic(input)?, input })
    }

    pub fn context(&self) -> String {
        self.research.context()
    }

    pub fn finish(
        &mut self,
        model_text: &str,
    ) -> Result<DesignExpansionOutcome, ProductRunnerError> {
        self.workspace.grounding().validate().map_err(super::grounding)?;
        self.research.finish(self.input, model_text, &self.workspace.grounding().markdown())
    }
}

impl DeveloperToolExecutor for DesignTools<'_> {
    fn take_host_tool_calls(&mut self) -> u32 {
        self.workspace.take_host_tool_calls()
    }
    fn system_guidance(&mut self) -> Option<String> {
        self.workspace.system_guidance()
    }
    fn observe_model_context(
        &mut self,
        messages: &[peritus_model_protocol::Message],
    ) -> Result<(), DeveloperLoopError> {
        self.workspace.observe_model_context(messages)
    }
    fn effect(&self, call: &CompletedToolCall) -> DeveloperToolEffect {
        if call.name().as_str() == TOOL {
            DeveloperToolEffect::MutationCapable
        } else {
            self.workspace.effect(call)
        }
    }
    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        if call.name().as_str() == TOOL {
            self.workspace
                .grounding()
                .validate()
                .map_err(|detail| DeveloperLoopError::Tool(detail.to_owned()))?;
            let grounding = self.workspace.grounding().markdown();
            self.research.execute(self.input, call, &grounding)
        } else {
            self.workspace.execute(call)
        }
    }
    fn reuse_workspace_read(
        &mut self,
        call: &CompletedToolCall,
        observation: &DeveloperToolObservation,
    ) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
        self.workspace.reuse_workspace_read(call, observation)
    }
    fn observation_images(
        &mut self,
        call: &CompletedToolCall,
        observation: &DeveloperToolObservation,
    ) -> Result<Vec<peritus_model_protocol::MediaInput>, DeveloperLoopError> {
        self.workspace.observation_images(call, observation)
    }
    fn completion_blocker(&self) -> Option<String> {
        self.workspace.completion_blocker().or_else(|| self.research.completion_blocker(self.input))
    }
    fn required_tool_name(&self) -> Option<&str> {
        self.workspace.required_tool_name()
    }
    fn tool_calls_allowed(&self) -> bool {
        self.workspace.tool_calls_allowed()
    }
    fn take_progress_feedback(&mut self) -> Option<String> {
        self.research.take_progress_feedback().or_else(|| self.workspace.take_progress_feedback())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_view_distinguishes_absent_source_only_and_corrupt_expansion_state() {
        let directory = tempfile::tempdir().expect("temporary design state");
        let trace = directory.path().join("run.trace");
        assert_eq!(retained_artifact_state(&trace).expect("inspect empty state"), (false, false));

        std::fs::write(trace.with_extension("design-source.json"), b"source")
            .expect("write retained source marker");
        assert_eq!(
            retained_artifact_state(&trace).expect("inspect source-only state"),
            (true, false),
            "a committed source without an expansion is a recoverable publication boundary"
        );

        std::fs::write(trace.with_extension("design-expansion.json"), b"corrupt")
            .expect("write corrupt expansion marker");
        assert_eq!(
            retained_artifact_state(&trace).expect("inspect retained corrupt state"),
            (true, true),
            "a retained expansion must surface its restore error, not fall back to a draft"
        );
    }

    #[test]
    fn stage_six_tool_has_strict_questions_restart_and_publication_schema() {
        let definition = definition().expect("valid research tool definition");
        assert_eq!(definition.name().as_str(), TOOL);
        let encoded = definition.parameters().canonical_bytes();
        let schema: serde_json::Value =
            serde_json::from_slice(encoded).expect("canonical JSON schema");
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema.to_string().contains("external_authority_required"));
        assert!(schema.to_string().contains("text_sha256"));
        assert!(schema.to_string().contains("publish_ready"));
        assert!(schema.to_string().contains("partial_markdown"));
        assert!(schema.to_string().contains("source_evidence_ids"));
        assert!(schema.to_string().contains("wait_for_user"));
        assert!(schema.to_string().contains("restart_from_current_input"));
        assert!(schema.to_string().contains("material_product_outcome"));
        assert!(schema.to_string().contains("alternatives_and_consequences"));
    }

    #[test]
    fn direct_publication_preserves_every_missing_primary_checkpoint() {
        assert_eq!(
            publication_checkpoints(ExpansionStage::Captured).expect("captured path"),
            FROM_CAPTURED_TO_REVIEW
        );
        assert_eq!(
            publication_checkpoints(ExpansionStage::DomainAssessment).expect("assessment path"),
            FROM_ASSESSMENT_TO_REVIEW
        );
        assert_eq!(
            publication_checkpoints(ExpansionStage::FocusedResearch).expect("research path"),
            FROM_RESEARCH_TO_REVIEW
        );
        assert_eq!(
            publication_checkpoints(ExpansionStage::ExpansionDraft).expect("draft path"),
            FROM_DRAFT_TO_REVIEW
        );
        assert!(
            publication_checkpoints(ExpansionStage::ExpansionReview)
                .expect("reviewed path")
                .is_empty()
        );
        for stage in [
            ExpansionStage::WaitingForUser,
            ExpansionStage::ResearchLimited,
            ExpansionStage::Ready,
            ExpansionStage::Invalidated,
        ] {
            assert!(publication_checkpoints(stage).is_err(), "{stage:?}");
        }
    }
}
