//! One explicit scope contract shared by planning and deterministic gate admission.

use std::{fs, io::ErrorKind, path::Path};

use crate::{GateError, GateErrorKind, GateRecoveryAction};

/// Declared kind of work delivered by a workspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceProductScope {
    /// Retained source implementation, also the default when no marker is present.
    Source,
    /// Requested generated artifacts without an implicit source scaffold.
    Artifact,
}

impl WorkspaceProductScope {
    /// Reads the optional `peritus-workspace.toml` contract without changing its authority.
    ///
    /// # Errors
    /// Rejects an unreadable, malformed, unknown, or incompatible declared contract. Only an
    /// actually absent optional marker selects the source default.
    pub fn read(root: &Path) -> Result<Self, GateError> {
        let path = root.join("peritus-workspace.toml");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if matches!(
                    fs::symlink_metadata(&path),
                    Err(ref absent) if absent.kind() == ErrorKind::NotFound
                ) {
                    return Ok(Self::Source);
                }
                return Err(GateError::sourced(
                    GateErrorKind::Workspace,
                    GateRecoveryAction::CorrectInput,
                    "declared workspace scope cannot be read",
                    error,
                ));
            }
            Err(error) => {
                return Err(GateError::sourced(
                    GateErrorKind::Workspace,
                    GateRecoveryAction::CorrectInput,
                    "declared workspace scope cannot be read",
                    error,
                ));
            }
        };
        let value = toml::from_str::<toml::Value>(&text).map_err(|error| {
            GateError::sourced(
                GateErrorKind::Workspace,
                GateRecoveryAction::CorrectInput,
                "declared workspace scope is invalid TOML",
                error,
            )
        })?;
        let table = value.as_table().ok_or_else(|| invalid("workspace scope must be a table"))?;
        if table.len() != 2 {
            return Err(invalid("workspace scope must declare only schema_version and kind"));
        }
        if table.get("schema_version").and_then(toml::Value::as_integer) != Some(1) {
            return Err(invalid("workspace scope schema_version is unsupported"));
        }
        match table.get("kind").and_then(toml::Value::as_str) {
            Some("artifact") => Ok(Self::Artifact),
            Some("source") => Ok(Self::Source),
            _ => Err(invalid("workspace scope kind must be artifact or source")),
        }
    }
}

fn invalid(detail: &'static str) -> GateError {
    GateError::new(GateErrorKind::Workspace, GateRecoveryAction::CorrectInput, detail)
}
