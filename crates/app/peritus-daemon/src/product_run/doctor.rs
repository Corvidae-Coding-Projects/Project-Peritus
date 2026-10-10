//! Local diagnostic observations. No provider calls, command execution, repairs, or uploads.

use super::{ProductRunService, ProductRunServiceError};
use peritus_app_protocol::{
    DoctorFinding, DoctorQuery, DoctorReport, DoctorStatus as Status, ProductRunOperationState,
};

impl ProductRunService {
    pub(crate) fn doctor(
        &self,
        query: DoctorQuery,
    ) -> Result<DoctorReport, ProductRunServiceError> {
        let mut findings = vec![finding(
            "daemon",
            Status::Healthy,
            "Authenticated local daemon answered this diagnostic request.",
            "No restart or repair was performed.",
        )?];
        findings.push(self.workspace_diagnostic(query)?);
        let provider_ready =
            query.provider().is_some_and(|id| self.inner.providers.contains_key(&id));
        findings.push(if provider_ready {
            finding(
                "provider-route",
                Status::Healthy,
                "Selected provider route is configured in the running daemon.",
                "Configuration is not proof of authentication or remote availability.",
            )?
        } else {
            finding(
                "provider-route",
                Status::Blocked,
                "No configured provider matches the selected route.",
                "Select or configure a provider explicitly with peritus providers.",
            )?
        });
        findings.push(finding("provider-authentication", Status::Unsupported,
            "No credential value was read or displayed, and no authentication or network probe was run.",
            "Use the provider's explicit authentication flow if a request reports an authentication failure.")?);
        let records =
            self.inner.records.try_read().map_err(|_| ProductRunServiceError::Unavailable)?;
        findings.push(finding(
            "loaded-product-state",
            Status::Healthy,
            "Daemon-owned product records are readable in memory; they were validated at startup.",
            "This is not a full on-disk integrity scan or a repair.",
        )?);
        let (live, unresolved) = records
            .values()
            .filter(|record| record.request.workspace_id() == query.workspace())
            .try_fold((false, false), |(live, unresolved), record| {
                let state = self.project_operation(record)?.state();
                Ok::<_, ProductRunServiceError>((
                    live || matches!(
                        state,
                        ProductRunOperationState::Running
                            | ProductRunOperationState::WaitingForUser
                    ),
                    unresolved
                        || matches!(
                            state,
                            ProductRunOperationState::RecoveryRequired
                                | ProductRunOperationState::OutcomeUnknown
                        ),
                ))
            })?;
        findings.push(if live && unresolved {
            finding(
                "workspace-activity",
                Status::Warning,
                "The selected workspace has live daemon-owned work and an operation requiring explicit reconciliation.",
                "Use the legal controls shown on each run; diagnostics did not pause, retry, acknowledge, or alter either operation.",
            )?
        } else if unresolved {
            finding(
                "workspace-activity",
                Status::Warning,
                "The selected workspace has an operation requiring explicit reconciliation.",
                "Use the legal controls shown on that run; diagnostics did not retry, acknowledge, or alter it.",
            )?
        } else if live {
            finding(
                "workspace-activity",
                Status::Warning,
                "The selected workspace has a live daemon-owned operation.",
                "Diagnostics did not pause, cancel, restart, or alter that work.",
            )?
        } else {
            finding(
                "workspace-activity",
                Status::Healthy,
                "No live or unresolved product operation is registered for the selected workspace.",
                "Opening diagnostics does not start a run.",
            )?
        });
        findings.push(finding(
            "tool-execution",
            Status::Unsupported,
            "Project commands and package scripts were not executed during diagnostics.",
            "Inspect approved command profiles before explicitly running checks.",
        )?);
        findings.push(finding("native-capture", Status::Unsupported,
            "No approved capture target or capture consent is attached to this request.",
            "Select an application window or page through an available capture backend; never capture the desktop implicitly.")?);
        DoctorReport::new(query, findings).map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    fn workspace_diagnostic(
        &self,
        query: DoctorQuery,
    ) -> Result<DoctorFinding, ProductRunServiceError> {
        if let Some(folder) = self.inner.folders.get(&query.workspace()) {
            // Identity was validated on registration. Live filesystem checks are deliberately
            // separate from this bounded, nonblocking inspection of admitted configuration.
            return finding(
                "workspace-policy",
                Status::Healthy,
                if folder.writable() {
                    "Selected folder has explicit in-place write trust."
                } else {
                    "Selected folder is configured read-only."
                },
                "Identity and authority are revalidated at execution; this report grants no permissions.",
            );
        }
        if self.inner.workspaces.contains_key(&query.workspace()) {
            finding(
                "workspace-policy",
                Status::Healthy,
                "Selected workspace is registered with the running daemon.",
                "Current write authority is checked again for each admitted operation.",
            )
        } else {
            finding(
                "workspace-policy",
                Status::Blocked,
                "Selected workspace is not registered in this daemon.",
                "Select the intended workspace explicitly; diagnostics do not register or trust folders.",
            )
        }
    }
}

fn finding(
    check: &str,
    status: Status,
    observation: &str,
    action: &str,
) -> Result<DoctorFinding, ProductRunServiceError> {
    DoctorFinding::new(check.to_owned(), status, observation.to_owned(), action.to_owned())
        .map_err(|_| ProductRunServiceError::InvalidMessage)
}
