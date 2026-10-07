//! Mandatory repository-grounded design document produced before implementation.

mod artifact;

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

/// Inspects the repository with read-only tools, writes the detailed design, and returns it.
pub async fn create(
    input: &ProductRunInput,
    primary: &Arc<dyn ModelProvider>,
    fallbacks: &[Arc<dyn ModelProvider>],
    cycle: u32,
    accounting: &mut RunAccounting,
) -> Result<DesignDocument, ProductRunnerError> {
    let scope = read_design_scope(&input.workspace_root)?;
    if scope == DesignScope::Artifact && !input.workspace_kind.is_in_place() {
        return artifact::create(input);
    }
    let memory = crate::local_context::LocalContextHandle::open(input, "designer")?;
    let mut providers = crate::failover::ProviderCursor::new(primary, fallbacks);
    let mut invocation = 0_u32;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut correction = None;
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
        let stable_context = input.conversation.stable_request_context();
        let (prompt, attachments) =
            media.into_parts(user_prompt(&stable_context, correction.as_deref()));
        let mut tools = input.configure_tools(
            WorkspaceDeveloperTools::read_only(input.workspace_root.clone())
                .with_task_contract(&transcript),
        );
        let result = crate::local_context::run_live_invocation(
            providers.current(),
            DeveloperLoopRequest {
                local_session_directory: Some(input.native_session_directory("designer")),
                request_prefix: format!(
                    "{}-invocation-{invocation}",
                    crate::turn::request_name(input.run_id, "designer", cycle)
                ),
                system: system_prompt(accounting.remaining()) + input.delivery_instructions(),
                prompt,
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
        accounting.check()?;
        let result = match result {
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
        let path = input.trace_path.with_extension("design.md");
        publish(&path, markdown.as_bytes())?;
        return Ok(DesignDocument { path, markdown, conversation_revision: revision });
    }
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
    let proportionality = "Scale the design to the actual change. For a small change, provide only the concrete implementation decision and how to verify it. A concise plan is sufficient; no byte count, title format, or number of headings is required. Give multi-module source work the detail needed for independent implementation. Use headings when they help the reader, and do not repeat requirements merely to make the document longer.";
    let instructions = format!(
        "You are the design architect in a serious coding harness. Inspect the actual repository with the read-only workspace tools before designing. Return an implementation plan in Markdown. Preserve the requested ambition and cover the full requested product rather than proposing an MVP. Ground the plan in concrete existing paths, manifests, interfaces, conventions, and constraints; for a greenfield repository, specify the exact structure to create. Begin acceptance reasoning from the original request's literal paths, values, operations, and grammatical scope. Do not override an explicit expected value with a model-derived invariant or manufacture a conflict by broadening a narrowly scoped rule. Respect the workspace's declared product kind: for an artifact workspace whose requested deliverables are generated outputs rather than retained code, design a bounded producer and independent artifact/effect verification without inventing package scaffolding. Where the change needs them, address acceptance criteria, repository findings, interfaces, data flow, affected files, implementation steps, verification, and concrete risks. Make steps independently actionable where practical. Focus on realistic application behavior and avoid speculative adversarial edge cases. Do not edit files, run commands, implement code, or commit.\n\n{}\n\n{proportionality}",
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

fn read_design_scope(workspace_root: &Path) -> Result<DesignScope, ProductRunnerError> {
    peritus_gates::WorkspaceProductScope::read(workspace_root)
        .map(|scope| match scope {
            peritus_gates::WorkspaceProductScope::Artifact => DesignScope::Artifact,
            peritus_gates::WorkspaceProductScope::Source => DesignScope::Source,
        })
        .map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidPrecondition,
                "admit declared workspace design scope",
                error.to_string(),
            )
        })
}

#[cfg(test)]
fn design_scope(workspace_root: &Path) -> DesignScope {
    read_design_scope(workspace_root).expect("fixture workspace scope is valid")
}

fn user_prompt(transcript: &str, correction: Option<&str>) -> String {
    let correction = correction.map_or(String::new(), |value| {
        format!("\n\nHarness correction from the previous rejected design:\n{value}")
    });
    format!(
        "Conversation and requested outcome:\n{transcript}\n\nInspect the managed workspace and write the complete implementation design that the writer will follow.{correction}"
    )
}

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The previous plan was rejected during {}: {}. Resolve that specific criterion under the same task and retained context. Accepted source observations remain available; do not repeat startup inspection solely because plan validation was retried. Return the concrete implementation plan at the level of detail this request needs. No byte count, title format, or heading count is required.",
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
    if markdown.is_empty() {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "validate implementation design",
            "designer returned an empty implementation plan; provide the concrete change and its verification",
        ));
    }
    Ok(format!("{markdown}\n"))
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
mod tests {
    use super::*;

    #[test]
    fn design_requires_a_real_markdown_document() {
        assert!(normalize("not a design").is_err());
        let detailed = format!(
            "# Design\n\n## Objective\n{}\n\n## Repository findings\nConcrete.\n\n## Architecture\nConcrete.\n\n## Implementation\nConcrete.\n\n## Verification\nConcrete.",
            "Complete requested behavior. ".repeat(24)
        );
        assert!(normalize(&detailed).is_ok());
    }

    #[test]
    fn rejected_design_retry_explains_the_exact_heading_contract() {
        let error = normalize("# Objective\n\n# Architecture\n\n# Verification")
            .expect_err("invalid heading hierarchy");
        let correction = correction_prompt(&error);
        let prompt = user_prompt("task", Some(&correction));

        assert!(prompt.contains("Harness correction from the previous rejected design"));
        assert!(prompt.contains("first nonblank line"));
        assert!(prompt.contains("at least four section headings"));
        assert!(prompt.contains(error.detail()));
    }

    #[test]
    fn design_keeps_literal_values_and_scoped_rules_authoritative() {
        let prompt = system_prompt(Some(std::time::Duration::from_mins(10)));
        assert!(prompt.contains("original request's literal paths, values, operations"));
        assert!(prompt.contains("Do not override an explicit expected value"));
        assert!(prompt.contains("broadening a narrowly scoped rule"));
        assert!(prompt.contains("non-exhaustive"));
        assert!(prompt.contains("preserve that precedence"));
        assert!(prompt.contains("owns the primary field"));
        assert!(prompt.contains("opaque contract values"));
        assert!(prompt.contains("reversible requested artifact"));
        assert!(prompt.contains("without inventing package scaffolding"));

        let unbounded = system_prompt(None);
        assert!(!unbounded.contains("seconds left"));
        assert!(!unbounded.contains("shared window"));
    }

    #[test]
    fn artifact_workspace_marker_selects_the_deterministic_design_path() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(
            workspace.path().join("peritus-workspace.toml"),
            "schema_version = 1\nkind = \"artifact\"\n",
        )
        .expect("manifest");

        let scope = design_scope(workspace.path());
        assert_eq!(scope, DesignScope::Artifact);
    }
}
