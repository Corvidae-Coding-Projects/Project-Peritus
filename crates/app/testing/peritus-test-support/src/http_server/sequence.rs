//! Owned bounded loopback server for retry and multi-attempt HTTP sequences.

use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, MutexGuard};
use std::thread::{self, JoinHandle};

use super::model::{FakeHttpExchangeScript, FakeHttpLimits};
use super::observation::FakeHttpExchange;
use super::server::{Control, Shared};
use super::{FakeHttpError, FakeHttpErrorKind};

/// One fresh loopback listener that serves an exact ordered sequence and joins its worker.
#[derive(Debug)]
pub struct FakeHttpSequenceServer {
    address: SocketAddr,
    shared: Arc<Shared>,
    result: Receiver<Result<Vec<FakeHttpExchange>, FakeHttpError>>,
    worker: Option<JoinHandle<()>>,
}

impl FakeHttpSequenceServer {
    /// Starts an exact nonempty sequence on one stable loopback endpoint.
    ///
    /// # Errors
    ///
    /// Returns a typed configuration, bind, or worker-spawn failure. The sequence count is bounded
    /// by the configured chunk count so an adapter retry cannot create an unbounded accept loop.
    pub fn start(
        scripts: Vec<FakeHttpExchangeScript>,
        limits: FakeHttpLimits,
    ) -> Result<Self, FakeHttpError> {
        if scripts.is_empty() || scripts.len() > limits.max_chunks() {
            return Err(FakeHttpError::new(
                FakeHttpErrorKind::InvalidConfiguration,
                "fake HTTP exchange sequence is empty or exceeds its bound",
            ));
        }
        for script in &scripts {
            script.validate(limits)?;
        }
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|_source| {
                FakeHttpError::new(
                    FakeHttpErrorKind::Bind,
                    "could not bind isolated sequence listener",
                )
            })?;
        let address = listener.local_addr().map_err(|_source| {
            FakeHttpError::new(
                FakeHttpErrorKind::Bind,
                "could not inspect isolated sequence listener",
            )
        })?;
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        let (sender, result) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("peritus-fake-http-sequence".to_owned())
            .spawn(move || {
                let outcome = serve_sequence(&listener, scripts, limits, &worker_shared);
                if let Ok(mut control) = worker_shared.control.lock() {
                    control.active = None;
                    control.blocked = None;
                    control.finished = true;
                    worker_shared.changed.notify_all();
                }
                let _ignored = sender.send(outcome);
            })
            .map_err(|_source| {
                FakeHttpError::new(
                    FakeHttpErrorKind::Spawn,
                    "could not spawn owned HTTP sequence worker",
                )
            })?;
        Ok(Self { address, shared, result, worker: Some(worker) })
    }

    /// Returns the stable listener address used by every sequence step.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns an `http` base URL for the stable listener.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Joins the naturally completed sequence and returns every direct exchange observation.
    ///
    /// # Errors
    ///
    /// Returns a worker, protocol, request-limit, or result-channel failure.
    pub fn finish(mut self) -> Result<Vec<FakeHttpExchange>, FakeHttpError> {
        self.join_worker()?;
        self.result.recv().map_err(|_disconnected| {
            FakeHttpError::new(
                FakeHttpErrorKind::MissingResult,
                "HTTP sequence worker omitted its result",
            )
        })?
    }

    fn lock_control(&self) -> Result<MutexGuard<'_, Control>, FakeHttpError> {
        self.shared.control.lock().map_err(|_poisoned| {
            FakeHttpError::new(FakeHttpErrorKind::Io, "fake HTTP sequence synchronization failed")
        })
    }

    fn join_worker(&mut self) -> Result<(), FakeHttpError> {
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_panic| {
                FakeHttpError::new(
                    FakeHttpErrorKind::WorkerPanic,
                    "owned HTTP sequence worker panicked",
                )
            })?;
        }
        Ok(())
    }

    fn shutdown_and_join(&mut self) {
        if self.worker.is_none() {
            return;
        }
        if let Ok(mut control) = self.lock_control() {
            control.shutdown = true;
            control.released = true;
            if let Some(stream) = control.active.take() {
                let _shutdown = stream.shutdown(Shutdown::Both);
            }
            self.shared.changed.notify_all();
        }
        let _wake_accept = TcpStream::connect(self.address);
        let _joined = self.join_worker();
    }
}

impl Drop for FakeHttpSequenceServer {
    fn drop(&mut self) {
        self.shutdown_and_join();
    }
}

fn serve_sequence(
    listener: &TcpListener,
    scripts: Vec<FakeHttpExchangeScript>,
    limits: FakeHttpLimits,
    shared: &Arc<Shared>,
) -> Result<Vec<FakeHttpExchange>, FakeHttpError> {
    let mut exchanges = Vec::with_capacity(scripts.len());
    for script in scripts {
        {
            let mut control = shared.control.lock().map_err(|_poisoned| {
                FakeHttpError::new(
                    FakeHttpErrorKind::Io,
                    "fake HTTP sequence synchronization failed",
                )
            })?;
            if control.shutdown {
                return Err(FakeHttpError::new(
                    FakeHttpErrorKind::Io,
                    "fake HTTP sequence worker was shut down",
                ));
            }
            control.active = None;
            control.blocked = None;
            control.released = false;
        }
        exchanges.push(super::wire::serve(
            listener,
            &script.expected,
            &script.response,
            limits,
            shared,
        )?);
    }
    Ok(exchanges)
}
