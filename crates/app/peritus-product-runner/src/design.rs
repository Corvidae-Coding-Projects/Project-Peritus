//! Mandatory repository-grounded design document produced before implementation.

mod artifact;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use peritus_agent::{DeveloperLoop, DeveloperLoopLimits, DeveloperLoopRequest};
use peritus_provider_core::ModelProvider;

use crate::budget::RunAccounting;
use crate::developer_tools::{WorkspaceDeveloperTools, read_only_definitions};
use crate::execution::{ProductRunInput, check_cancelled};
use crate::trace::FileDeveloperTrace;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MINIMUM_DESIGN_BYTES: usize = 512;
const MAXIMUM_DESIGN_BYTES: usize = 1024 * 1024;
const MAX_INVALID_DESIGNS: u8 = 3;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DesignScope {
    Artifact,
    Source,
}

/// Detailed design artifact and conversation revision it covers.
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
}

/// Inspects the repository with read-only tools, writes the detailed design, and returns it.
pub async fn create(
    input: &ProductRunInput,
    primary: &Arc<dyn ModelProvider>,
    fallbacks: &[Arc<dyn ModelProvider>],
    cycle: u32,
    accounting: &mut RunAccounting,
) -> Result<DesignDocument, ProductRunnerError> {
    let scope = design_scope(&input.workspace_root);
    if scope == DesignScope::Artifact {
        return artifact::create(input);
    }
    let mut providers = crate::failover::ProviderCursor::new(primary, fallbacks);
    let mut invocation = 0_u32;
    let mut invalid_designs = 0_u8;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut correction = None;
    loop {
        check_cancelled(input)?;
        invocation = invocation.saturating_add(1);
        let revision = input.conversation.revision();
        let transcript = input.conversation.render();
        let media = match crate::workspace_media::discover(
            &input.workspace_root,
            &transcript,
            providers.current().profile(),
        ) {
            Ok(media) => media,
            Err(error) if let Some(switch) = providers.advance_for_capability(&error) => {
                crate::failover::record_switch(input, "designer", cycle, accounting, switch)?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let (prompt, attachments) =
            media.into_parts(user_prompt(&transcript, correction.as_deref()));
        let mut tools = WorkspaceDeveloperTools::read_only(input.workspace_root.clone());
        let mut trace = FileDeveloperTrace::new(input.trace_path.clone());
        let result = DeveloperLoop::run(
            providers.current(),
            DeveloperLoopRequest {
                request_prefix: format!(
                    "{}-invocation-{invocation}",
                    crate::turn::request_name(input.run_id, "designer", cycle)
                ),
                system: system_prompt(accounting.remaining()),
                prompt,
                attachments,
                tools: read_only_definitions()?,
                limits: DeveloperLoopLimits::new(48, 512)
                    .map_err(|error| crate::turn::developer_error(&error))?,
                cancellation: input.provider_cancellation.clone(),
            },
            &mut tools,
            &mut trace,
        )
        .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if let Some(reason) = provider_recovery.retry(&error) {
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
        provider_recovery.reset();
        accounting.record(&result)?;
        check_cancelled(input)?;
        if input.conversation.revision() != revision {
            invalid_designs = 0;
            correction = None;
            continue;
        }
        let markdown =
            tools.grounding().validate().map_err(grounding).and_then(|()| normalize(&result.text));
        let mut markdown = match markdown {
            Ok(markdown) => markdown,
            Err(error) => {
                invalid_designs = invalid_designs.saturating_add(1);
                if invalid_designs < MAX_INVALID_DESIGNS {
                    correction = Some(correction_prompt(&error));
                    continue;
                }
                return Err(error);
            }
        };
        markdown.push_str(&tools.grounding().markdown());
        let path = input.trace_path.with_extension("design.md");
        publish(&path, markdown.as_bytes())?;
        return Ok(DesignDocument { path, markdown, conversation_revision: revision });
    }
}

fn system_prompt(remaining: std::time::Duration) -> String {
    let proportionality = "Scale the design to the actual change. Keep small changes concise while giving multi-module source work all detail needed for independent implementation. Do not repeat the same requirement across sections merely to make the document longer.";
    let instructions = format!(
        "You are the design architect in a serious coding harness. Inspect the actual repository with the read-only workspace tools before designing. Return only a detailed Markdown design document, not JSON and not a code fence. Preserve the requested ambition and cover the full requested product rather than proposing an MVP. Ground the document in concrete existing paths, manifests, interfaces, conventions, and constraints; for a greenfield repository, specify the exact structure to create. Begin acceptance reasoning from the original request's literal paths, values, operations, and grammatical scope. Do not override an explicit expected value with a model-derived invariant or manufacture a conflict by broadening a narrowly scoped rule. Respect the workspace's declared product kind: for an artifact workspace whose requested deliverables are generated outputs rather than retained code, design a bounded producer and independent artifact/effect verification without inventing package scaffolding. Include sections for Objective and acceptance criteria, Repository findings, Architecture and interfaces, Data and control flow, File and module plan, Implementation slices, Verification, and Risks or explicit non-goals. Make slices independently actionable where practical. Focus on realistic application behavior and avoid speculative adversarial edge cases. Do not edit files, run commands, implement code, or commit.\n\n{}\n\n{proportionality}",
        crate::engineering_workflow::architect(),
    );
    format!(
        "The complete run has approximately {} seconds left at this design invocation. Keep enough of that shared window for implementation, gates, independent review, and fixes.\n\n{instructions}",
        remaining.as_secs()
    )
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
        "Conversation and requested outcome:\n{transcript}\n\nInspect the managed workspace and write the complete implementation design that the writer will follow.{correction}"
    )
}

fn correction_prompt(error: &ProductRunnerError) -> String {
    format!(
        "The previous design was rejected during {}: {}. Return the full design again. Its first nonblank line must be one document title beginning with `# `, and it must contain at least four section headings beginning with `## `. Do not make every section a top-level `#` heading.",
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
    let sections = markdown.lines().filter(|line| line.starts_with("## ")).count();
    if markdown.len() < MINIMUM_DESIGN_BYTES
        || markdown.len() > MAXIMUM_DESIGN_BYTES
        || !markdown.starts_with("# ")
        || sections < 4
    {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "validate implementation design",
            "designer must return a detailed Markdown document with a title and at least four sections",
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
        let prompt = system_prompt(std::time::Duration::from_mins(10));
        assert!(prompt.contains("original request's literal paths, values, operations"));
        assert!(prompt.contains("Do not override an explicit expected value"));
        assert!(prompt.contains("broadening a narrowly scoped rule"));
        assert!(prompt.contains("non-exhaustive"));
        assert!(prompt.contains("preserve that precedence"));
        assert!(prompt.contains("owns the primary field"));
        assert!(prompt.contains("opaque contract values"));
        assert!(prompt.contains("reversible requested artifact"));
        assert!(prompt.contains("without inventing package scaffolding"));
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
