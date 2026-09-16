//! Bounded authenticated local connection acceptor and owned task set.

use std::{
    future::{Future, poll_fn},
    io::Write as _,
    sync::Arc,
    task::Poll,
    time::Duration,
};

use tokio::{
    sync::{Semaphore, mpsc, watch},
    task::JoinSet,
    time::{Instant, sleep_until},
};

use super::{AuthenticatedConnection, LocalEndpoint};
use crate::{
    AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery,
    product_run::ProductRunService,
    session::{ShutdownCommand, run_connection},
    terminal::TerminalRegistry,
};

// Persistent listener/resource errors must yield rather than spin and flood the daemon log.
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);

#[cfg(all(test, unix))]
mod tests;

pub async fn serve(
    endpoint: LocalEndpoint,
    authority: AuthorityHandle,
    terminals: TerminalRegistry,
    product_runs: ProductRunService,
    maximum_connections: usize,
    shutdown_request: mpsc::Sender<ShutdownCommand>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), DaemonError> {
    let permits = Arc::new(Semaphore::new(maximum_connections));
    let mut connections = JoinSet::new();
    let mut retry_at = None;
    loop {
        let action = {
            let mut changed = Box::pin(stop.changed());
            let mut accepted = (permits.available_permits() > 0)
                .then(|| Box::pin(accept_after(&endpoint, retry_at)));
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
                        retry_at = None;
                        let authority = authority.clone();
                        let terminals = terminals.clone();
                        let product_runs = product_runs.clone();
                        let shutdown_request = shutdown_request.clone();
                        let connection_stop = stop.clone();
                        connections.spawn(async move {
                            let _permit = permit;
                            let result = run_connection(
                                connection,
                                authority,
                                terminals,
                                product_runs,
                                shutdown_request,
                                connection_stop,
                            )
                            .await;
                            if let Err(error) = &result {
                                report_connection_event(
                                    b"application connection terminated: ",
                                    error,
                                );
                            }
                            result
                        });
                    }
                    Err(error) if error.code_kind() == DaemonErrorCode::Unauthorized => {
                        drop(permit);
                        retry_at = None;
                    }
                    Err(error) if error.recovery() == DaemonRecovery::Retry => {
                        drop(permit);
                        report_rejected_connection(&error);
                        retry_at = Some(Instant::now() + ACCEPT_RETRY_DELAY);
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

async fn accept_after(
    endpoint: &LocalEndpoint,
    retry_at: Option<Instant>,
) -> Result<AuthenticatedConnection, DaemonError> {
    if let Some(deadline) = retry_at {
        sleep_until(deadline).await;
    }
    endpoint.accept().await
}

enum ServerAction {
    Stop(Result<(), watch::error::RecvError>),
    Joined(Option<Result<Result<(), DaemonError>, tokio::task::JoinError>>),
    Accepted(Result<AuthenticatedConnection, DaemonError>),
}

fn report_rejected_connection(error: &DaemonError) {
    report_connection_event(b"local connection rejected: ", error);
}

fn report_connection_event(prefix: &[u8], error: &DaemonError) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(prefix);
    let _ = stderr.write_all(error.code().as_bytes());
    let _ = stderr.write_all(b" during ");
    let _ = stderr.write_all(error.operation().as_bytes());
    let _ = stderr.write_all(b": ");
    let _ = stderr.write_all(error.detail().as_bytes());
    let _ = stderr.write_all(b"\n");
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
