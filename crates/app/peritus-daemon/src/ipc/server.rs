//! Bounded authenticated local connection acceptor and owned task set.

use std::{
    future::{Future, poll_fn},
    io::Write as _,
    sync::Arc,
    task::Poll,
};

use peritus_app_protocol::ShutdownRequest;
use tokio::{
    sync::{Semaphore, mpsc, watch},
    task::JoinSet,
};

use super::{AuthenticatedConnection, LocalEndpoint};
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
        let action = {
            let mut changed = Box::pin(stop.changed());
            let mut accepted =
                (permits.available_permits() > 0).then(|| Box::pin(endpoint.accept()));
            poll_fn(|context| {
                if let Poll::Ready(changed) = changed.as_mut().poll(context) {
                    return Poll::Ready(ServerAction::Stop(changed));
                }
                if !connections.is_empty()
                    && let Poll::Ready(joined) = connections.poll_join_next(context)
                {
                    return Poll::Ready(ServerAction::Joined(joined));
                }
                if let Some(accepted) = &mut accepted
                    && let Poll::Ready(accepted) = accepted.as_mut().poll(context)
                {
                    return Poll::Ready(ServerAction::Accepted(accepted));
                }
                Poll::Pending
            })
            .await
        };
        match action {
            ServerAction::Stop(changed) => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
            ServerAction::Joined(Some(joined)) => match joined {
                Ok(Ok(()) | Err(_)) => {}
                Err(error) => return Err(worker_error(error)),
            },
            ServerAction::Joined(None) => {}
            ServerAction::Accepted(accepted) => {
                let permit = Arc::clone(&permits).acquire_owned().await.map_err(|_| stopped())?;
                match accepted {
                    Ok(connection) => {
                        let authority = authority.clone();
                        let terminals = terminals.clone();
                        let shutdown_request = shutdown_request.clone();
                        let connection_stop = stop.clone();
                        connections.spawn(async move {
                            let _permit = permit;
                            let result = run_connection(
                                connection,
                                authority,
                                terminals,
                                shutdown_request,
                                connection_stop,
                            )
                            .await;
                            if let Err(error) = &result {
                                let _ = write!(
                                    &mut std::io::stderr(),
                                    "application connection terminated: {} during {}: {}\n",
                                    error.code(),
                                    error.operation(),
                                    error.detail(),
                                );
                            }
                            result
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

enum ServerAction {
    Stop(Result<(), watch::error::RecvError>),
    Joined(Option<Result<Result<(), DaemonError>, tokio::task::JoinError>>),
    Accepted(Result<AuthenticatedConnection, DaemonError>),
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
