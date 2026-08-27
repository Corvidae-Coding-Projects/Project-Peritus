//! Bounded authenticated local connection acceptor and owned task set.

use std::sync::Arc;

use peritus_app_protocol::ShutdownRequest;
use tokio::{
    sync::{Semaphore, mpsc, watch},
    task::JoinSet,
};

use super::LocalEndpoint;
use crate::{
    AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery, session::run_connection,
    terminal::TerminalRegistry,
};

pub(crate) async fn serve(
    endpoint: LocalEndpoint,
    authority: AuthorityHandle,
    terminals: TerminalRegistry,
    maximum_connections: usize,
    shutdown_request: mpsc::Sender<ShutdownRequest>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), DaemonError> {
    let permits = Arc::new(Semaphore::new(maximum_connections));
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() { break; }
            }
            Some(joined) = connections.join_next(), if !connections.is_empty() => {
                match joined {
                    Ok(Ok(()) | Err(_)) => {}
                    Err(error) => return Err(worker_error(error)),
                }
            }
            accepted = endpoint.accept(), if permits.available_permits() > 0 => {
                let permit = Arc::clone(&permits).acquire_owned().await.map_err(|_| stopped())?;
                match accepted {
                    Ok(connection) => {
                        let authority = authority.clone();
                        let terminals = terminals.clone();
                        let shutdown_request = shutdown_request.clone();
                        let connection_stop = stop.clone();
                        connections.spawn(async move {
                            let _permit = permit;
                            run_connection(
                                connection,
                                authority,
                                terminals,
                                shutdown_request,
                                connection_stop,
                            ).await
                        });
                    }
                    Err(error) if error.code_kind() == DaemonErrorCode::Unauthorized => {
                        drop(permit);
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }
    while let Some(joined) = connections.join_next().await {
        match joined {
            Ok(Ok(()) | Err(_)) => {}
            Err(error) => return Err(worker_error(error)),
        }
    }
    Ok(())
}

fn stopped() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::UncleanShutdown,
        DaemonRecovery::Reconcile,
        "serve local endpoint",
        "connection semaphore closed unexpectedly",
    )
}

fn worker_error(error: tokio::task::JoinError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Worker,
        DaemonRecovery::Reconcile,
        "join application connection",
        "application connection task panicked or was cancelled",
        error,
    )
}
