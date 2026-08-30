use super::workflow_command_policy;
use super::workflow_command_policy::CommandPolicy;
use super::{workflow_governance, workflow_governance_jobs};
use crate::error::Diagnostic;
use std::path::Path;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

pub(super) fn reject_defaults(
    mapping: &Hash,
    path: &Path,
    location: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if mapping_value(mapping, "defaults").is_some() {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}.defaults` can replace the inspected run-command interpreter"),
            "remove workflow or job run defaults and use the reviewed shell directly on each permitted step",
        ));
    }
}

pub(super) fn validate_step(
    step: &Hash,
    path: &Path,
    location: &str,
    policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(run) = mapping_value(step, "run") else { return };
    if let Some(shell) = mapping_value(step, "shell")
        && shell.as_str() != Some("bash")
    {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}.shell` is not the exact reviewed `bash` interpreter"),
            "remove the shell override or use literal `bash`; script paths and shell templates are not inspected",
        ));
    }
    let Some(script) = run.as_str() else {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}.run` is not a string"),
            "express the run command as YAML text so dependency policy can inspect it",
        ));
        return;
    };
    if path == Path::new(workflow_governance::PATH)
        && location == "jobs.gate-a.steps[0]"
        && workflow_governance_jobs::gate_status_script_is_exact(script)
    {
        return;
    }
    workflow_command_policy::validate(
        script,
        path,
        &format!("{location}.run"),
        policy,
        diagnostics,
    );
}

fn mapping_value<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}
