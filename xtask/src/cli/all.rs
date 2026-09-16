use super::write_output;
use crate::error::XtaskError;
use crate::model::ArchitecturePolicy;
use crate::{api_contract, architecture, metadata, reproducibility, trust};
use std::io::Write;
use std::path::Path;

pub(super) fn execute(root: &Path, output: &mut dyn Write) -> Result<(), XtaskError> {
    let policy = metadata::architecture_policy(root)?;
    if trust::check_local_authorization(root, &policy)? {
        return write_output(output, "all checks passed for the exact authorized candidate\n");
    }
    let counts = execute_checks(root, &policy, TrustMode::Local)?;
    write_summary(output, &counts)
}

pub(crate) fn check_candidate(root: &Path) -> Result<(), XtaskError> {
    let policy = metadata::architecture_policy(root)?;
    execute_checks(root, &policy, TrustMode::Candidate).map(|_| ())
}

#[derive(Clone, Copy)]
enum TrustMode {
    Local,
    Candidate,
}

struct Counts {
    packages: usize,
    source_files: usize,
    api_files: usize,
    executable_entry_points: usize,
    trust_files: usize,
    documentation_files: usize,
    actions: usize,
}

fn execute_checks(
    root: &Path,
    policy: &ArchitecturePolicy,
    trust_mode: TrustMode,
) -> Result<Counts, XtaskError> {
    let (packages, source_files) = architecture::check(root, policy)?;
    let api = api_contract::check(root, policy)?;
    let documentation_files = crate::documentation::check(root)?;
    let trust_files = match trust_mode {
        TrustMode::Local => trust::check_local(root, policy)?,
        TrustMode::Candidate => trust::check_candidate(root, policy)?,
    };
    let tools = metadata::toolchain_policy(root)?;
    let actions = reproducibility::check(root, &tools)?;
    Ok(Counts {
        packages,
        source_files,
        api_files: api.files,
        executable_entry_points: api.executable_entry_points,
        trust_files,
        documentation_files,
        actions,
    })
}

fn write_summary(output: &mut dyn Write, counts: &Counts) -> Result<(), XtaskError> {
    write_output(
        output,
        &format!(
            "all checks passed: {} package(s), {} source file(s), \
             {} formal-boundary file(s), {} ordinary-safe executable entry point(s), \
             {} trust-scanned file(s), {} documentation file(s), \
             {} pinned action(s)\n",
            counts.packages,
            counts.source_files,
            counts.api_files,
            counts.executable_entry_points,
            counts.trust_files,
            counts.documentation_files,
            counts.actions,
        ),
    )
}
