//! Live daemon ownership and signal-facing runtime API.

mod progress;
mod runner;
mod teardown;

use std::{
    future::{Future, poll_fn},
    task::Poll,
};

use peritus_app_protocol::ShutdownRequest;
use peritus_evidence::EvidenceStore;
use peritus_process::ProcessStore;
use peritus_projection::ProjectionStore;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

use super::{evolution::ProductionCatalog, workspace::WorkspaceCatalog};
use crate::instance::InstanceGuard;
use crate::outbox::OutboxRuntime;
use crate::product_run::ProductRunService;
use crate::telemetry::TelemetryRuntime;
use crate::terminal::TerminalRegistry;
use crate::worker::WorkerSupervisor;
use crate::{
    AuthorityHandle, DaemonComponents, DaemonConfig, DaemonError, DaemonErrorCode, DaemonIdentity,
    DaemonRecovery, LocalEndpointAddress,
};

/// Live production daemon ownership bundle.
pub struct DaemonRuntime {
    config: DaemonConfig,
    identity: DaemonIdentity,
    authority: AuthorityHandle,
    authority_task: JoinHandle<Result<(), DaemonError>>,
    endpoint_address: LocalEndpointAddress,
    server_stop: watch::Sender<bool>,
    server_task: Option<JoinHandle<Result<(), DaemonError>>>,
    shutdown_requests: mpsc::Receiver<ShutdownRequest>,
    accepted_shutdown: Option<ShutdownRequest>,
    outbox: Option<OutboxRuntime>,
    telemetry: Option<TelemetryRuntime>,
    workers: WorkerSupervisor,
    terminals: TerminalRegistry,
    product_runs: ProductRunService,
    _components: DaemonComponents,
    _evidence: EvidenceStore,
    processes: ProcessStore,
    _projections: ProjectionStore,
    _production: ProductionCatalog,
    _workspaces: WorkspaceCatalog,
    _instance: InstanceGuard,
}

impl DaemonRuntime {
    /// Returns stable daemon/store identity.
    #[must_use]
    pub const fn identity(&self) -> &DaemonIdentity {
        &self.identity
    }

    /// Returns the bounded serialized authority client.
    #[must_use]
    pub const fn authority(&self) -> &AuthorityHandle {
        &self.authority
    }

    /// Returns the stable authenticated local endpoint address.
    #[must_use]
    pub const fn endpoint_address(&self) -> &LocalEndpointAddress {
        &self.endpoint_address
    }

    /// Returns the exact accepted A3 shutdown request, when shutdown originated from a client.
    #[must_use]
    pub const fn accepted_shutdown_request(&self) -> Option<ShutdownRequest> {
        self.accepted_shutdown
    }

    /// Waits for an operating-system or authenticated A3 shutdown request.
    ///
    /// # Errors
    ///
    /// Returns a transport error if the signal source or endpoint server fails.
    pub async fn wait_for_shutdown_signal(&mut self) -> Result<(), DaemonError> {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .map_err(|error| signal_error("register termination signal", error))?;
            let action = {
                let mut interrupt = Box::pin(tokio::signal::ctrl_c());
                let mut termination = Box::pin(terminate.recv());
                let mut request = Box::pin(self.shutdown_requests.recv());
                let server = self.server_task.as_mut().expect("live runtime owns server task");
                poll_fn(|context| {
                    if let Poll::Ready(result) = interrupt.as_mut().poll(context) {
                        return Poll::Ready(ShutdownAction::Interrupt(result));
                    }
                    if termination.as_mut().poll(context).is_ready() {
                        return Poll::Ready(ShutdownAction::Terminate);
                    }
                    if let Poll::Ready(request) = request.as_mut().poll(context) {
                        return Poll::Ready(ShutdownAction::Request(request));
                    }
                    if let Poll::Ready(result) =
                        Future::poll(std::pin::Pin::new(&mut *server), context)
                    {
                        return Poll::Ready(ShutdownAction::Server(result));
                    }
                    Poll::Pending
                })
                .await
            };
            match action {
                ShutdownAction::Interrupt(result) => {
                    result.map_err(|error| signal_error("wait for interrupt signal", error))
                }
                ShutdownAction::Terminate => Ok(()),
                ShutdownAction::Request(request) => {
                    self.accepted_shutdown = request;
                    Ok(())
                }
                ShutdownAction::Server(result) => {
                    self.server_task = None;
                    server_exit(result)
                }
            }
        }
        #[cfg(windows)]
        {
            let action = {
                let mut interrupt = Box::pin(tokio::signal::ctrl_c());
                let mut request = Box::pin(self.shutdown_requests.recv());
                let server = self.server_task.as_mut().expect("live runtime owns server task");
                poll_fn(|context| {
                    if let Poll::Ready(result) = interrupt.as_mut().poll(context) {
                        return Poll::Ready(ShutdownAction::Interrupt(result));
                    }
                    if let Poll::Ready(request) = request.as_mut().poll(context) {
                        return Poll::Ready(ShutdownAction::Request(request));
                    }
                    if let Poll::Ready(result) =
                        Future::poll(std::pin::Pin::new(&mut *server), context)
                    {
                        return Poll::Ready(ShutdownAction::Server(result));
                    }
                    Poll::Pending
                })
                .await
            };
            match action {
                ShutdownAction::Interrupt(result) => {
                    result.map_err(|error| signal_error("wait for interrupt signal", error))
                }
                ShutdownAction::Terminate => Ok(()),
                ShutdownAction::Request(request) => {
                    self.accepted_shutdown = request;
                    Ok(())
                }
                ShutdownAction::Server(result) => {
                    self.server_task = None;
                    server_exit(result)
                }
            }
        }
    }
}

enum ShutdownAction {
    Interrupt(Result<(), std::io::Error>),
    Terminate,
    Request(Option<ShutdownRequest>),
    Server(Result<Result<(), DaemonError>, tokio::task::JoinError>),
}

fn signal_error(operation: &'static str, error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Transport,
        DaemonRecovery::Operator,
        operation,
        "daemon signal source failed",
        error,
    )
}

fn server_exit(
    result: Result<Result<(), DaemonError>, tokio::task::JoinError>,
) -> Result<(), DaemonError> {
    match result {
        Ok(result) => result,
        Err(error) => Err(DaemonError::with_source(
            DaemonErrorCode::Worker,
            DaemonRecovery::Reconcile,
            "join local endpoint server",
            "local endpoint server panicked or was cancelled",
            error,
        )),
    }
}
