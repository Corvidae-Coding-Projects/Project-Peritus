//! Deterministic detailed designs for explicit generated-artifact workspaces.

use std::path::Path;

#[cfg(test)]
use std::{collections::VecDeque, fs};

use super::{DesignDocument, publish};
use crate::developer_tools::{DirectoryListingOwner, ListingError};
use crate::delivery_requirement::ExternalEffectRequirement;
use crate::execution::{ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

struct InventoryEntry {
    path: String,
    kind: &'static str,
    bytes: Option<u64>,
}

struct Inventory {
    entries: Vec<InventoryEntry>,
    truncated: bool,
}

struct InventoryEvidence {
    inventory: Inventory,
    observation: Option<String>,
    total_entries: Option<u64>,
    next: Option<String>,
    error: Option<String>,
}

#[derive(Eq, PartialEq)]
struct ConversationEvidence {
    revision: u64,
    source_revision: u64,
    source_binding: [u8; 32],
    source_catalog: [u8; 32],
    source_backed: bool,
    inline: String,
    inline_complete: bool,
}

pub(super) fn create(input: &ProductRunInput) -> Result<DesignDocument, ProductRunnerError> {
    loop {
        check_cancelled(input)?;
        let conversation = conversation_evidence(input)?;
        let inventory = inventory(input)?;
        check_cancelled(input)?;
        if conversation_evidence(input)? != conversation {
            continue;
        }
        let requirement = ExternalEffectRequirement::from_task(input.delivery_scope, &input.task);
        let mut markdown = render(&conversation.inline, &inventory.inventory, requirement);
        render_evidence(&mut markdown, &conversation, &inventory);
        if conversation_evidence(input)? != conversation {
            continue;
        }
        let path = input.trace_path.with_extension("design.md");
        publish(&path, markdown.as_bytes())?;
        if conversation_evidence(input)? != conversation {
            continue;
        }
        return Ok(DesignDocument {
            path,
            markdown,
            conversation_revision: conversation.revision,
        });
    }
}

fn conversation_evidence(
    input: &ProductRunInput,
) -> Result<ConversationEvidence, ProductRunnerError> {
    let revision = input.conversation.revision();
    let source_revision = input.conversation.request_source_revision().map_err(|detail| {
        repository("bind artifact request sources", detail)
    })?;
    let source_backed = input.conversation.request_sources_required().map_err(|detail| {
        repository("bind artifact request sources", detail)
    })?;
    if source_backed && source_revision != revision {
        return Err(repository(
            "bind artifact request sources",
            "request-source revision differs from the current conversation revision",
        ));
    }
    let (inline, inline_complete) = if source_backed {
        // Governed runs keep full logical bodies in the exact source catalog. Avoid asking the
        // conversation adapter to allocate and copy the whole conversation into this document.
        (String::new(), false)
    } else {
        (input.conversation.stable_request_context(), true)
    };
    Ok(ConversationEvidence {
        revision,
        source_revision,
        source_binding: input.conversation.request_source_binding(),
        source_catalog: input.conversation.request_source_catalog_binding(),
        source_backed,
        inline,
        inline_complete,
    })
}

fn inventory(input: &ProductRunInput) -> Result<InventoryEvidence, ProductRunnerError> {
    let owner = DirectoryListingOwner::new(
        input.workspace_root.clone(),
        input.trace_path.with_extension("directory-listings"),
        std::sync::Arc::clone(&input.cancelled),
        input.provider_cancellation.clone(),
    );
    let page = match owner.page(None, None) {
        Ok(page) => page,
        Err(ListingError::Cancelled) => {
            check_cancelled(input)?;
            return Err(repository("inventory artifact workspace", "directory capture cancelled"));
        }
        Err(error) => {
            return Ok(InventoryEvidence {
                inventory: Inventory { entries: Vec::new(), truncated: false },
                observation: None,
                total_entries: None,
                next: None,
                error: Some(error.to_string()),
            });
        }
    };
    let mut entries = Vec::new();
    for item in page.items() {
        let path = if let Some(metadata) = item.metadata() {
            metadata.path().as_str().to_owned()
        } else {
            item.name().display_name()
        };
        if ignored(Path::new(&path)) {
            continue;
        }
        let (kind, bytes) = item.metadata().map_or_else(
            || {
                (
                    item.exclusion().map_or("excluded", |reason| reason.as_str()),
                    None,
                )
            },
            |metadata| {
                (
                    match metadata.kind() {
                        peritus_workspace::WorkspaceEntryKind::File => "file",
                        peritus_workspace::WorkspaceEntryKind::Directory => "directory",
                    },
                    (metadata.kind() == peritus_workspace::WorkspaceEntryKind::File)
                        .then_some(metadata.size()),
                )
            },
        );
        entries.push(InventoryEntry { path, kind, bytes });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(InventoryEvidence {
        inventory: Inventory { entries, truncated: !page.complete() },
        observation: Some(page.observation_handle()),
        total_entries: Some(page.observation().count()),
        next: page.next().map(str::to_owned),
        error: None,
    })
}

#[cfg(test)]
fn inventory_with_limit(root: &Path, maximum: usize) -> Result<Inventory, ProductRunnerError> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut entries = Vec::new();
    while let Some(directory) = pending.pop_front() {
        let mut children = fs::read_dir(&directory)
            .map_err(|error| repository("inventory artifact workspace", error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| repository("inventory artifact workspace", error.to_string()))?;
        children.sort_by_key(fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| repository("inventory artifact workspace", "path escaped workspace"))?
                .to_path_buf();
            if ignored(&relative) {
                continue;
            }
            if entries.len() == maximum {
                entries.sort_by(|left: &InventoryEntry, right| left.path.cmp(&right.path));
                return Ok(Inventory { entries, truncated: true });
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| repository("inspect artifact input", error.to_string()))?;
            let kind = if metadata.is_dir() {
                pending.push_back(path);
                "directory"
            } else if metadata.is_file() {
                "file"
            } else {
                "other"
            };
            entries.push(InventoryEntry {
                path: relative.to_string_lossy().into_owned(),
                kind,
                bytes: metadata.is_file().then_some(metadata.len()),
            });
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Inventory { entries, truncated: false })
}

fn render(
    transcript: &str,
    inventory: &Inventory,
    effect_requirement: ExternalEffectRequirement,
) -> String {
    let mut design = String::from(
        "# Generated artifact production design\n\n\
## Objective and acceptance criteria\n\n\
Produce the complete requested artifacts in this explicit artifact workspace. The inline request projection below is bound to the exact conversation source revision and catalog recorded in Repository grounding evidence. The complete typed user-request sources remain authoritative and retrievable through `request_sources` and `request_source_read`; every named input, output, value, constraint, exclusion, timing rule, and side effect is acceptance-critical:\n\n",
    );
    for line in transcript.lines() {
        design.push_str("> ");
        design.push_str(line);
        design.push('\n');
    }
    if effect_requirement.is_required() {
        design.push_str(
            "\nThe requested deliverable is a live caller-authorized operational result. Supporting scripts, documentation, and configuration files are not completion without the requested effect and a later fresh end-to-end verification.\n",
        );
    }
    design.push_str(
        "\n## Repository findings\n\nThe workspace declares `schema_version = 1` and `kind = \"artifact\"`. It has no retained implementation requirement, so the writer should use the bounded workspace tools directly and leave only the requested outputs. The design records one physical page from an owner-retained exact root-directory observation before mutation:\n\n",
    );
    for entry in &inventory.entries {
        design.push_str("- `");
        design.push_str(&entry.path);
        design.push_str("`: ");
        design.push_str(entry.kind);
        if let Some(bytes) = entry.bytes {
            design.push_str(" (");
            design.push_str(&bytes.to_string());
            design.push_str(" bytes)");
        }
        design.push('\n');
    }
    if inventory.truncated {
        design.push_str(
            "\nThis is a physical navigation page, not a task-scope sample or completion claim. Follow the retained next cursor recorded in Repository grounding evidence, and navigate returned directories explicitly. Omission from this page does not prove that a path is absent.\n",
        );
    }
    design.push_str(
        "\n## Architecture and interfaces\n\nThe original request is the input/output contract. Input paths remain read-only unless the request explicitly requires editing them in place. The writer owns only the explicitly requested output or in-place edit paths and uses `workspace_list`, `workspace_read`, `workspace_write`, `workspace_patch`, `workspace_remove`, and non-destructive `run_command` calls as needed. No package scaffold, retained producer, dependency, network access, or extra artifact is introduced unless the request explicitly requires it.\n\n\
## Data and control flow\n\n1. List the workspace and read the exact current-round inputs named by the request.\n2. If inputs arrive over time, observe them for the full requested interval and perform the required final poll before freezing state.\n3. Apply the request's literal filtering, ordering, deduplication, transformation, and preservation rules.\n4. Build every requested output from one consistent observed state and write independent outputs together when they have no data dependency.\n5. Re-read the outputs and run the applicable host-owned artifact gates before completion.\n\n\
## File and module plan\n\nNo source modules are introduced unless requested. Existing inputs and harness-owned files remain untouched unless the request explicitly requires changing them. Persistent changes are limited to the output or in-place edit paths named in the authoritative conversation; temporary files or directories are removed when the request requires cleanup.\n\n\
## Implementation slices\n\n- **Observe:** inventory and read only the current request's authoritative inputs, including required polling or staged-input boundaries.\n- **Transform:** compute the requested state deterministically while preserving literal identifiers and first-seen or ordering semantics.\n- **Publish:** create the complete requested artifacts without unrelated files or source scaffolding.\n- **Verify:** parse or re-read each final artifact, check cross-artifact consistency, and confirm prohibited effects did not occur.\n\n\
## Verification\n\nVerification must cover every explicit acceptance statement in the conversation, validate the syntax of structured outputs, confirm exact required fields and literal values, and inspect filesystem effects. When acceptance depends on an empirical quality, size, speed, or resource threshold, prepare reusable inputs once when practical, keep a compact candidate ledger of parameters and measured results, preserve the best valid candidate atomically, and use bounded low-cost experiments before expensive full candidates. Keep selection data distinct from the final acceptance holdout: iterate on a training split or cross-validation, then consult the final holdout for the selected candidate rather than repeatedly choosing against it. If a holdout has already guided selection, account for that bias and require a defensible margin or independent evidence for a near-threshold claim. Re-run the selected candidate through the authoritative end-to-end measurement; a training or search metric is not final acceptance. When an empirical or heuristic producer is calibrated from one supplied example but must generalize, reserve an independent segment or use contract-preserving perturbations with known expected relationships; rerunning only the calibration sample is insufficient. When a deliverable accepts inputs beyond the supplied example, exercise at least one independently created or independently selected input and derive format fields, dimensions, offsets, identifiers, and defaults from the authoritative input contract. Treat example-derived constants as hypotheses that must be varied or proved invariant; one successful supplied-input run does not establish a parameterized interface. The product's independent artifact gates remain authoritative for supported formats. A successful write is not completion until the final files are re-read and the requested outcome is checked.\n\n\
## Risks and explicit non-goals\n\nDo not guess unpublished schemas or hidden evaluator conventions. Do not read future-stage or adjacent inputs merely because they are visible. Do not modify input fixtures unless explicitly requested, use network access when excluded, retain helper code, or add package infrastructure for a one-run artifact task. Report an actual source contradiction rather than silently changing the contract.\n\n\
## Repository grounding evidence\n\nThis design was rendered by the Rust product runner from an exact revisioned request-source binding and one bounded physical page of an owner-retained directory observation. The physical page does not limit task scope or claim that unrendered paths are absent. It did not rely on unverified model claims about repository contents.\n",
    );
    if effect_requirement.is_required() {
        design.push_str(
            "\n## Live operational delivery\n\nExecute the requested operation with command purpose `external_effect`, then perform a later deterministic state or end-to-end check with purpose `verification`. Both must succeed. Supporting files remain secondary to that observed live result.\n",
        );
    }
    design
}

fn render_evidence(
    design: &mut String,
    conversation: &ConversationEvidence,
    inventory: &InventoryEvidence,
) {
    design.push_str("\n### Exact retained bindings\n\n");
    design.push_str("- Conversation revision: `");
    design.push_str(&conversation.revision.to_string());
    design.push_str("`\n- Request-source revision: `");
    design.push_str(&conversation.source_revision.to_string());
    design.push_str("`\n- Request-source binding: `");
    design.push_str(&hex_bytes(&conversation.source_binding));
    design.push_str("`\n- Request-source catalog: `");
    design.push_str(&hex_bytes(&conversation.source_catalog));
    design.push_str("`\n- Inline request projection complete: `");
    design.push_str(if conversation.inline_complete { "true" } else { "false" });
    design.push_str("`\n");
    if conversation.source_backed && !conversation.inline_complete {
        design.push_str(
            "- The inline projection is only a physical render page. Read every required typed user source from the exact catalog before using workspace tools.\n",
        );
    }
    match (&inventory.observation, inventory.total_entries, &inventory.error) {
        (Some(observation), Some(total), _) => {
            design.push_str("- Root directory observation: `");
            design.push_str(observation);
            design.push_str("`\n- Exact exposed root-child count: `");
            design.push_str(&total.to_string());
            design.push_str("`\n");
            if let Some(next) = &inventory.next {
                design.push_str("- Root directory next cursor: `");
                design.push_str(next);
                design.push_str("`\n");
            } else {
                design.push_str("- Root directory page state: `complete`\n");
            }
        }
        (_, _, Some(error)) => {
            design.push_str("- Root directory observation error: `");
            design.push_str(&error.replace('`', "'"));
            design.push_str("`\n- No absence or completeness claim is made for this failed observation.\n");
        }
        _ => {
            design.push_str("- Root directory observation state is unavailable.\n");
        }
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn ignored(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(".git" | "target" | "node_modules" | ".venv" | "__pycache__")
        )
    })
}

fn repository(operation: &'static str, detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, operation, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_design_preserves_the_request_and_observed_inventory() {
        let design = render(
            "Create out/report.json with status \"ready\".\nDo not modify input files.",
            &Inventory {
                entries: vec![
                    InventoryEntry {
                        path: "in/source.json".to_owned(),
                        kind: "file",
                        bytes: Some(42),
                    },
                    InventoryEntry { path: "out".to_owned(), kind: "directory", bytes: None },
                ],
                truncated: false,
            },
            ExternalEffectRequirement::Optional,
        );

        assert!(design.starts_with("# Generated artifact production design"));
        assert!(design.contains("> Create out/report.json with status \"ready\"."));
        assert!(design.contains("`in/source.json`: file (42 bytes)"));
        assert!(design.contains("## Implementation slices"));
        assert!(design.contains("rerunning only the calibration sample is insufficient"));
        assert!(design.contains("keep a compact candidate ledger"));
        assert!(design.contains("preserve the best valid candidate atomically"));
        assert!(design.contains("Keep selection data distinct from the final acceptance holdout"));
        assert!(design.contains("account for that bias"));
        assert!(design.contains("one successful supplied-input run does not establish"));
        assert!(design.contains("Do not guess unpublished schemas"));
    }

    #[test]
    fn artifact_design_preserves_explicit_in_place_edit_authority() {
        let request = "Fix in/program.py in place; preserve in/source.json.";
        let design = render(
            request,
            &Inventory { entries: Vec::new(), truncated: false },
            ExternalEffectRequirement::Optional,
        );

        assert!(design.contains(&format!("> {request}")));
        assert!(design.contains("unless the request explicitly requires editing them in place"));
        assert!(design.contains("output or in-place edit paths"));
        assert!(!design.contains("Input paths are read-only evidence."));
        assert!(!design.contains("There are no retained source modules."));
    }

    #[test]
    fn operational_design_requires_live_effect_and_fresh_verification() {
        let design = render(
            "Start the local service and leave it running.",
            &Inventory { entries: Vec::new(), truncated: false },
            ExternalEffectRequirement::Required,
        );

        assert!(design.contains("live caller-authorized operational result"));
        assert!(design.contains("not completion without the requested effect"));
        assert!(design.contains("`external_effect`"));
        assert!(design.contains("`verification`"));
    }

    #[test]
    fn large_workspace_inventory_is_bounded_without_blocking_design() {
        let root = tempfile::tempdir().expect("artifact workspace");
        for name in ["c.txt", "a.txt", "b.txt"] {
            fs::write(root.path().join(name), name).expect("artifact input");
        }

        let inventory = inventory_with_limit(root.path(), 2).expect("bounded inventory");

        assert!(inventory.truncated);
        assert_eq!(
            inventory.entries.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
            ["a.txt", "b.txt"]
        );
        let design =
            render("Inspect the supplied inputs.", &inventory, ExternalEffectRequirement::Optional);
        assert!(design.contains("deterministic navigation sample truncated after the first 2"));
        assert!(design.contains("does not prove that a path is absent"));
    }
}
