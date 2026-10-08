//! Owned process/Wasm subprocess and bounded framed exchange.

use std::{
    future::Future, path::PathBuf, pin::Pin, process::Stdio, sync::Arc, time::Duration,
};

use peritus_plugin_sdk::{
    HostRequest, JsonWirePolicy, PluginQuotas, PluginRequestEnvelope, PluginResponseEnvelope,
    RequestId, decode_frame, encode_frame,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::Sleep,
};

use crate::{
    HostCancellation, HostError, HostFailureClass, RecoveryDisposition, quota::QuotaPermit,
};

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

enum PhaseWait<T> {
    Cancelled,
    TimedOut,
    Completed(T),
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
        self.exchange_inner(request, timeout, cancellation, None).await
    }

    pub(crate) async fn exchange_admitted(
        &self,
        request: PluginRequestEnvelope,
        timeout: Option<Duration>,
        cancellation: &HostCancellation,
        permit: &mut QuotaPermit<'_>,
    ) -> Result<PluginResponseEnvelope, HostError> {
        self.exchange_inner(request, timeout, cancellation, Some(permit)).await
    }

    async fn exchange_inner(
        &self,
        request: PluginRequestEnvelope,
        timeout: Option<Duration>,
        cancellation: &HostCancellation,
        mut permit: Option<&mut QuotaPermit<'_>>,
    ) -> Result<PluginResponseEnvelope, HostError> {
        let mut deadline = timeout.map(|timeout| Box::pin(tokio::time::sleep(timeout)));
        let _transaction = match await_phase(
            self.transaction.lock(),
            &mut deadline,
            cancellation,
        )
        .await
        {
            PhaseWait::Cancelled => return Err(cancelled_before_send()),
            PhaseWait::TimedOut => return Err(timeout_before_send()),
            PhaseWait::Completed(transaction) => transaction,
        };
        let frame = encode_frame(&request, self.request_policy).map_err(|error| {
            HostError::with_source(
                HostFailureClass::Protocol,
                RecoveryDisposition::CorrectRequest,
                "encode plugin request",
                error.to_string(),
                error,
            )
        })?;
        let mut stdin = match await_phase(self.stdin.lock(), &mut deadline, cancellation).await {
            PhaseWait::Cancelled => return Err(cancelled_before_send()),
            PhaseWait::TimedOut => return Err(timeout_before_send()),
            PhaseWait::Completed(stdin) => stdin,
        };
        let written = match await_phase(stdin.write(&frame), &mut deadline, cancellation).await {
            PhaseWait::Cancelled => {
                drop(stdin);
                return Err(cancelled_before_send());
            }
            PhaseWait::TimedOut => {
                drop(stdin);
                return Err(timeout_before_send());
            }
            PhaseWait::Completed(Ok(written)) => written,
            PhaseWait::Completed(Err(error)) => {
                drop(stdin);
                let failure = io_error("write plugin request", error);
                return Err(self.end_unsent_transport_failure(failure).await);
            }
        };
        if written == 0 {
            drop(stdin);
            let failure = io_error(
                "write plugin request",
                std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "plugin request transport accepted no bytes",
                ),
            );
            return Err(self.end_unsent_transport_failure(failure).await);
        }
        if let Some(permit) = permit.as_mut() {
            permit.admit();
        }
        match await_phase(stdin.write_all(&frame[written..]), &mut deadline, cancellation).await {
            PhaseWait::Cancelled => {
                drop(stdin);
                return Err(self
                    .end_aborted_exchange(&request, false, false)
                    .await);
            }
            PhaseWait::TimedOut => {
                drop(stdin);
                return Err(self
                    .end_aborted_exchange(&request, true, false)
                    .await);
            }
            PhaseWait::Completed(Ok(())) => {}
            PhaseWait::Completed(Err(error)) => {
                drop(stdin);
                let failure = io_error("write plugin request", error);
                return Err(self.end_indeterminate_failure(failure).await);
            }
        }
        match await_phase(stdin.flush(), &mut deadline, cancellation).await {
            PhaseWait::Cancelled => {
                drop(stdin);
                return Err(self
                    .end_aborted_exchange(&request, false, true)
                    .await);
            }
            PhaseWait::TimedOut => {
                drop(stdin);
                return Err(self
                    .end_aborted_exchange(&request, true, true)
                    .await);
            }
            PhaseWait::Completed(Ok(())) => {}
            PhaseWait::Completed(Err(error)) => {
                drop(stdin);
                let failure = io_error("flush plugin request", error);
                return Err(self.end_indeterminate_failure(failure).await);
            }
        }
        drop(stdin);
        let response = match await_phase(self.read(), &mut deadline, cancellation).await {
            PhaseWait::Cancelled => {
                return Err(self
                    .end_aborted_exchange(&request, false, true)
                    .await);
            }
            PhaseWait::TimedOut => {
                return Err(self
                    .end_aborted_exchange(&request, true, true)
                    .await);
            }
            PhaseWait::Completed(Ok(response)) => response,
            PhaseWait::Completed(Err(error)) => {
                return Err(self.end_indeterminate_failure(error).await);
            }
        };
        if response.request_id != request.request_id
            || response.protocol_version != request.protocol_version
        {
            let failure = HostError::new(
                HostFailureClass::Protocol,
                RecoveryDisposition::RestartPlugin,
                "correlate plugin response",
                "plugin response identity or protocol version differs from its request",
            );
            return Err(self.end_indeterminate_failure(failure).await);
        }
        Ok(response)
    }

    async fn end_aborted_exchange(
        &self,
        request: &PluginRequestEnvelope,
        timed_out: bool,
        notify: bool,
    ) -> HostError {
        let reason = if timed_out { "host deadline" } else { "host cancellation" };
        let termination = if notify {
            self.terminate_with_notice(request, reason).await
        } else {
            self.terminate().await
        };
        indeterminate_abort(timed_out, termination)
    }

    async fn end_unsent_transport_failure(&self, failure: HostError) -> HostError {
        match self.terminate().await {
            Ok(()) => failure,
            Err(termination) => termination,
        }
    }

    async fn end_indeterminate_failure(&self, failure: HostError) -> HostError {
        let termination = self.terminate().await;
        indeterminate_failure(failure, termination)
    }

    async fn terminate_with_notice(
        &self,
        request: &PluginRequestEnvelope,
        reason: &'static str,
    ) -> Result<(), HostError> {
        let cancel = PluginRequestEnvelope {
            protocol_version: request.protocol_version,
            request_id: request.request_id.clone(),
            request: HostRequest::Cancel {
                request_id: request.request_id.clone(),
                reason: reason.to_owned(),
            },
        };
        let mut termination = Box::pin(self.terminate());
        let mut notification = Box::pin(self.write(&cancel));
        tokio::select! {
            biased;
            _ = notification.as_mut() => {},
            result = termination.as_mut() => return result,
        }
        termination.await
    }

    pub(crate) async fn terminate(&self) -> Result<(), HostError> {
        let mut child = self.child.lock().await;
        if child
            .try_wait()
            .map_err(|error| termination_error("inspect owned plugin status", error))?
            .is_some()
        {
            return Ok(());
        }
        if let Err(kill_error) = child.start_kill() {
            return match child.try_wait() {
                Ok(Some(_)) => Ok(()),
                Ok(None) => Err(termination_error("signal owned plugin termination", kill_error)),
                Err(status_error) => Err(HostError::with_source(
                    HostFailureClass::Infrastructure,
                    RecoveryDisposition::Reconcile,
                    "terminate owned plugin",
                    format!(
                        "could not signal the owned plugin ({kill_error}) or confirm its status ({status_error})"
                    ),
                    status_error,
                )),
            };
        }
        child
            .wait()
            .await
            .map_err(|error| termination_error("reap owned plugin", error))?;
        Ok(())
    }

    async fn write(&self, request: &PluginRequestEnvelope) -> Result<(), HostError> {
        self.write_inner(request, None).await
    }

    async fn write_inner(
        &self,
        request: &PluginRequestEnvelope,
        mut permit: Option<&mut QuotaPermit<'_>>,
    ) -> Result<(), HostError> {
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
        let written = stdin
            .write(&frame)
            .await
            .map_err(|error| io_error("write plugin request", error))?;
        if written == 0 {
            return Err(io_error(
                "write plugin request",
                std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "plugin request transport accepted no bytes",
                ),
            ));
        }
        if let Some(permit) = permit.as_mut() {
            permit.admit();
        }
        stdin
            .write_all(&frame[written..])
            .await
            .map_err(|error| io_error("write plugin request", error))?;
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

async fn await_phase<F>(
    future: F,
    deadline: &mut Option<Pin<Box<Sleep>>>,
    cancellation: &HostCancellation,
) -> PhaseWait<F::Output>
where
    F: Future,
{
    let mut future = Box::pin(future);
    let mut cancelled = Box::pin(cancellation.cancelled());
    match deadline {
        Some(deadline) => {
            tokio::select! {
                biased;
                _ = cancelled.as_mut() => PhaseWait::Cancelled,
                _ = deadline.as_mut() => PhaseWait::TimedOut,
                output = future.as_mut() => PhaseWait::Completed(output),
            }
        }
        None => {
            tokio::select! {
                biased;
                _ = cancelled.as_mut() => PhaseWait::Cancelled,
                output = future.as_mut() => PhaseWait::Completed(output),
            }
        }
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

fn termination_error(detail: &'static str, error: std::io::Error) -> HostError {
    HostError::with_source(
        HostFailureClass::Infrastructure,
        RecoveryDisposition::Reconcile,
        "terminate owned plugin",
        detail,
        error,
    )
}

fn cancelled_before_send() -> HostError {
    HostError::new(
        HostFailureClass::Cancelled,
        RecoveryDisposition::None,
        "invoke plugin",
        "plugin invocation was cancelled before any request bytes were accepted",
    )
}

fn timeout_before_send() -> HostError {
    HostError::new(
        HostFailureClass::Timeout,
        RecoveryDisposition::RetryLater,
        "invoke plugin",
        "plugin invocation exceeded its wall-time quota before any request bytes were accepted",
    )
}

fn indeterminate_abort(timed_out: bool, termination: Result<(), HostError>) -> HostError {
    let trigger = if timed_out { "timed out" } else { "was cancelled" };
    match termination {
        Ok(()) => HostError::new(
            HostFailureClass::Indeterminate,
            RecoveryDisposition::Reconcile,
            "complete plugin invocation",
            format!(
                "plugin invocation {trigger} after request bytes were accepted; the owned plugin was terminated and reaped, but external effects remain unresolved"
            ),
        ),
        Err(error) => HostError::with_source(
            HostFailureClass::Indeterminate,
            RecoveryDisposition::Reconcile,
            "complete plugin invocation",
            format!(
                "plugin invocation {trigger} after request bytes were accepted; external effects and owned-process cleanup remain unresolved: {error}"
            ),
            error,
        ),
    }
}

fn indeterminate_failure(
    failure: HostError,
    termination: Result<(), HostError>,
) -> HostError {
    let detail = match &termination {
        Ok(()) => format!(
            "a request was accepted but no correlated result was established ({failure}); the owned plugin was terminated and reaped, but external effects remain unresolved"
        ),
        Err(error) => format!(
            "a request was accepted but no correlated result was established ({failure}); external effects and owned-process cleanup remain unresolved: {error}"
        ),
    };
    HostError::with_source(
        HostFailureClass::Indeterminate,
        RecoveryDisposition::Reconcile,
        "complete plugin invocation",
        detail,
        failure,
    )
}
