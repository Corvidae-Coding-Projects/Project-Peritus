//! Deterministic detailed designs for explicit generated-artifact workspaces.

use std::{collections::VecDeque, fs, path::Path};

use super::{DesignDocument, publish};
use crate::delivery_requirement::ExternalEffectRequirement;
use crate::execution::{ProductRunInput, check_cancelled};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_INVENTORY_ENTRIES: usize = 2_000;

struct InventoryEntry {
    path: String,
    kind: &'static str,
    bytes: Option<u64>,
}

struct Inventory {
    entries: Vec<InventoryEntry>,
    truncated: bool,
}

pub(super) fn create(input: &ProductRunInput) -> Result<DesignDocument, ProductRunnerError> {
    loop {
        check_cancelled(input)?;
        let revision = input.conversation.revision();
        let transcript = input.conversation.render();
        let inventory = inventory(&input.workspace_root)?;
        if input.conversation.revision() != revision {
            continue;
        }
        let requirement = ExternalEffectRequirement::from_task(input.delivery_scope, &input.task);
        let markdown = render(&transcript, &inventory, requirement);
        let path = input.trace_path.with_extension("design.md");
        publish(&path, markdown.as_bytes())?;
        return Ok(DesignDocument { path, markdown, conversation_revision: revision });
    }
}

fn inventory(root: &Path) -> Result<Inventory, ProductRunnerError> {
    inventory_with_limit(root, MAX_INVENTORY_ENTRIES)
}

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
Produce the complete requested artifacts in this explicit artifact workspace. The following exact conversation remains the authoritative requirement ledger; every named input, output, value, constraint, exclusion, timing rule, and side effect is acceptance-critical:\n\n",
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
        "\n## Repository findings\n\nThe workspace declares `schema_version = 1` and `kind = \"artifact\"`. It has no retained implementation requirement, so the writer should use the bounded workspace tools directly and leave only the requested outputs. The design inventory observed before mutation is:\n\n",
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
        design.push_str("\nThe design inventory is a deterministic navigation sample truncated after the first ");
        design.push_str(&inventory.entries.len().to_string());
        design.push_str(
            " non-ignored entries. Omission from this sample does not prove that a path is absent. Use the bounded workspace listing, search, and read tools to inspect exact paths relevant to the request.\n",
        );
    }
    design.push_str(
        "\n## Architecture and interfaces\n\nThe original request is the input/output contract. Input paths are read-only evidence. The writer owns only the explicitly requested output paths and uses `workspace_list`, `workspace_read`, `workspace_write`, `workspace_patch`, `workspace_remove`, and non-destructive `run_command` calls as needed. No package scaffold, retained producer, dependency, network access, or extra artifact is introduced unless the request explicitly requires it.\n\n\
## Data and control flow\n\n1. List the workspace and read the exact current-round inputs named by the request.\n2. If inputs arrive over time, observe them for the full requested interval and perform the required final poll before freezing state.\n3. Apply the request's literal filtering, ordering, deduplication, transformation, and preservation rules.\n4. Build every requested output from one consistent observed state and write independent outputs together when they have no data dependency.\n5. Re-read the outputs and run the applicable host-owned artifact gates before completion.\n\n\
## File and module plan\n\nThere are no retained source modules. Existing inputs and harness-owned files remain untouched. Persistent changes are limited to the output paths named in the authoritative conversation; temporary files or directories are removed when the request requires cleanup.\n\n\
## Implementation slices\n\n- **Observe:** inventory and read only the current request's authoritative inputs, including required polling or staged-input boundaries.\n- **Transform:** compute the requested state deterministically while preserving literal identifiers and first-seen or ordering semantics.\n- **Publish:** create the complete requested artifacts without unrelated files or source scaffolding.\n- **Verify:** parse or re-read each final artifact, check cross-artifact consistency, and confirm prohibited effects did not occur.\n\n\
## Verification\n\nVerification must cover every explicit acceptance statement in the conversation, validate the syntax of structured outputs, confirm exact required fields and literal values, and inspect filesystem effects. When acceptance depends on an empirical quality, size, speed, or resource threshold, prepare reusable inputs once when practical, keep a compact candidate ledger of parameters and measured results, preserve the best valid candidate atomically, and use bounded low-cost experiments before expensive full candidates. Keep selection data distinct from the final acceptance holdout: iterate on a training split or cross-validation, then consult the final holdout for the selected candidate rather than repeatedly choosing against it. If a holdout has already guided selection, account for that bias and require a defensible margin or independent evidence for a near-threshold claim. Re-run the selected candidate through the authoritative end-to-end measurement; a training or search metric is not final acceptance. When an empirical or heuristic producer is calibrated from one supplied example but must generalize, reserve an independent segment or use contract-preserving perturbations with known expected relationships; rerunning only the calibration sample is insufficient. When a deliverable accepts inputs beyond the supplied example, exercise at least one independently created or independently selected input and derive format fields, dimensions, offsets, identifiers, and defaults from the authoritative input contract. Treat example-derived constants as hypotheses that must be varied or proved invariant; one successful supplied-input run does not establish a parameterized interface. The product's independent artifact gates remain authoritative for supported formats. A successful write is not completion until the final files are re-read and the requested outcome is checked.\n\n\
## Risks and explicit non-goals\n\nDo not guess unpublished schemas or hidden evaluator conventions. Do not read future-stage or adjacent inputs merely because they are visible. Do not modify input fixtures, use network access when excluded, retain helper code, or add package infrastructure for a one-run artifact task. Report an actual source contradiction rather than silently changing the contract.\n\n\
## Repository grounding evidence\n\nThis design was rendered by the Rust product runner from the exact durable conversation and a bounded, sorted filesystem inventory. It did not rely on unverified model claims about repository contents.\n",
    );
    if effect_requirement.is_required() {
        design.push_str(
            "\n## Live operational delivery\n\nExecute the requested operation with command purpose `external_effect`, then perform a later deterministic state or end-to-end check with purpose `verification`. Both must succeed. Supporting files remain secondary to that observed live result.\n",
        );
    }
    design
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
