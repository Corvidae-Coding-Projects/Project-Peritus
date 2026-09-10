//! Redacted launch-time observations. Opening `/doctor` does not rerun bootstrap or setup.

use crate::{LauncherError, PreparedProduct, SiblingBinaries};
use peritus_app_protocol::{DoctorFinding, DoctorQuery, DoctorReport, DoctorStatus};
use peritus_types::WorkspaceId;

pub(super) fn launcher_report(
    prepared: &PreparedProduct,
    binaries: &SiblingBinaries,
    workspace: WorkspaceId,
) -> Result<DoctorReport, LauncherError> {
    // SiblingBinaries can only be constructed through its checked discovery paths. Do not
    // execute --version again from diagnostics: a check is not authority to rerun executables.
    let findings = [
        ("launcher-binary-agreement", DoctorStatus::Healthy,
            format!("At launch: application and sibling daemon passed the packaged version {} check.", binaries.verified_version()),
            "This is a launch-time observation, not a fresh executable probe."),
        ("launcher-product-store", DoctorStatus::Healthy,
            "At launch: local product state and strict daemon configuration were read and validated.".to_owned(),
            "No database repair or configuration regeneration is performed by /doctor."),
        ("launcher-provider-references", DoctorStatus::Healthy,
            format!("At launch: {} selected provider routes passed configuration validation; no credential values are included.", prepared.daemon_config().providers().len()),
            "Reference validity is not proof of current authentication. Use explicit provider setup if needed."),
        ("launcher-tool-policy", DoctorStatus::Healthy,
            format!("At launch: {} C4 tool allowlist entries were configured. Product workspace tools use their separate execution policy.", prepared.daemon_config().tools().allowed().len()),
            "No project command or package script was executed as a diagnostic check."),
    ].into_iter().map(|(check, status, observation, action)| {
        DoctorFinding::new(check.to_owned(), status, observation, action.to_owned()).map_err(|error| protocol_error(&error))
    }).collect::<Result<Vec<_>, _>>()?;
    DoctorReport::new(DoctorQuery::new(workspace, None), findings)
        .map_err(|error| protocol_error(&error))
}

fn protocol_error(error: &peritus_app_protocol::AppProtocolError) -> LauncherError {
    LauncherError::Tui(peritus_tui::TuiError::InvalidValue(format!(
        "invalid launcher diagnostic: {}",
        error.code().as_str()
    )))
}
