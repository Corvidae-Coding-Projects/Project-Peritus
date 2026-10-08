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
use peritus_run_settlement::CandidateIdentity;

use crate::budget::RunAccounting;
use crate::developer_tools::{GroundingEvidence, WorkspaceDeveloperTools, read_only_definitions};
use crate::execution::{ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DesignScope {
    Artifact,
    Source,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceGroundingIdentity {
    run: [u8; 16],
    workspace: [u8; 16],
    content: [u8; 32],
    request_sources: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
struct SourceDesignBinding {
    grounding: SourceGroundingIdentity,
    repository: [u8; 32],
    conversation_revision: u64,
    request_source_revision: u64,
    request_source_catalog: [u8; 32],
    request_sources_required: bool,
}

impl SourceDesignBinding {
    fn capture(
        input: &ProductRunInput,
        candidate: CandidateIdentity,
    ) -> Result<Self, ProductRunnerError> {
        if candidate.run_id() != input.run_id || candidate.workspace_id() != input.workspace_id {
            return Err(source_binding(
                "retained candidate identity belongs to a different run or workspace",
            ));
        }
        let conversation_revision = input.conversation.revision();
        let request_source_revision = input
            .conversation
            .request_source_revision()
            .map_err(source_binding)?;
        let request_sources_required = input
            .conversation
            .request_sources_required()
            .map_err(source_binding)?;
        if request_source_revision != conversation_revision {
            return Err(source_binding(
                "request-source revision differs from the current conversation revision",
            ));
        }
        Ok(Self {
            grounding: SourceGroundingIdentity {
                run: *candidate.run_id().as_bytes(),
                workspace: *candidate.workspace_id().as_bytes(),
                content: candidate.content_digest().into_bytes(),
                request_sources: input.conversation.request_source_binding(),
            },
            repository: candidate.repository_digest().into_bytes(),
            conversation_revision,
            request_source_revision,
            request_source_catalog: input.conversation.request_source_catalog_binding(),
            request_sources_required,
        })
    }

    fn grounding_is_reusable_for(&self, current: &Self) -> bool {
        self.grounding == current.grounding
    }

    fn same_authority_as(&self, current: &Self) -> bool {
        self.grounding == current.grounding
            && self.repository == current.repository
            && self.conversation_revision == current.conversation_revision
            && self.request_source_revision == current.request_source_revision
            && self.request_source_catalog == current.request_source_catalog
            && self.request_sources_required == current.request_sources_required
    }

    fn grounding_role(&self) -> String {
        let mut bytes = [0_u8; 96];
        bytes[..16].copy_from_slice(&self.grounding.run);
        bytes[16..32].copy_from_slice(&self.grounding.workspace);
        bytes[32..64].copy_from_slice(&self.grounding.content);
        bytes[64..].copy_from_slice(&self.grounding.request_sources);
        format!("designer-{}", hex_bytes(peritus_codec::sha256(&bytes).as_bytes()))
    }
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
    candidate: CandidateIdentity,
) -> Result<DesignDocument, ProductRunnerError> {
    let scope = read_design_scope(&input.workspace_root)?;
    if scope == DesignScope::Artifact && !input.workspace_kind.is_in_place() {
        return artifact::create(input);
    }
    let memory = crate::local_context::LocalContextHandle::open_async(input, "designer").await?;
    let mut providers = crate::failover::ProviderCursor::new(primary, fallbacks);
    let mut invocation = 0_u64;
    let mut provider_recovery = crate::failover::RoleRecovery::default();
    let mut correction = None;
    let mut binding = SourceDesignBinding::capture(input, candidate)?;
    let mut grounding_role = binding.grounding_role();
    let mut grounding_scope = crate::turn::request_scope(input.run_id, &grounding_role);
    let mut grounding_revision = binding.conversation_revision;
    let mut grounding_prefix = format!(
        "{}-revision-{grounding_revision}-invocation-",
        crate::turn::request_name(input.run_id, &grounding_role, cycle),
    );
    let mut grounding = memory
        .as_ref()
        .map(|memory| memory.recover_grounding_scope(&grounding_scope))
        .transpose()
        .map_err(|error| crate::turn::developer_error(&error))?
        .unwrap_or_else(|| GroundingEvidence::for_workspace(&input.workspace_root));
    loop {
        check_cancelled(input)?;
        let current_binding = SourceDesignBinding::capture(input, candidate)?;
        if !binding.same_authority_as(&current_binding) {
            if current_binding.conversation_revision == binding.conversation_revision
                && !binding.grounding_is_reusable_for(&current_binding)
            {
                return Err(source_binding(
                    "governing request-source identity changed without a conversation revision",
                ));
            }
            grounding_role = current_binding.grounding_role();
            grounding_scope = crate::turn::request_scope(input.run_id, &grounding_role);
            grounding_revision = current_binding.conversation_revision;
            grounding_prefix = format!(
                "{}-revision-{grounding_revision}-invocation-",
                crate::turn::request_name(input.run_id, &grounding_role, cycle),
            );
            let recovered = memory
                .as_ref()
                .map(|memory| memory.recover_grounding_scope(&grounding_scope))
                .transpose()
                .map_err(|error| crate::turn::developer_error(&error))?
                .unwrap_or_else(|| GroundingEvidence::for_workspace(&input.workspace_root));
            if binding.grounding_is_reusable_for(&current_binding) {
                grounding.merge(&recovered);
            } else {
                grounding = recovered;
            }
            providers.reopen();
            binding = current_binding;
            correction = None;
        }
        invocation = invocation.checked_add(1).ok_or_else(|| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Repository,
                "allocate designer invocation identity",
                "invocation sequence overflow",
            )
        })?;
        let revision = binding.conversation_revision;
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
                .with_grounding(grounding.clone())
                .with_task_contract(&transcript),
        );
        let reopened = memory
            .as_ref()
            .map(|memory| memory.pending_reentry_prefix(&grounding_prefix))
            .transpose()
            .map_err(|error| crate::turn::developer_error(&error))?
            .flatten();
        let request_prefix = match reopened {
            Some(prefix) => prefix,
            None => crate::turn::invocation_request_name(
                input.run_id,
                &grounding_role,
                cycle,
                revision,
                invocation,
            )?,
        };
        let result = crate::local_context::run_live_invocation(
            providers.current(),
            DeveloperLoopRequest {
                local_session_directory: Some(input.native_session_directory("designer")),
                request_prefix,
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
        grounding = tools.grounding().clone();
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
        crate::failover::record_provider_success(&mut provider_recovery);
        check_cancelled(input)?;
        if !binding.same_authority_as(&SourceDesignBinding::capture(input, candidate)?) {
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
    match (tools.grounding().validate(), normalize(text)) {
        (Ok(()), Ok(mut markdown)) => {
            markdown.push_str(&tools.grounding().markdown());
            Ok(markdown)
        }
        (grounding_result, markdown_result) => {
            let mut failures = Vec::new();
            if let Err(detail) = grounding_result {
                failures.push(detail.to_owned());
            }
            if let Err(error) = markdown_result {
                failures.push(error.detail().to_owned());
            }
            Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "validate implementation design",
                failures.join("; "),
            ))
        }
    }
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

fn source_binding(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Repository,
        "bind implementation design to authoritative sources",
        detail,
    )
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
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
