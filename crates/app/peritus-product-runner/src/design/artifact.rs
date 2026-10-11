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
    omissions: Vec<String>,
    truncated: bool,
}

pub(super) fn create(input: &ProductRunInput) -> Result<DesignDocument, ProductRunnerError> {
    loop {
        check_cancelled(input)?;
        let revision = input.conversation.revision();
        let transcript = input.conversation.stable_request_context();
        let inventory = inventory(&input.workspace_root, || check_cancelled(input))?;
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

fn inventory(
    root: &Path,
    mut check_cancelled: impl FnMut() -> Result<(), ProductRunnerError>,
) -> Result<Inventory, ProductRunnerError> {
    inventory_cancellable(root, MAX_INVENTORY_ENTRIES, &mut check_cancelled)
}

fn inventory_cancellable(
    root: &Path,
    maximum: usize,
    check_cancelled: &mut impl FnMut() -> Result<(), ProductRunnerError>,
) -> Result<Inventory, ProductRunnerError> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut entries = Vec::new();
    let mut omissions = Vec::new();
    let mut ordinal = 0_usize;
    while let Some(directory) = pending.pop_front() {
        check_cancelled()?;
        let read_dir = match fs::read_dir(&directory) {
            Ok(read_dir) => read_dir,
            Err(error) if directory == root => {
                return Err(repository("inventory artifact workspace", error.to_string()));
            }
            Err(error) => {
                if ordinal == maximum {
                    return Ok(Inventory { entries, omissions, truncated: true });
                }
                omissions.push(format!("{}: read directory: {error}", directory.display()));
                ordinal = ordinal.saturating_add(1);
                continue;
            }
        };
        let remaining = maximum.saturating_sub(ordinal).saturating_add(1).max(1);
        let candidates = scan_directory(root, read_dir, remaining, check_cancelled)?;
        for child in candidates.children {
            if ordinal == maximum {
                return Ok(Inventory { entries, omissions, truncated: true });
            }
            let path = child.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| repository("inventory artifact workspace", "path escaped workspace"))?
                .to_path_buf();
            check_cancelled()?;
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    omissions.push(format!("{}: inspect entry: {error}", relative.display()));
                    ordinal = ordinal.saturating_add(1);
                    continue;
                }
            };
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
            ordinal = ordinal.saturating_add(1);
        }
        for error in candidates.iterator_errors {
            if ordinal == maximum {
                return Ok(Inventory { entries, omissions, truncated: true });
            }
            omissions.push(format!("{}: enumerate child: {error}", directory.display()));
            ordinal = ordinal.saturating_add(1);
        }
        if candidates.overflow && ordinal >= maximum {
            return Ok(Inventory { entries, omissions, truncated: true });
        }
    }
    Ok(Inventory { entries, omissions, truncated: false })
}

struct DirectoryCandidates {
    children: Vec<fs::DirEntry>,
    iterator_errors: Vec<String>,
    overflow: bool,
}

fn scan_directory(
    root: &Path,
    read_dir: fs::ReadDir,
    remaining: usize,
    check_cancelled: &mut impl FnMut() -> Result<(), ProductRunnerError>,
) -> Result<DirectoryCandidates, ProductRunnerError> {
    let mut children = Vec::new();
    let mut iterator_errors = Vec::new();
    let mut overflow = false;
    for child in read_dir {
        check_cancelled()?;
        match child {
            Ok(child) => {
                let path = child.path();
                let relative = path.strip_prefix(root).map_err(|_| {
                    repository("inventory artifact workspace", "path escaped workspace")
                })?;
                if ignored(relative) {
                    continue;
                }
                let name = child.file_name();
                let index = children
                    .binary_search_by(|entry: &fs::DirEntry| entry.file_name().cmp(&name))
                    .unwrap_or_else(|index| index);
                if children.len() < remaining {
                    children.insert(index, child);
                } else if index < remaining {
                    children.insert(index, child);
                    children.pop();
                    overflow = true;
                } else {
                    overflow = true;
                }
            }
            Err(error) => {
                if iterator_errors.len() < remaining {
                    iterator_errors.push(error.to_string());
                } else {
                    overflow = true;
                }
            }
        }
    }
    Ok(DirectoryCandidates { children, iterator_errors, overflow })
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
        design.push_str(
            &inventory.entries.len().saturating_add(inventory.omissions.len()).to_string(),
        );
        design.push_str(
            " visible entries and omissions. Omission from this sample does not prove that a path is absent. Use the bounded workspace listing, search, and read tools to inspect exact paths relevant to the request.\n",
        );
        design.push_str(
            "Restart exact inspection from the first page with `workspace_list` using `path=\".\"` and the needed depth, with no cursor. The inventory sample's internal ordinal is not a workspace-list cursor; follow only the cursor returned by `workspace_list` for later pages.\n",
        );
    }
    if !inventory.omissions.is_empty() {
        design.push_str("\n## Inventory omissions\n\n");
        for omission in &inventory.omissions {
            design.push_str("- ");
            design.push_str(omission);
            design.push('\n');
        }
    }
    design.push_str(
        "\n## Architecture and interfaces\n\nThe original request is the input/output contract. Input paths remain read-only unless the request explicitly requires editing them in place. The writer owns only the explicitly requested output or in-place edit paths and uses `workspace_list`, `workspace_read`, `workspace_write`, `workspace_patch`, `workspace_remove`, and non-destructive `run_command` calls as needed. No package scaffold, retained producer, dependency, network access, or extra artifact is introduced unless the request explicitly requires it.\n\n\
## Data and control flow\n\n1. List the workspace and read the exact current-round inputs named by the request.\n2. If inputs arrive over time, observe them for the full requested interval and perform the required final poll before freezing state.\n3. Apply the request's literal filtering, ordering, deduplication, transformation, and preservation rules.\n4. Build every requested output from one consistent observed state and write independent outputs together when they have no data dependency.\n5. Re-read the outputs and run the applicable host-owned artifact gates before completion.\n\n\
## File and module plan\n\nNo source modules are introduced unless requested. Existing inputs and harness-owned files remain untouched unless the request explicitly requires changing them. Persistent changes are limited to the output or in-place edit paths named in the authoritative conversation; temporary files or directories are removed when the request requires cleanup.\n\n\
## Implementation slices\n\n- **Observe:** inventory and read only the current request's authoritative inputs, including required polling or staged-input boundaries.\n- **Transform:** compute the requested state deterministically while preserving literal identifiers and first-seen or ordering semantics.\n- **Publish:** create the complete requested artifacts without unrelated files or source scaffolding.\n- **Verify:** parse or re-read each final artifact, check cross-artifact consistency, and confirm prohibited effects did not occur.\n\n\
## Verification\n\nVerification must cover every explicit acceptance statement in the conversation, validate the syntax of structured outputs, confirm exact required fields and literal values, and inspect filesystem effects. When acceptance depends on an empirical quality, size, speed, or resource threshold, prepare reusable inputs once when practical, keep a compact candidate ledger of parameters and measured results, preserve the best valid candidate atomically, and use bounded low-cost experiments before expensive full candidates. Keep selection data distinct from the final acceptance holdout: iterate on a training split or cross-validation, then consult the final holdout for the selected candidate rather than repeatedly choosing against it. If a holdout has already guided selection, account for that bias and require a defensible margin or independent evidence for a near-threshold claim. Re-run the selected candidate through the authoritative end-to-end measurement; a training or search metric is not final acceptance. When an empirical or heuristic producer is calibrated from one supplied example but must generalize, reserve an independent segment or use contract-preserving perturbations with known expected relationships; rerunning only the calibration sample is insufficient. When a deliverable accepts inputs beyond the supplied example, exercise at least one independently created or independently selected input and derive format fields, dimensions, offsets, identifiers, and defaults from the authoritative input contract. Treat example-derived constants as hypotheses that must be varied or proved invariant; one successful supplied-input run does not establish a parameterized interface. The product's independent artifact gates remain authoritative for supported formats. A successful write is not completion until the final files are re-read and the requested outcome is checked.\n\n\
## Risks and explicit non-goals\n\nDo not guess unpublished schemas or hidden evaluator conventions. Do not read future-stage or adjacent inputs merely because they are visible. Do not modify input fixtures unless explicitly requested, use network access when excluded, retain helper code, or add package infrastructure for a one-run artifact task. Report an actual source contradiction rather than silently changing the contract.\n\n\
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
