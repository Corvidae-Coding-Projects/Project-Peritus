//! Owned process/Wasm subprocess and bounded framed exchange.

use std::{
    future::Future as _, path::PathBuf, process::Stdio, sync::Arc, task::Poll, time::Duration,
};

use peritus_plugin_sdk::{
    HostRequest, PluginRequestEnvelope, PluginResponseEnvelope, RequestId, decode_frame,
    encode_frame,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

use crate::{HostCancellation, HostError, HostFailureClass, RecoveryDisposition};

#[derive(Clone, Debug)]
pub enum LaunchPlan {
    Process { executable: PathBuf, arguments: Vec<String>, working_directory: PathBuf },
    Wasm { runtime: PathBuf, module: PathBuf, arguments: Vec<String>, working_directory: PathBuf },
}

pub struct PluginConnection {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<ChildStdout>,
    transaction: Mutex<()>,
    frame_bytes: u32,
}

enum ResponseWait {
    Cancelled,
    TimedOut,
    Completed(Result<PluginResponseEnvelope, HostError>),
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
            .stdin
            .take()
            .ok_or_else(|| transport_error("launched plugin stdin is unavailable"))?;
        let stdout = child
            .stdout
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
    ) -> Result<PluginResponseEnvelope, HostError> {
        let _transaction = self.transaction.lock().await;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        self.write(&request).await?;
        let response = match self.wait_for_response(timeout, cancellation).await {
            ResponseWait::Cancelled => {
                let cancel = PluginRequestEnvelope {
                    protocol_version: request.protocol_version,
                    request_id: request.request_id.clone(),
                    request: HostRequest::Cancel {
                        request_id: request.request_id.clone(),
                        reason: "host cancellation".to_owned(),
                    },
                };
                let _ = self.write(&cancel).await;
                self.terminate().await;
                return Err(cancelled());
            }
            ResponseWait::TimedOut => {
                let cancel = PluginRequestEnvelope {
                    protocol_version: request.protocol_version,
                    request_id: request.request_id.clone(),
                    request: HostRequest::Cancel {
                        request_id: request.request_id.clone(),
                        reason: "host deadline".to_owned(),
                    },
                };
                let _ = self.write(&cancel).await;
                self.terminate().await;
                return Err(timeout_error());
            }
            ResponseWait::Completed(result) => result?,
        };
        if response.request_id != request.request_id
            || response.protocol_version != request.protocol_version
        {
            self.terminate().await;
            return Err(HostError::new(
                HostFailureClass::Protocol,
                RecoveryDisposition::RestartPlugin,
                "correlate plugin response",
                "plugin response identity or protocol version differs from its request",
            ));
        }
        Ok(response)
    }

    async fn wait_for_response(
        &self,
        timeout: Duration,
        cancellation: &HostCancellation,
    ) -> ResponseWait {
        let mut cancelled = Box::pin(cancellation.cancelled());
        let mut response = Box::pin(tokio::time::timeout(timeout, self.read()));
        std::future::poll_fn(|context| {
            if cancelled.as_mut().poll(context).is_ready() {
                return Poll::Ready(ResponseWait::Cancelled);
            }
            match response.as_mut().poll(context) {
                Poll::Ready(Ok(result)) => Poll::Ready(ResponseWait::Completed(result)),
                Poll::Ready(Err(_)) => Poll::Ready(ResponseWait::TimedOut),
                Poll::Pending => Poll::Pending,
            }
        })
        .await
    }

    pub(crate) async fn terminate(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = child.wait().await;
        drop(child);
    }

    async fn write(&self, request: &PluginRequestEnvelope) -> Result<(), HostError> {
        let frame = encode_frame(request, self.frame_bytes).map_err(|error| {
            HostError::with_source(
                HostFailureClass::Protocol,
                RecoveryDisposition::CorrectRequest,
                "encode plugin request",
                error.to_string(),
                error,
            )
        })?;
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&frame).await.map_err(|error| io_error("write plugin request", error))?;
        stdin.flush().await.map_err(|error| io_error("flush plugin request", error))
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
