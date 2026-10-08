//! Durable console metadata shared by the gateway and the independent PTY owner.

use crate::{
    consoles::Console,
    error::{Result, problem},
    state::save,
};
use peritus_process::ProcessTreeIdentity;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub(super) const RECORD_FILE: &str = "console.json";
pub(super) const OUTPUT_FILE: &str = "output.bin";
pub(super) const LAUNCH_FILE: &str = "launch.json";
const SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CloseDisposition {
    Terminate,
    Dismiss,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Phase {
    Running,
    Exited,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessBinding {
    pub(super) pid: u32,
    pub(super) start_token: u64,
    pub(super) process_group: Option<u32>,
    #[serde(default)]
    pub(super) job: Option<String>,
    pub(super) complete_containment: bool,
}

impl ProcessBinding {
    pub(super) const fn identity(&self) -> ProcessTreeIdentity {
        ProcessTreeIdentity::new(
            self.pid,
            Some(self.start_token),
            self.process_group,
            self.complete_containment,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LaunchManifest {
    schema_version: u32,
    pub(super) operation: String,
    pub(super) console: Console,
    pub(super) token: String,
    pub(super) workspace: String,
    pub(super) store: String,
    pub(super) state_file: PathBuf,
    pub(super) executable: PathBuf,
    pub(super) working_directory: PathBuf,
    pub(super) arguments: Vec<String>,
}

impl LaunchManifest {
    pub(super) fn new(
        operation: String,
        console: Console,
        token: String,
        workspace: String,
        store: String,
        state_file: PathBuf,
        executable: PathBuf,
        working_directory: PathBuf,
        arguments: Vec<String>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            operation,
            console,
            token,
            workspace,
            store,
            state_file,
            executable,
            working_directory,
            arguments,
        }
    }

    pub(super) fn read(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        value.validate(path)?;
        Ok(value)
    }

    pub(super) fn publish(&self, path: &Path) -> Result<()> {
        save(path, &serde_json::to_vec(self)?)
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION
            || path.file_name().and_then(|name| name.to_str()) != Some(LAUNCH_FILE)
            || path.parent().and_then(Path::file_name).and_then(|name| name.to_str())
                != Some(self.console.id.as_str())
            || !valid_identity(&self.console.id)
            || !valid_identity(&self.token)
            || !valid_identity(&self.workspace)
            || !valid_store(&self.store)
            || !valid_operation(&self.operation)
            || !self.state_file.is_absolute()
            || !self.working_directory.is_absolute()
        {
            return Err(problem("Console launch manifest identity is invalid"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConsoleRecord {
    schema_version: u32,
    pub(super) operation: String,
    pub(super) console: Console,
    pub(super) token: String,
    #[serde(default)]
    pub(super) workspace: String,
    #[serde(default)]
    pub(super) store: String,
    #[serde(default)]
    pub(super) state_file: PathBuf,
    pub(super) endpoint: String,
    pub(super) owner: ProcessBinding,
    pub(super) child: ProcessBinding,
    pub(super) phase: Phase,
    pub(super) output_bytes: u64,
    pub(super) exit_code: Option<u32>,
    pub(super) exit_signal: Option<String>,
    pub(super) close_disposition: Option<CloseDisposition>,
    pub(super) close_confirmed: bool,
    pub(super) error: Option<String>,
}

impl ConsoleRecord {
    pub(super) fn running(
        manifest: &LaunchManifest,
        endpoint: SocketAddr,
        owner: ProcessBinding,
        child: ProcessBinding,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            operation: manifest.operation.clone(),
            console: manifest.console.clone(),
            token: manifest.token.clone(),
            workspace: manifest.workspace.clone(),
            store: manifest.store.clone(),
            state_file: manifest.state_file.clone(),
            endpoint: endpoint.to_string(),
            owner,
            child,
            phase: Phase::Running,
            output_bytes: 0,
            exit_code: None,
            exit_signal: None,
            close_disposition: None,
            close_confirmed: false,
            error: None,
        }
    }

    pub(super) fn read(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        value.validate(path)?;
        Ok(value)
    }

    pub(super) fn publish(&self, path: &Path) -> Result<()> {
        self.validate(path)?;
        save(path, &serde_json::to_vec(self)?)
    }

    pub(super) fn endpoint(&self) -> Result<SocketAddr> {
        let address = self.endpoint.parse::<SocketAddr>().map_err(problem)?;
        if !address.ip().is_loopback() {
            return Err(problem("Console owner endpoint is not loopback-only"));
        }
        Ok(address)
    }

    pub(super) const fn ended(&self) -> bool {
        !matches!(self.phase, Phase::Running)
    }

    pub(super) const fn closed(&self) -> bool {
        self.close_confirmed
    }

    pub(super) const fn legacy(&self) -> bool {
        self.schema_version == 1
    }

    fn validate(&self, path: &Path) -> Result<()> {
        let unix_child_binding = cfg!(unix)
            && self.child.process_group == Some(self.child.pid)
            && self.child.job.is_none()
            && self.child.complete_containment;
        let windows_child_binding = cfg!(windows)
            && self.child.process_group.is_none()
            && self.child.job.as_ref().is_some_and(|job| valid_job(job))
            && self.child.complete_containment;
        let legacy_binding = self.schema_version == 1
            && self.workspace.is_empty()
            && self.store.is_empty()
            && self.state_file.as_os_str().is_empty()
            && self.owner.process_group.is_none()
            && self.owner.job.is_none()
            && !self.owner.complete_containment
            && (unix_child_binding
                || cfg!(windows)
                    && self.child.process_group.is_none()
                    && self.child.job.is_none()
                    && !self.child.complete_containment);
        let namespaced_binding = self.schema_version == SCHEMA_VERSION
            && valid_identity(&self.workspace)
            && valid_store(&self.store)
            && self.state_file.is_absolute()
            && (unix_child_binding || windows_child_binding);
        if !(legacy_binding || namespaced_binding)
            || path.file_name().and_then(|name| name.to_str()) != Some(RECORD_FILE)
            || path.parent().and_then(Path::file_name).and_then(|name| name.to_str())
                != Some(self.console.id.as_str())
            || !valid_identity(&self.console.id)
            || !valid_identity(&self.token)
            || !valid_operation(&self.operation)
            || self.owner.pid == 0
            || self.owner.start_token == 0
            || self.owner.process_group.is_some()
            || namespaced_binding && cfg!(unix)
                && (self.owner.job.is_some() || self.owner.complete_containment)
            || namespaced_binding && cfg!(windows)
                && (!self.owner.complete_containment
                    || self.owner.job != self.child.job)
            || self.child.pid == 0
            || self.child.start_token == 0
            || self.close_confirmed && self.close_disposition.is_none()
            || self.close_disposition == Some(CloseDisposition::Dismiss)
                && (!self.close_confirmed || !self.ended())
        {
            return Err(problem("Console ownership record is invalid"));
        }
        self.endpoint()?;
        Ok(())
    }
}

pub(super) fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    if !valid_identity(id) {
        return Err(problem("Console identity is invalid"));
    }
    Ok(root.join(id))
}

pub(super) fn valid_identity(value: &str) -> bool {
    valid_lower_hex(value, 32)
}

fn valid_operation(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_store(value: &str) -> bool {
    valid_lower_hex(value, 64)
}

fn valid_job(value: &str) -> bool {
    value.starts_with("Local\\PeritusConsole-")
        && value.len() == "Local\\PeritusConsole-".len() + 32
        && valid_lower_hex(&value["Local\\PeritusConsole-".len()..], 32)
}

fn valid_lower_hex(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
