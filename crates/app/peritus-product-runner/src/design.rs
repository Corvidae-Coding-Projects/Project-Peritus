//! Mandatory repository-grounded design document produced before implementation.

mod artifact;
mod reuse;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_provider_core::ModelProvider;

use crate::budget::RunAccounting;
use crate::developer_tools::{WorkspaceDeveloperTools, read_only_definitions};
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

    pub(crate) const fn restored(
        path: PathBuf,
        markdown: String,
        conversation_revision: u64,
    ) -> Self {
        Self { path, markdown, conversation_revision }
    }
}

pub async fn create(
    input: &ProductRunInput,
    primary: &Arc<dyn ModelProvider>,
    fallbacks: &[Arc<dyn ModelProvider>],
    cycle: u32,
    accounting: &mut RunAccounting,
) -> Result<DesignDocument, ProductRunnerError> {
    if let Some(document) = reuse::load(input)? {
        return Ok(document);
    }
    if design_scope(&input.workspace_root) == DesignScope::Artifact
        && !input.workspace_kind.is_in_place()
    {
        return artifact::create(input);
    }
    let mut providers = crate::failover::ProviderCursor::new(primary, fallbacks);
    let memory = input.working_memory("designer")?;
    let mut invocation = 0_u32;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut correction = None;
    let mut tools = input.read_only_workspace_tools().for_design();
    loop {
        check_cancelled(input)?;
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
        let prompt = invocation_prompt(input, correction.as_deref())?;
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
                tools: read_only_definitions()?,
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
        if input.conversation.revision() != revision {
            correction = None;
            continue;
        }
        let markdown = match grounded_markdown(&tools, &result.text) {
            Ok(markdown) => markdown,
            Err(error) => {
                accounting.record_role_retry()?;
                correction = Some(correction_prompt(&error));
                continue;
            }
        };
        return publish_design(input, markdown, revision);
    }
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

fn publish_design(
    input: &ProductRunInput,
    markdown: String,
    conversation_revision: u64,
) -> Result<DesignDocument, ProductRunnerError> {
    let path = input.trace_path.with_extension("design.md");
    publish(&path, markdown.as_bytes())?;
    let document = DesignDocument { path, markdown, conversation_revision };
    if let Err(error) = reuse::save(input, &document) {
        eprintln!("peritus: design reuse receipt unavailable: {error}");
    }
    Ok(document)
}

fn grounded_markdown(
    tools: &WorkspaceDeveloperTools,
    text: &str,
) -> Result<String, ProductRunnerError> {
    tools.grounding().validate().map_err(grounding)?;
    let mut markdown = normalize(text)?;
    markdown.push_str(&tools.grounding().markdown());
    Ok(markdown)
}

fn system_prompt(remaining: Option<std::time::Duration>) -> String {
    let proportionality = "Scale the design to the actual change. This phase has one startup grounding check, reused if already complete. After that check return the task plan; the host closes further tools for this phase. Do not attempt a discovery loop or demand complete source knowledge before handing off. Use the request and retained observations, distinguish known contracts from proposed paths, and put specific source questions in the first implementation slice. The writer reads those exact targets while implementing. Preserve all requested outcomes without repeating requirements or inventing source facts.";
    let instructions = format!(
        "You are the design architect in a serious coding harness. Use existing host-verified observations or inspect only the relevant repository contracts before designing. Return only a proportional Markdown task plan, not JSON and not a code fence. A title is optional; the host supplies a missing heading without another request. Preserve the requested ambition and cover the full requested product rather than proposing an MVP. Use observed paths and contracts when available; label proposed paths and unresolved source questions honestly. Do not require repository-wide inspection before returning a plan. For a greenfield repository, specify the exact structure to create. Begin acceptance reasoning from the original request's literal paths, values, operations, and grammatical scope. Do not override an explicit expected value with a model-derived invariant or manufacture a conflict by broadening a narrowly scoped rule. Respect the workspace's declared product kind: for an artifact workspace whose requested deliverables are generated outputs rather than retained code, design a bounded producer and independent artifact/effect verification without inventing package scaffolding. Cover the objective, exact acceptance criteria, affected paths/contracts, next implementation slices, and real behavioral checks. Use additional architecture, data-flow and risk sections only when the change needs those decisions. Make slices independently actionable where practical. Focus on realistic application behavior and avoid speculative adversarial edge cases. Do not edit files, run commands, implement code, or commit.\n\n{}\n\n{proportionality}",
        crate::engineering_workflow::architect(),
    );
    if let Some(remaining) = remaining {
        format!(
            "The complete run has approximately {} seconds left at this design invocation. Keep enough of that shared window for implementation, gates, independent review, and fixes.\n\n{instructions}",
            remaining.as_secs()
        )
    } else {
        instructions
    }
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
        "Conversation and requested outcome:\n{transcript}\n\nUse the single grounding check and retained evidence to write the task plan now. Hand unresolved source questions to the writer's next implementation slice; do not restart repository discovery.{correction}"
    )
}

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The previous design was rejected during {}: {}. Return the full design again. Use one # title and a meaningful, proportional task plan. Small changes do not need extra sections or repeated requirements.",
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
