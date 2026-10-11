//! Run-owned persistent computer control through the existing C4/C2 process runtime.

use super::{CommandRuntime, StartCommand};
use crate::developer_tools::path::tool;
use peritus_agent::DeveloperLoopError;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

pub(super) struct ComputerDriver {
    directory: PathBuf,
    handle: String,
}

impl Drop for ComputerDriver {
    fn drop(&mut self) {
        // The controller closes only sessions it owns; attached user apps remain running.
        let _ = peritus_tools_computer::request(
            &self.directory,
            "host-cleanup",
            "shutdown",
            &json!({}),
        );
    }
}

impl CommandRuntime {
    pub(in crate::developer_tools) fn computer_request(
        &self,
        id: &str,
        method: &str,
        params: &Value,
        create: bool,
    ) -> Result<Value, DeveloperLoopError> {
        if tokio::runtime::Handle::try_current().is_ok_and(|handle| {
            matches!(handle.runtime_flavor(), tokio::runtime::RuntimeFlavor::MultiThread)
        }) {
            return tokio::task::block_in_place(|| {
                self.computer_request_inner(id, method, params, create)
            });
        }
        self.computer_request_inner(id, method, params, create)
    }

    fn computer_request_inner(
        &self,
        id: &str,
        method: &str,
        params: &Value,
        create: bool,
    ) -> Result<Value, DeveloperLoopError> {
        let mut driver = self
            .inner
            .computer
            .lock()
            .map_err(|_| tool("computer controller state is poisoned"))?;
        if driver.is_none() {
            if !create {
                return Err(tool(
                    "No live computer session exists in this run; the writer must open or attach first",
                ));
            }
            let mut nonce = [0_u8; 16];
            getrandom::fill(&mut nonce).map_err(|error| tool(error.to_string()))?;
            let name = nonce.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
            let directory = self.inner.state_root.join("computer").join(name);
            let script = peritus_tools_computer::prepare_driver(&directory).map_err(tool)?;
            let python = peritus_tools_computer::python().map_err(tool)?;
            let python =
                python.to_str().ok_or_else(|| tool("automation Python path is not UTF-8"))?;
            let arguments = vec![
                "-u".into(),
                script.to_string_lossy().into_owned(),
                "serve".into(),
                "--state".into(),
                directory.to_string_lossy().into_owned(),
                "--workspace".into(),
                self.inner.workspace_root.to_string_lossy().into_owned(),
                "--owner-pid".into(),
                std::process::id().to_string(),
            ];
            let environment = [
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "XDG_RUNTIME_DIR",
                "XDG_SESSION_TYPE",
                "XDG_CURRENT_DESKTOP",
                "DBUS_SESSION_BUS_ADDRESS",
                "AT_SPI_BUS_ADDRESS",
                "GTK_MODULES",
                "NO_AT_BRIDGE",
            ]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok().map(|value| (key.to_owned(), value)))
            .collect();
            let started = self.start(StartCommand {
                program: python,
                arguments: &arguments,
                cwd: &self.inner.workspace_root,
                timeout: None,
                interactive: false,
                rows: 24,
                columns: 80,
                idempotency_key: &format!("computer-controller-{id}"),
                environment,
                owner_registered: None,
            })?;
            let handle = started["handle"]
                .as_str()
                .ok_or_else(|| tool("computer controller has no native command handle"))?
                .to_owned();
            let started_at = Instant::now();
            while !directory.join("ready.json").is_file() {
                let state = self.poll(&handle)?;
                if state["state"].as_str() != Some("running")
                    || started_at.elapsed() > Duration::from_secs(20)
                {
                    let _ = self.cancel(&handle);
                    return Err(tool(format!(
                        "computer controller could not start: {state}; run peritus automation setup"
                    )));
                }
                thread::sleep(Duration::from_millis(40));
            }
            *driver = Some(ComputerDriver { directory, handle });
        }
        let owned = driver.as_ref().ok_or_else(|| tool("computer controller is unavailable"))?;
        if self.poll(&owned.handle)?["state"].as_str() != Some("running") {
            return Err(tool(
                "Computer controller stopped; its GUI sessions were lost. Start a new run and observe fresh state.",
            ));
        }
        peritus_tools_computer::request(&owned.directory, id, method, params).map_err(tool)
    }

    pub(in crate::developer_tools) fn computer_image(
        &self,
        image: &Value,
    ) -> Result<Vec<u8>, DeveloperLoopError> {
        let driver = self
            .inner
            .computer
            .lock()
            .map_err(|_| tool("computer controller state is poisoned"))?;
        let owned = driver.as_ref().ok_or_else(|| tool("screenshot has no owning controller"))?;
        let path = PathBuf::from(
            image["path"].as_str().ok_or_else(|| tool("screenshot path is missing"))?,
        );
        if path.is_symlink()
            || !path
                .canonicalize()
                .map_err(|error| tool(error.to_string()))?
                .starts_with(&owned.directory)
        {
            return Err(tool("screenshot is outside its owning controller"));
        }
        let metadata = fs::metadata(&path).map_err(|error| tool(error.to_string()))?;
        if !metadata.is_file() || metadata.len() > 4 * 1024 * 1024 {
            return Err(tool("screenshot exceeds its file bound"));
        }
        let bytes = fs::read(path).map_err(|error| tool(error.to_string()))?;
        let digest =
            Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        if image["sha256"].as_str() != Some(digest.as_str())
            || image["bytes"].as_u64() != Some(bytes.len() as u64)
        {
            return Err(tool("screenshot differs from the actual control observation"));
        }
        Ok(bytes)
    }
}
