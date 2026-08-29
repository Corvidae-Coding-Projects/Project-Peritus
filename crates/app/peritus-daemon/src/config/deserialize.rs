//! Exact Serde adapters for the strict external configuration representation.

use std::path::PathBuf;

use serde::Deserialize;
use serde::Deserializer;

use super::{
    ApprovalRegistryDeclaration, DaemonConfig, DaemonLimits, DaemonPaths, LocalHumanPrincipal,
    ProductRunPolicy, ProjectDeclaration, ProviderRoute, TelemetryExport, ToolPolicy,
    WorkspaceDeclaration,
};

impl<'de> Deserialize<'de> for TelemetryExport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Representation {
            mode: String,
            directory: Option<PathBuf>,
            quota_bytes: Option<u64>,
        }

        let representation = Representation::deserialize(deserializer)?;
        let local_fields_configured =
            representation.directory.is_some() || representation.quota_bytes.is_some();
        match representation.mode.as_str() {
            "disabled" if !local_fields_configured => Ok(Self::Disabled),
            "disabled" => Err(serde::de::Error::custom(
                "disabled telemetry export cannot configure local-file fields",
            )),
            "local-file" => match (representation.directory, representation.quota_bytes) {
                (Some(directory), Some(quota_bytes)) => {
                    Ok(Self::LocalFile { directory, quota_bytes })
                }
                _ => Err(serde::de::Error::custom(
                    "local-file telemetry export requires directory and quota_bytes",
                )),
            },
            _ => Err(serde::de::Error::unknown_variant(
                &representation.mode,
                &["disabled", "local-file"],
            )),
        }
    }
}

impl<'de> Deserialize<'de> for DaemonConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Representation {
            version: u16,
            store_id: String,
            paths: DaemonPaths,
            approval_registry: ApprovalRegistryDeclaration,
            limits: Option<DaemonLimits>,
            human: LocalHumanPrincipal,
            projects: Option<Vec<ProjectDeclaration>>,
            workspaces: Option<Vec<WorkspaceDeclaration>>,
            tools: Option<ToolPolicy>,
            providers: Option<Vec<ProviderRoute>>,
            product: Option<ProductRunPolicy>,
            telemetry: TelemetryExport,
        }

        let representation = Representation::deserialize(deserializer)?;
        Ok(Self {
            version: representation.version,
            store_id: representation.store_id,
            paths: representation.paths,
            approval_registry: representation.approval_registry,
            limits: representation.limits.unwrap_or_default(),
            human: representation.human,
            projects: representation.projects.unwrap_or_default(),
            workspaces: representation.workspaces.unwrap_or_default(),
            tools: representation.tools.unwrap_or_default(),
            providers: representation.providers.unwrap_or_default(),
            product: representation.product.unwrap_or_default(),
            telemetry: representation.telemetry,
        })
    }
}
