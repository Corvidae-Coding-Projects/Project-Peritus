use super::workflow_ci;
use super::workflow_command_policy::CommandPolicy;
use super::workflow_files::{DocumentKind, action_files, workflow_files};
use super::workflow_governance;
use super::workflow_local::{LocalUseKind, validate_local_reference};
use super::workflow_pins::{validate_pin_occurrences, validate_required_ci_pins};
use super::workflow_run;
use crate::error::{Diagnostic, XtaskError};
use crate::model::ToolchainPolicy;
use std::fs;
use std::path::Path;
use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlLoader};

const WORKFLOW_DIRECTORY: &str = ".github/workflows";
const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
const CI_WORKFLOW_ALTERNATE: &str = ".github/workflows/ci.yaml";

pub(super) fn validate(
    root: &Path,
    tools: &ToolchainPolicy,
    command_policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<usize, XtaskError> {
    let mut files = workflow_files(root)?;
    if !files.iter().any(|(path, _)| {
        let path = relative(root, path);
        path == Path::new(CI_WORKFLOW) || path == Path::new(CI_WORKFLOW_ALTERNATE)
    }) {
        diagnostics.push(Diagnostic::at(
            WORKFLOW_DIRECTORY,
            "the canonical ci.yml or ci.yaml workflow is missing",
            "restore the foundation CI workflow so toolchain pins and required gates remain enforced",
        ));
    }
    if !files.iter().any(|(path, _)| relative(root, path) == Path::new(workflow_governance::PATH)) {
        diagnostics.push(Diagnostic::at(
            WORKFLOW_DIRECTORY,
            "the required Gate A status workflow is missing",
            "restore the canonical workflow that emits the repository ruleset's required Gate A check",
        ));
    }
    files.extend(action_files(root)?);
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut action_count = 0;
    for (path, kind) in files {
        let contents =
            fs::read_to_string(&path).map_err(|error| XtaskError::io("read", &path, error))?;
        action_count +=
            validate_document(root, &path, kind, &contents, tools, command_policy, diagnostics);
    }
    Ok(action_count)
}

pub(super) fn validate_document(
    root: &Path,
    path: &Path,
    kind: DocumentKind,
    contents: &str,
    tools: &ToolchainPolicy,
    command_policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    let relative = relative(root, path);
    let documents = match YamlLoader::load_from_str(contents) {
        Ok(documents) => documents,
        Err(error) => {
            diagnostics.push(Diagnostic::at(
                relative,
                format!("YAML is not valid: {error}"),
                "correct the YAML syntax so the complete CI policy can be checked",
            ));
            return 0;
        }
    };
    if documents.len() != 1 {
        diagnostics.push(Diagnostic::at(
            relative,
            format!("expected one YAML document, found {}", documents.len()),
            "use exactly one mapping document per workflow or action manifest",
        ));
        return 0;
    }
    let Some(document) = documents.first() else { return 0 };
    let Some(mapping) = document.as_hash() else {
        diagnostics.push(Diagnostic::at(
            relative,
            "the YAML document root is not a mapping",
            "use a GitHub workflow or action mapping as the document root",
        ));
        return 0;
    };

    validate_pin_occurrences(document, relative, tools, diagnostics);
    if relative == Path::new(CI_WORKFLOW) || relative == Path::new(CI_WORKFLOW_ALTERNATE) {
        validate_required_ci_pins(mapping, relative, tools, diagnostics);
        workflow_ci::validate(mapping, relative, tools, command_policy, diagnostics);
    } else if relative == Path::new(workflow_governance::PATH) {
        validate_required_ci_pins(mapping, relative, tools, diagnostics);
        workflow_governance::validate(mapping, contents, relative, tools, diagnostics);
    }
    match kind {
        DocumentKind::Workflow => {
            validate_workflow(root, mapping, relative, command_policy, diagnostics)
        }
        DocumentKind::Action => {
            validate_action(root, mapping, relative, command_policy, diagnostics)
        }
    }
}

fn validate_workflow(
    root: &Path,
    workflow: &Hash,
    path: &Path,
    command_policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    workflow_run::reject_defaults(workflow, path, "workflow", diagnostics);
    let Some(jobs) = mapping_value(workflow, "jobs").and_then(Yaml::as_hash) else {
        diagnostics.push(Diagnostic::at(
            path,
            "workflow `jobs` is missing or is not a mapping",
            "define workflow jobs as a YAML mapping",
        ));
        return 0;
    };
    jobs.iter()
        .map(|(name, job)| {
            validate_job(
                root,
                job,
                path,
                &format!("jobs.{}", yaml_name(name)),
                command_policy,
                diagnostics,
            )
        })
        .sum()
}

fn validate_action(
    root: &Path,
    action: &Hash,
    path: &Path,
    command_policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    let Some(runs) = mapping_value(action, "runs") else {
        diagnostics.push(Diagnostic::at(
            path,
            "action `runs` is missing",
            "define the action runtime as a YAML mapping",
        ));
        return 0;
    };
    validate_action_runs(root, runs, path, "runs", command_policy, diagnostics)
}

fn validate_job(
    root: &Path,
    job: &Yaml,
    path: &Path,
    location: &str,
    command_policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    match job {
        Yaml::Hash(mapping) => {
            workflow_run::reject_defaults(mapping, path, location, diagnostics);
            validate_job_timeout(mapping, path, location, diagnostics);
            let mut count = mapping_value(mapping, "uses").map_or(0, |uses| {
                validate_uses(
                    root,
                    uses,
                    path,
                    &format!("{location}.uses"),
                    LocalUseKind::Workflow,
                    diagnostics,
                )
            });
            if let Some(steps) = mapping_value(mapping, "steps") {
                count += validate_steps(
                    root,
                    steps,
                    path,
                    &format!("{location}.steps"),
                    command_policy,
                    diagnostics,
                );
            }
            if let Some(merged) = mapping_value(mapping, "<<") {
                count += validate_job(
                    root,
                    merged,
                    path,
                    &format!("{location}.<<"),
                    command_policy,
                    diagnostics,
                );
            }
            count
        }
        Yaml::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                validate_job(
                    root,
                    value,
                    path,
                    &format!("{location}[{index}]"),
                    command_policy,
                    diagnostics,
                )
            })
            .sum(),
        _ => 0,
    }
}

fn validate_job_timeout(
    mapping: &Hash,
    path: &Path,
    location: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let timeout = mapping_value(mapping, "timeout-minutes").and_then(Yaml::as_i64);
    if !timeout.is_some_and(|minutes| (1..=10).contains(&minutes)) {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}` does not have a timeout from 1 through 10 minutes"),
            "split the work into bounded jobs and keep every hosted runner at or below ten minutes",
        ));
    }
}

fn validate_action_runs(
    root: &Path,
    runs: &Yaml,
    path: &Path,
    location: &str,
    command_policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    match runs {
        Yaml::Hash(mapping) => {
            let mut count = mapping_value(mapping, "steps").map_or(0, |steps| {
                validate_steps(
                    root,
                    steps,
                    path,
                    &format!("{location}.steps"),
                    command_policy,
                    diagnostics,
                )
            });
            if let Some(merged) = mapping_value(mapping, "<<") {
                count += validate_action_runs(
                    root,
                    merged,
                    path,
                    &format!("{location}.<<"),
                    command_policy,
                    diagnostics,
                );
            }
            count
        }
        Yaml::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                validate_action_runs(
                    root,
                    value,
                    path,
                    &format!("{location}[{index}]"),
                    command_policy,
                    diagnostics,
                )
            })
            .sum(),
        _ => 0,
    }
}

fn validate_steps(
    root: &Path,
    steps: &Yaml,
    path: &Path,
    location: &str,
    command_policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    match steps {
        Yaml::Hash(mapping) => {
            let mut count = mapping_value(mapping, "uses").map_or(0, |uses| {
                validate_uses(
                    root,
                    uses,
                    path,
                    &format!("{location}.uses"),
                    LocalUseKind::Action,
                    diagnostics,
                )
            });
            workflow_run::validate_step(mapping, path, location, command_policy, diagnostics);
            if let Some(merged) = mapping_value(mapping, "<<") {
                count += validate_steps(
                    root,
                    merged,
                    path,
                    &format!("{location}.<<"),
                    command_policy,
                    diagnostics,
                );
            }
            count
        }
        Yaml::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                validate_steps(
                    root,
                    value,
                    path,
                    &format!("{location}[{index}]"),
                    command_policy,
                    diagnostics,
                )
            })
            .sum(),
        _ => 0,
    }
}

fn validate_uses(
    root: &Path,
    uses: &Yaml,
    path: &Path,
    location: &str,
    local_kind: LocalUseKind,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    let Some(action) = uses.as_str() else {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}` is not a string"),
            "use a literal local path or remote action pinned to a 40-character commit SHA",
        ));
        return 0;
    };
    if action.starts_with("./") {
        validate_local_reference(root, action, path, location, local_kind, diagnostics);
        return 0;
    }

    let valid_remote = action.rsplit_once('@').is_some_and(|(repository, revision)| {
        repository.contains('/')
            && repository.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
            && is_nonzero_hex(revision, 40)
    });
    if !valid_remote {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}` uses `{action}` without an immutable commit SHA"),
            "replace the tag or expression with a reviewed 40-character action commit and retain the release tag as a comment",
        ));
    }
    1
}

fn mapping_value<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}

fn yaml_name(value: &Yaml) -> String {
    value.as_str().map_or_else(|| format!("{value:?}"), ToOwned::to_owned)
}

fn is_nonzero_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().any(|byte| byte != b'0')
}

fn relative<'a>(root: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
}
