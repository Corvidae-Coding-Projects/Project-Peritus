//! Owned process/Wasm subprocess and bounded framed exchange.

use std::{
    future::Future as _, path::PathBuf, process::Stdio, sync::Arc, task::Poll, time::Duration,
};

use peritus_plugin_sdk::{
    HostRequest, JsonWirePolicy, PluginQuotas, PluginRequestEnvelope, PluginResponseEnvelope,
    RequestId, decode_frame, encode_frame,
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
    request_policy: JsonWirePolicy,
    response_policy: JsonWirePolicy,
}

enum ResponseWait {
    Cancelled,
    TimedOut,
    Completed(Result<PluginResponseEnvelope, HostError>),
}

impl PluginConnection {
    pub(crate) fn spawn(
        plan: LaunchPlan,
        quotas: PluginQuotas,
        protocol_version: u16,
    ) -> Result<Arc<Self>, HostError> {
        let request_policy = quotas
            .request_wire_policy()
        .map_err(|error| wire_policy_error(error.to_string()))?;
        let response_policy = quotas
            .response_wire_policy()
        .map_err(|error| wire_policy_error(error.to_string()))?;
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
            .env("PERITUS_PLUGIN_PROTOCOL", protocol_version.to_string())
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
            request_policy,
            response_policy,
        }))
    }

    pub(crate) async fn exchange(
        &self,
        request: PluginRequestEnvelope,
        timeout: Option<Duration>,
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
        timeout: Option<Duration>,
        cancellation: &HostCancellation,
    ) -> ResponseWait {
        let mut cancelled = Box::pin(cancellation.cancelled());
        let mut response = Box::pin(async {
            match timeout {
                Some(timeout) => match tokio::time::timeout(timeout, self.read()).await {
                    Ok(result) => ResponseWait::Completed(result),
                    Err(_) => ResponseWait::TimedOut,
                },
                None => ResponseWait::Completed(self.read().await),
            }
        });
        std::future::poll_fn(|context| {
            if cancelled.as_mut().poll(context).is_ready() {
                return Poll::Ready(ResponseWait::Cancelled);
            }
            match response.as_mut().poll(context) {
                Poll::Ready(result) => Poll::Ready(result),
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
        let frame = encode_frame(request, self.request_policy).map_err(|error| {
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
        if length == 0 || length > self.response_policy.frame_bytes() {
            return Err(HostError::new(
                HostFailureClass::Protocol,
                RecoveryDisposition::RestartPlugin,
                "read plugin response",
                "plugin declared a zero or oversized frame",
            ));
        }
        let frame_length = 4_usize
            .checked_add(length as usize)
            .ok_or_else(|| transport_error("plugin frame allocation length overflowed"))?;
        let mut frame = Vec::with_capacity(frame_length);
        frame.extend_from_slice(&header);
        frame.resize(frame_length, 0);
        stdout
            .read_exact(&mut frame[4..])
            .await
            .map_err(|error| io_error("read plugin response body", error))?;
        drop(stdout);
        decode_frame(&frame, self.response_policy).map_err(|error| {
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

fn wire_policy_error(detail: String) -> HostError {
    HostError::new(
        HostFailureClass::Protocol,
        RecoveryDisposition::CorrectRequest,
        "configure plugin wire policy",
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
