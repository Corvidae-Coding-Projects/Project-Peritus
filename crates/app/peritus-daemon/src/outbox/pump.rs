//! Owned bounded task that claims, routes, and settles durable outbox rows.

use std::time::Duration;

use tokio::{sync::watch, task::JoinHandle};

use super::{DestinationRouter, clock::OutboxClock};
use crate::{AuthorityHandle, DaemonError, DaemonErrorCode, DaemonRecovery};

const LEASE_SECONDS: u64 = 30;
const IDLE_MILLIS: u64 = 25;

/// Owned handle for the sole durable outbox claim loop.
pub(crate) struct OutboxRuntime {
    stop: watch::Sender<bool>,
    task: JoinHandle<Result<(), DaemonError>>,
}

impl OutboxRuntime {
    pub(crate) fn start(
        authority: AuthorityHandle,
        router: DestinationRouter,
        authority_epoch: u64,
    ) -> Result<Self, DaemonError> {
        let clock = OutboxClock::new(authority_epoch)?;
        let (stop, receiver) = watch::channel(false);
        let task = tokio::spawn(run(authority, router, clock, receiver));
        Ok(Self { stop, task })
    }

    pub(crate) async fn shutdown(&mut self, timeout: Duration) -> Result<(), DaemonError> {
        let _ = self.stop.send(true);
        match tokio::time::timeout(timeout, &mut self.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(DaemonError::with_source(
                DaemonErrorCode::UncleanShutdown,
                DaemonRecovery::Reconcile,
                "join outbox worker",
                "outbox worker panicked or was cancelled",
                error,
            )),
            Err(_) => {
                self.task.abort();
                Err(DaemonError::new(
                    DaemonErrorCode::UncleanShutdown,
                    DaemonRecovery::Reconcile,
                    "join outbox worker",
                    "outbox worker exceeded the configured shutdown bound",
                ))
            }
        }
    }
}

async fn run(
    authority: AuthorityHandle,
    router: DestinationRouter,
    clock: OutboxClock,
    mut stop: watch::Receiver<bool>,
) -> Result<(), DaemonError> {
    loop {
        if *stop.borrow() {
            return Ok(());
        }
        let (now, lease_until) = clock.lease(LEASE_SECONDS)?;
        let Some(message) = authority.claim_outbox(now, lease_until).await? else {
            tokio::select! {
                changed = stop.changed() => {
                    if changed.is_err() || *stop.borrow() { return Ok(()); }
                }
                () = tokio::time::sleep(Duration::from_millis(IDLE_MILLIS)) => {}
            }
            continue;
        };
        match router.deliver(&message).await {
            Ok(true) => {
                let fence = message.fence().ok_or_else(|| {
                    DaemonError::new(
                        DaemonErrorCode::CorruptState,
                        DaemonRecovery::Reconcile,
                        "settle outbox delivery",
                        "claimed outbox row has no claim fence",
                    )
                })?;
                authority.acknowledge_outbox(message.id(), fence).await?;
            }
            Ok(false) => {}
            Err(error) => {
                let terminal = message.attempts() >= message.max_attempts()
                    || matches!(
                        error.recovery(),
                        DaemonRecovery::CorrectRequest
                            | DaemonRecovery::ReadOnly
                            | DaemonRecovery::Operator
                    );
                if terminal {
                    authority
                        .enter_read_only(format!(
                            "{} while delivering durable outbox destination {}",
                            error.code(),
                            message.destination(),
                        ))
                        .await?;
                    return Err(error);
                }
            }
        }
    }
}
