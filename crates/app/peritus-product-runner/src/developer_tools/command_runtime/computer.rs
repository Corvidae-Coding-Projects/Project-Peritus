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
    pub(in crate::developer_tools) fn computer_document(
        &self,
        result: &Value,
    ) -> Result<String, DeveloperLoopError> {
        let directory = {
            let driver =
                self.inner.web_reader.lock().map_err(|_| tool("web reader state is poisoned"))?;
            driver.as_ref().ok_or_else(|| tool("web document has no controller"))?.directory.clone()
        };
        let path = PathBuf::from(
            result["path"].as_str().ok_or_else(|| tool("web document path is missing"))?,
        );
        let expected = directory.join("web-readings");
        if path.is_symlink() || path.parent() != Some(expected.as_path()) {
            return Err(tool("web document is outside its owning controller"));
        }
        if fs::metadata(&path).map_err(|e| tool(e.to_string()))?.len() > 64 * 1024 * 1024 {
            return Err(tool("web document exceeds its resource bound"));
        }
        let bytes = fs::read(&path).map_err(|e| tool(e.to_string()))?;
        let digest = crate::encode_hex(&Sha256::digest(&bytes));
        if result["sha256"].as_str() != Some(digest.as_str())
            || result["bytes"].as_u64() != Some(bytes.len() as u64)
        {
            return Err(tool("web document differs from its actual browser observation"));
        }
        let html = String::from_utf8(bytes).map_err(|e| tool(e.to_string()))?;
        fs::remove_file(path).map_err(|e| tool(e.to_string()))?;
        Ok(html)
    }

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
        let slot = if method == "web" { &self.inner.web_reader } else { &self.inner.computer };
        let mut driver = slot.lock().map_err(|_| tool("computer controller state is poisoned"))?;
        if method == "web"
            && let Some(owned) = driver.as_ref()
            && self.poll(&owned.handle)?["state"].as_str() != Some("running")
        {
            // A completed cancellation or crashed reader is replaceable within the same run.
            *driver = None;
        }
        if driver.is_none() {
            if !create {
                return Err(tool(
                    "No live computer session exists in this run; the writer must open or attach first",
                ));
            }
            *driver = Some(self.start_computer_driver(id, method)?);
        }
        let owned = driver.as_ref().ok_or_else(|| tool("computer controller is unavailable"))?;
        if self.poll(&owned.handle)?["state"].as_str() != Some("running") {
            return Err(tool(
                "Computer controller stopped; its GUI sessions were lost. Start a new run and observe fresh state.",
            ));
        }
        let result = match peritus_tools_computer::request(&owned.directory, id, method, params) {
            Err(error) if method == "web" => {
                // End a wedged read's owned process tree without affecting interactive GUI sessions.
                let mut cancelled = self.cancel(&owned.handle)?;
                let deadline = Instant::now() + Duration::from_secs(5);
                while cancelled["state"].as_str() == Some("running") && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(50));
                    cancelled = self.poll(&owned.handle)?;
                }
                if cancelled["state"].as_str() == Some("running") {
                    return Err(tool(format!(
                        "web reader cancellation is pending after {error}; retained its process owner for recovery"
                    )));
                }
                *driver = None;
                Err(tool(format!(
                    "web reader stopped after {error}; retry opens a fresh isolated reader; existing GUI sessions are preserved"
                )))
            }
            result => result.map_err(tool),
        };
        drop(driver);
        result
    }

    fn start_computer_driver(
        &self,
        id: &str,
        method: &str,
    ) -> Result<ComputerDriver, DeveloperLoopError> {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|error| tool(error.to_string()))?;
        let directory = self
            .inner
            .state_root
            .join(if method == "web" { "web-reader" } else { "computer" })
            .join(crate::encode_hex(&nonce));
        let script = peritus_tools_computer::prepare_driver(&directory).map_err(tool)?;
        let python = peritus_tools_computer::python().map_err(tool)?;
        let python = python.to_str().ok_or_else(|| tool("automation Python path is not UTF-8"))?;
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
        let mut environment = desktop_environment();
        environment.push((
            "PLAYWRIGHT_BROWSERS_PATH".into(),
            peritus_tools_computer::browser_cache().map_err(tool)?.to_string_lossy().into_owned(),
        ));
        if let Ok((socket, token_file)) =
            peritus_tools_computer::chrome_extension_bridge_environment()
        {
            environment.push((
                "PERITUS_CHROME_EXTENSION_SOCKET".into(),
                socket.to_string_lossy().into_owned(),
            ));
            environment.push((
                "PERITUS_CHROME_EXTENSION_TOKEN_FILE".into(),
                token_file.to_string_lossy().into_owned(),
            ));
        }
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
        Ok(ComputerDriver { directory, handle })
    }

    pub(in crate::developer_tools) fn computer_image(
        &self,
        image: &Value,
    ) -> Result<Vec<u8>, DeveloperLoopError> {
        let directory = {
            let driver = self
                .inner
                .computer
                .lock()
                .map_err(|_| tool("computer controller state is poisoned"))?;
            driver
                .as_ref()
                .ok_or_else(|| tool("screenshot has no owning controller"))?
                .directory
                .clone()
        };
        let path = PathBuf::from(
            image["path"].as_str().ok_or_else(|| tool("screenshot path is missing"))?,
        );
        if path.is_symlink()
            || !path
                .canonicalize()
                .map_err(|error| tool(error.to_string()))?
                .starts_with(&directory)
        {
            return Err(tool("screenshot is outside its owning controller"));
        }
        let metadata = fs::metadata(&path).map_err(|error| tool(error.to_string()))?;
        if !metadata.is_file() || metadata.len() > 4 * 1024 * 1024 {
            return Err(tool("screenshot exceeds its file bound"));
        }
        let bytes = fs::read(path).map_err(|error| tool(error.to_string()))?;
        let digest = crate::encode_hex(&Sha256::digest(&bytes));
        if image["sha256"].as_str() != Some(digest.as_str())
            || image["bytes"].as_u64() != Some(bytes.len() as u64)
        {
            return Err(tool("screenshot differs from the actual control observation"));
        }
        Ok(bytes)
    }
}

fn desktop_environment() -> Vec<(String, String)> {
    [
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
    .collect()
}
