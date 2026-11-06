//! Mandatory repository-grounded design document produced before implementation.

mod artifact;
mod expansion;
mod expert_pass;
mod expert_tools;
#[cfg(test)]
mod qualification;
pub mod research;
mod retention;
mod reuse;
mod rollout;
mod source;

pub use expert_tools::{
    context as expert_context, definitions as expert_definitions, execute as execute_expert,
    owns as expert_tool,
};
pub const fn expert_policy() -> &'static str {
    expert_tools::POLICY
}

pub fn expert_context_scope(
    input: &ProductRunInput,
    source: &SourceDesignSnapshot,
) -> Result<String, ProductRunnerError> {
    let pass = expert_pass::ExpertPass::load(input, source)?;
    Ok(pass.unresolved().first().and_then(|q| q["id"].as_str()).map_or_else(
        || {
            if pass.domains.is_empty() && pass.imported_records.is_none() {
                "intake"
            } else {
                "review"
            }
            .to_owned()
        },
        str::to_owned,
    ))
}

use crate::engineering_workflow::vitamins::DOMAIN_DESIGN_POLICY_VERSION as DESIGN_EXPANSION_POLICY_VERSION;
pub use expansion::{
    ExpansionRecords, ExpansionSnapshot, ExpansionStage, advance_design_expansion,
    capture_design_expansion, publish_partial_design_expansion, publish_ready_design_expansion,
    read_design_expansion,
};

pub use source::{
    MAX_SOURCE_DESIGN_BYTES, MAX_SOURCE_DESIGN_PAGE_BYTES, SourceDesignAttachment,
    SourceDesignCaptureRequest, SourceDesignMetadata, SourceDesignOrigin, SourceDesignPage,
    SourceDesignPrior, SourceDesignSnapshot, SourceDesignWorkspace, capture_source_design,
};

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_provider_core::ModelProvider;
use serde::{Deserialize, Serialize};

use crate::budget::RunAccounting;
use crate::developer_tools::read_only_definitions;
use crate::execution::{ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAXIMUM_DESIGN_BYTES: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DesignScope {
    Artifact,
    Source,
}

/// Detailed design artifact and conversation revision it covers.
#[derive(Clone)]
pub struct DesignDocument {
    path: PathBuf,
    markdown: String,
    conversation_revision: u64,
    expansion: Option<ExpansionReceiptIdentity>,
}

/// Host-owned boundary between a complete implementation design and one necessary user decision.
pub enum DesignExpansionOutcome {
    Ready(Box<DesignDocument>),
    Waiting { question: String, conversation_revision: u64 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
/// Internal receipt identity preserved across durable run continuation.
pub struct ExpansionReceiptIdentity {
    source_sha256: [u8; 32],
    manifest_sha256: [u8; 32],
    #[serde(default)]
    expanded_sha256: [u8; 32],
    #[serde(default)]
    task_sha256: [u8; 32],
    #[serde(default)]
    evidence_sha256: [u8; 32],
    #[serde(default)]
    input_revision: u64,
    policy_version: String,
}

impl ExpansionReceiptIdentity {
    fn ready(
        input: &ProductRunInput,
        source: &SourceDesignSnapshot,
        expansion: &ExpansionSnapshot,
        markdown: &str,
    ) -> Result<Self, ProductRunnerError> {
        let Some((bytes, digest)) = expansion.ready_design_identity() else {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "bind expanded implementation design",
                "design expansion is not ready",
            ));
        };
        if bytes != markdown.len() as u64 || digest != peritus_codec::sha256(markdown.as_bytes()) {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "bind expanded implementation design",
                "expanded design bytes differ from the ready manifest",
            ));
        }
        Ok(Self {
            source_sha256: source.metadata().sha256().into_bytes(),
            manifest_sha256: expansion.digest().into_bytes(),
            expanded_sha256: digest.into_bytes(),
            task_sha256: peritus_codec::sha256(input.task.as_bytes()).into_bytes(),
            evidence_sha256: expansion.evidence_digest()?.into_bytes(),
            input_revision: expansion.input_revision(),
            policy_version: DESIGN_EXPANSION_POLICY_VERSION.to_owned(),
        })
    }

    fn is_complete(&self) -> bool {
        self.source_sha256 != [0; 32]
            && self.manifest_sha256 != [0; 32]
            && self.expanded_sha256 != [0; 32]
            && self.task_sha256 != [0; 32]
            && self.evidence_sha256 != [0; 32]
            && self.input_revision != 0
            && self.policy_version == DESIGN_EXPANSION_POLICY_VERSION
    }

    pub(crate) fn marker(&self) -> String {
        format!(
            "source_sha256={} manifest_sha256={} expanded_sha256={} task_sha256={} evidence_sha256={} input_revision={} policy={}",
            crate::encode_hex(&self.source_sha256),
            crate::encode_hex(&self.manifest_sha256),
            crate::encode_hex(&self.expanded_sha256),
            crate::encode_hex(&self.task_sha256),
            crate::encode_hex(&self.evidence_sha256),
            self.input_revision,
            self.policy_version,
        )
    }
}

impl DesignDocument {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn markdown(&self) -> &str {
        &self.markdown
    }

    pub const fn conversation_revision(&self) -> u64 {
        self.conversation_revision
    }

    pub(crate) const fn expansion_identity(&self) -> Option<&ExpansionReceiptIdentity> {
        self.expansion.as_ref()
    }

    pub(crate) fn contract_marker(&self) -> String {
        self.expansion.as_ref().map_or_else(
            || {
                format!(
                    "legacy_or_deterministic=true expanded_sha256={} input_revision={}",
                    crate::encode_hex(peritus_codec::sha256(self.markdown.as_bytes()).as_bytes()),
                    self.conversation_revision,
                )
            },
            ExpansionReceiptIdentity::marker,
        )
    }

    pub(crate) const fn restored(
        path: PathBuf,
        markdown: String,
        conversation_revision: u64,
    ) -> Self {
        Self { path, markdown, conversation_revision, expansion: None }
    }

    pub(crate) const fn restored_expanded(
        path: PathBuf,
        markdown: String,
        conversation_revision: u64,
        expansion: ExpansionReceiptIdentity,
    ) -> Self {
        Self { path, markdown, conversation_revision, expansion: Some(expansion) }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "provider failover, durable research state, and publication order remain explicit"
)]
pub async fn create(
    input: &ProductRunInput,
    primary: &Arc<dyn ModelProvider>,
    fallbacks: &[Arc<dyn ModelProvider>],
    cycle: u32,
    accounting: &mut RunAccounting,
) -> Result<DesignExpansionOutcome, ProductRunnerError> {
    if let Some(document) = reuse::load(input) {
        return Ok(DesignExpansionOutcome::Ready(Box::new(document)));
    }
    if let Some(document) = reuse::load_ready_expansion(input) {
        return Ok(DesignExpansionOutcome::Ready(Box::new(document)));
    }
    if design_scope(&input.workspace_root) == DesignScope::Artifact
        && !input.workspace_kind.is_in_place()
    {
        return artifact::create(input).map(Box::new).map(DesignExpansionOutcome::Ready);
    }
    rollout::require_new_expansion(&input.trace_path)?;
    let mut providers = crate::failover::ProviderCursor::new(primary, fallbacks);
    let mut invocation = 0_u32;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut correction = None;
    let workspace_tools = input.read_only_workspace_tools().for_design_expansion();
    let mut tools = research::DesignTools::new(input, workspace_tools)?;
    loop {
        check_cancelled(input)?;
        tools.refresh()?;
        let memory = input.working_memory("designer")?;
        crate::failover::bypass_open_circuit(input, "designer", cycle, accounting, &mut providers)?;
        invocation = invocation.saturating_add(1);
        let revision = input.conversation.revision();
        let transcript = input.conversation.render();
        let media = match input.media(&transcript, providers.current().profile()) {
            Ok(media) => media,
            Err(error) if let Some(switch) = providers.advance_for_capability(&error) => {
                crate::failover::record_switch(input, "designer", cycle, accounting, switch)?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let mut prompt = invocation_prompt(input, correction.as_deref())?;
        prompt.push_str(&tools.context());
        let (prompt, attachments) = media.into_parts(prompt);
        let result = crate::local_context::run_live_invocation(
            providers.current(),
            DeveloperLoopRequest {
                request_prefix: crate::turn::fresh_request_prefix(
                    input.run_id,
                    "designer",
                    cycle,
                    input.conversation.revision(),
                    invocation,
                )?,
                system: system_prompt(accounting.remaining()) + input.delivery_instructions(),
                prompt,
                cyber_access_program: None,
                attachments,
                tools: design_definitions()?,
                limits: DeveloperLoopLimits::new(48, 512)
                    .map_err(|error| crate::turn::developer_error(&error))?,
                cancellation: input.provider_cancellation.clone(),
            },
            &mut tools,
            crate::local_context::InvocationAccounting {
                trace_path: &input.trace_path,
                accounting,
            },
            memory.as_ref(),
            input.conversation.interaction(),
            peritus_agent::DeveloperModelRole::Writer,
        )
        .await;
        let result = match checked_invocation(accounting, result)? {
            Ok(result) => result,
            Err(peritus_agent::DeveloperLoopError::SegmentExhausted) => {
                accounting.record_role_retry()?;
                provider_recovery.reset();
                correction = Some(crate::failover::RoleRecovery::correction("segment_boundary"));
                continue;
            }
            Err(error) => {
                if let Some(reason) = provider_recovery.retry(&error) {
                    accounting.record_role_retry()?;
                    correction = Some(crate::failover::RoleRecovery::correction(reason));
                    continue;
                }
                if let Some(switch) = providers.advance(&error) {
                    crate::failover::record_switch(input, "designer", cycle, accounting, switch)?;
                    provider_recovery.reset();
                    correction = None;
                    continue;
                }
                return Err(crate::turn::developer_error(&error));
            }
        };
        crate::failover::record_provider_success(accounting, &providers, &mut provider_recovery);
        check_cancelled(input)?;
        if tools.needs_context_rebind() {
            continue;
        }
        if input.conversation.revision() != revision {
            correction = None;
            continue;
        }
        match tools.finish(&result.text) {
            Ok(outcome) => return Ok(outcome),
            Err(error) if error.kind() == ProductRunnerErrorKind::InvalidModelOutput => {
                accounting.record_role_retry()?;
                correction = Some(correction_prompt(&error));
            }
            Err(error) => return Err(error),
        }
    }
}

fn design_definitions() -> Result<Vec<peritus_model_protocol::ToolDefinition>, ProductRunnerError> {
    let mut definitions = read_only_definitions()?;
    definitions.push(research::definition().map_err(|error| crate::turn::developer_error(&error))?);
    definitions
        .push(research::source_definition().map_err(|error| crate::turn::developer_error(&error))?);
    definitions
        .extend(expert_tools::definitions().map_err(|error| crate::turn::developer_error(&error))?);
    Ok(definitions)
}

fn checked_invocation(
    accounting: &mut RunAccounting,
    result: Result<peritus_agent::DeveloperLoopOutcome, peritus_agent::DeveloperLoopError>,
) -> Result<
    Result<peritus_agent::DeveloperLoopOutcome, peritus_agent::DeveloperLoopError>,
    ProductRunnerError,
> {
    accounting.check()?;
    Ok(result)
}

fn invocation_prompt(
    input: &ProductRunInput,
    correction: Option<&str>,
) -> Result<String, ProductRunnerError> {
    let mut stable_context = input.conversation.stable_request_context();
    stable_context.push_str(&crate::guided_design::implementation_context(input)?);
    Ok(user_prompt(&stable_context, correction))
}

pub fn system_prompt(remaining: Option<std::time::Duration>) -> String {
    use std::fmt::Write as _;
    let mut instructions =
        format!("{}\n\n{}", crate::engineering_workflow::architect(), expert_tools::POLICY);
    if let Some(remaining) = remaining {
        write!(instructions, "\n\n# Dynamic run context\n\nThe caller-selected run horizon has approximately {} seconds left; preserve enough time for authorized implementation and verification.", remaining.as_secs()).expect("format String");
    }
    instructions
}
fn design_scope(workspace_root: &Path) -> DesignScope {
    let Ok(text) = fs::read_to_string(workspace_root.join("peritus-workspace.toml")) else {
        return DesignScope::Source;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return DesignScope::Source;
    };
    let Some(table) = value.as_table() else {
        return DesignScope::Source;
    };
    if table.len() == 2
        && table.get("schema_version").and_then(toml::Value::as_integer) == Some(1)
        && table.get("kind").and_then(toml::Value::as_str) == Some("artifact")
    {
        DesignScope::Artifact
    } else {
        DesignScope::Source
    }
}

fn user_prompt(transcript: &str, correction: Option<&str>) -> String {
    let correction = correction.map_or(String::new(), |value| {
        format!("\n\nHarness correction from the previous rejected design:\n{value}")
    });
    format!(
        "Conversation and requested outcome:\n{transcript}\n\nUse the expert pass: select the supplied design, identify focused domain questions, research authoritative answers, save useful amendments and acceptance criteria, review, and publish the retained draft by version. Keep critical unknowns explicit. The host builds the ledger; do not regenerate unchanged documents or bookkeeping.{correction}"
    )
}

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The retained design was rejected during {}: {}. Reconcile the current host projection, read only the affected retained draft or records, correct that amendment or apply a small design_candidate_patch, and retry design_publish at the current version. Preserve all unchanged work; do not resend the complete document or ledger.",
        error.operation(),
        error.detail(),
    )
}

fn normalize(value: &str) -> Result<String, ProductRunnerError> {
    let trimmed = value.trim();
    let markdown = trimmed
        .strip_prefix("```markdown")
        .or_else(|| trimmed.strip_prefix("```md"))
        .and_then(|inner| inner.strip_suffix("```"))
        .map_or(trimmed, str::trim);
    let has_title = markdown.starts_with("# ");
    if markdown.len() > MAXIMUM_DESIGN_BYTES
        || markdown.is_empty()
        || (has_title && !markdown.lines().skip(1).any(|line| !line.trim().is_empty()))
    {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "validate implementation design",
            "designer must return a nonempty Markdown task plan with a meaningful body",
        ));
    }
    // A missing presentation heading must not spend another provider request or reopen
    // discovery. The host owns the document envelope; preserve the actual plan verbatim.
    Ok(if has_title {
        format!("{markdown}\n")
    } else {
        format!("# Implementation task plan\n\n{markdown}\n")
    })
}

fn publish(path: &Path, bytes: &[u8]) -> Result<(), ProductRunnerError> {
    let parent = path.parent().ok_or_else(|| filesystem("design path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| filesystem(error.to_string()))?;
    let temporary = path.with_extension("design.md.new");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| filesystem(error.to_string()))?;
    file.write_all(bytes).map_err(|error| filesystem(error.to_string()))?;
    file.sync_all().map_err(|error| filesystem(error.to_string()))?;
    fs::rename(&temporary, path).map_err(|error| filesystem(error.to_string()))
}

fn filesystem(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Repository,
        "publish implementation design",
        detail,
    )
}

fn grounding(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "ground implementation design in repository evidence",
        detail,
    )
}

#[cfg(test)]
mod receipt_tests {
    use super::*;

    #[test]
    fn expanded_contract_marker_carries_every_stage_seven_identity() {
        let identity = ExpansionReceiptIdentity {
            source_sha256: [0x11; 32],
            manifest_sha256: [0x22; 32],
            expanded_sha256: [0x33; 32],
            task_sha256: [0x44; 32],
            evidence_sha256: [0x55; 32],
            input_revision: 7,
            policy_version: DESIGN_EXPANSION_POLICY_VERSION.to_owned(),
        };
        assert!(identity.is_complete());
        let marker = identity.marker();
        for expected in [
            "source_sha256=1111",
            "manifest_sha256=2222",
            "expanded_sha256=3333",
            "task_sha256=4444",
            "evidence_sha256=5555",
            "input_revision=7",
            DESIGN_EXPANSION_POLICY_VERSION,
        ] {
            assert!(marker.contains(expected), "marker omitted {expected}");
        }

        let legacy: ExpansionReceiptIdentity = serde_json::from_str(&format!(
            r#"{{"source_sha256":[17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17],"manifest_sha256":[34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34],"policy_version":"{DESIGN_EXPANSION_POLICY_VERSION}"}}"#,
        ))
        .expect("stage-five identity remains readable");
        assert!(!legacy.is_complete());
    }
}
