//! Owned process/Wasm subprocess and bounded framed exchange.

use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use peritus_plugin_sdk::{
    PluginRequestEnvelope, PluginResponseEnvelope, RequestId, decode_frame, encode_frame,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    process::{ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

use crate::{HostCancellation, HostError, HostFailureClass, RecoveryDisposition};

#[derive(Clone, Debug)]
pub enum LaunchPlan {
    Process { executable: PathBuf, arguments: Vec<String>, working_directory: PathBuf },
    Wasm { runtime: PathBuf, module: PathBuf, arguments: Vec<String>, working_directory: PathBuf },
}

pub struct PluginConnection {
    child: Mutex<Box<dyn process_wrap::tokio::ChildWrapper>>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<ChildStdout>,
    transaction: Mutex<()>,
    frame_bytes: u32,
}

impl PluginConnection {
    pub(crate) fn spawn(plan: LaunchPlan, frame_bytes: u32) -> Result<Arc<Self>, HostError> {
        let mut command = match plan {
            LaunchPlan::Process { executable, arguments, working_directory } => {
                let mut command = Command::new(executable);
                command.args(arguments).current_dir(working_directory);
                command
            }
            LaunchPlan::Wasm { runtime, module, arguments, working_directory } => {
                let mut command = Command::new(runtime);
                command
                    .arg("run")
                    .arg("--")
                    .arg(module)
                    .args(arguments)
                    .current_dir(working_directory);
                command
            }
        };
        command
            .env_clear()
            .env("PERITUS_PLUGIN_PROTOCOL", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut command = process_wrap::tokio::CommandWrap::from(command);
        #[cfg(unix)]
        command.wrap(process_wrap::tokio::ProcessSession);
        #[cfg(windows)]
        command.wrap(process_wrap::tokio::JobObject);
        command.wrap(process_wrap::tokio::KillOnDrop);
        let mut child = command.spawn().map_err(|error| {
            HostError::with_source(
                HostFailureClass::Infrastructure,
                RecoveryDisposition::CorrectRequest,
                "launch isolated plugin",
                error.to_string(),
                error,
            )
        })?;
        let stdin = child
            .stdin()
            .take()
            .ok_or_else(|| transport_error("launched plugin stdin is unavailable"))?;
        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| transport_error("launched plugin stdout is unavailable"))?;
        Ok(Arc::new(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(stdout),
            transaction: Mutex::new(()),
            frame_bytes,
        }))
    }

    pub(crate) async fn exchange(
        &self,
        request: PluginRequestEnvelope,
        timeout: Duration,
        cancellation: &HostCancellation,
        stopped: &HostCancellation,
    ) -> Result<PluginResponseEnvelope, HostError> {
        // Validate before admission: malformed, unsent requests cannot poison a healthy process.
        let frame = encode_frame(&request, self.frame_bytes).map_err(|error| {
            HostError::with_source(
                HostFailureClass::Protocol,
                RecoveryDisposition::CorrectRequest,
                "encode plugin request",
                error.to_string(),
                error,
            )
        })?;
        let mut possibly_sent = false;
        let result = {
            let exchange = async {
                let _transaction = self.transaction.lock().await;
                possibly_sent = true;
                let mut stdin = self.stdin.lock().await;
                stdin.write_all(&frame).await.map_err(|e| io_error("write plugin request", e))?;
                stdin.flush().await.map_err(|e| io_error("flush plugin request", e))?;
                drop(stdin);
                let response = self.read().await?;
                if response.request_id != request.request_id
                    || response.protocol_version != request.protocol_version
                {
                    return Err(HostError::new(
                        HostFailureClass::Protocol,
                        RecoveryDisposition::RestartPlugin,
                        "correlate plugin response",
                        "plugin response identity or protocol version differs from its request",
                    ));
                }
                Ok(response)
            };
            tokio::select! {
                biased;
                () = cancellation.cancelled() => Err(cancelled()),
                () = stopped.cancelled() => Err(cancelled()),
                result = tokio::time::timeout(timeout, exchange) => result.unwrap_or_else(|_| Err(timeout_error())),
            }
        };
        if possibly_sent && result.is_err() {
            // A full pipe cannot accept a cancellation frame. Close the owned process tree
            // instead; never replay an invocation whose effects might already have happened.
            self.terminate().await?;
            return result.map_err(|error| {
                HostError::new(
                    error.class(),
                    RecoveryDisposition::RestartPlugin,
                    error.operation(),
                    format!(
                        "{}; process terminated; reconcile any external effects before retry",
                        error.detail()
                    ),
                )
            });
        }
        result
    }

    pub(crate) async fn terminate(&self) -> Result<(), HostError> {
        let mut child = self.child.lock().await;
        if let Err(error) = child.start_kill() {
            // ESRCH means the owned process group already exited.
            let exited = error.kind() == std::io::ErrorKind::NotFound
                || (cfg!(unix) && error.raw_os_error() == Some(3));
            if !exited {
                return Err(io_error("terminate plugin tree", error));
            }
        }
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .map_err(|_| {
                HostError::new(
                    HostFailureClass::Indeterminate,
                    RecoveryDisposition::Reconcile,
                    "reap plugin tree",
                    "owned process tree did not terminate within cleanup deadline",
                )
            })?
            .map_err(|e| io_error("reap plugin tree", e))?;
        Ok(())
    }

    async fn read(&self) -> Result<PluginResponseEnvelope, HostError> {
        let mut stdout = self.stdout.lock().await;
        let mut header = [0_u8; 4];
        stdout
            .read_exact(&mut header)
            .await
            .map_err(|error| io_error("read plugin response header", error))?;
        let length = u32::from_be_bytes(header);
        if length == 0 || length > self.frame_bytes {
            return Err(HostError::new(
                HostFailureClass::Protocol,
                RecoveryDisposition::RestartPlugin,
                "read plugin response",
                "plugin declared a zero or oversized frame",
            ));
        }
        let mut frame = Vec::with_capacity(4 + length as usize);
        frame.extend_from_slice(&header);
        frame.resize(4 + length as usize, 0);
        stdout
            .read_exact(&mut frame[4..])
            .await
            .map_err(|error| io_error("read plugin response body", error))?;
        drop(stdout);
        decode_frame(&frame, self.frame_bytes).map_err(|error| {
            HostError::with_source(
                HostFailureClass::Protocol,
                RecoveryDisposition::RestartPlugin,
                "decode plugin response",
                error.to_string(),
                error,
            )
        })
    }
}

pub fn internal_request_id(label: &str) -> Result<RequestId, HostError> {
    RequestId::new(label.to_owned()).map_err(|error| {
        HostError::with_source(
            HostFailureClass::Protocol,
            RecoveryDisposition::CorrectRequest,
            "construct host request identity",
            error.to_string(),
            error,
        )
    })
}

fn io_error(operation: &'static str, error: std::io::Error) -> HostError {
    HostError::with_source(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::RestartPlugin,
        operation,
        error.to_string(),
        error,
    )
}

fn transport_error(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::RestartPlugin,
        "launch isolated plugin",
        detail,
    )
}

fn cancelled() -> HostError {
    HostError::new(
        HostFailureClass::Cancelled,
        RecoveryDisposition::None,
        "invoke plugin",
        "plugin invocation was cancelled",
    )
}

fn timeout_error() -> HostError {
    HostError::new(
        HostFailureClass::Timeout,
        RecoveryDisposition::RestartPlugin,
        "invoke plugin",
        "plugin invocation exceeded its wall-time quota",
    )
}
