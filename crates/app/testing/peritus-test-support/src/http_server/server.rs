//! Listener ownership, release control, and joined shutdown.

use super::model::{ExpectedHttpRequest, FakeHttpLimits, FakeHttpReleasePoint};
use super::observation::FakeHttpExchange;
use super::{FakeHttpError, FakeHttpErrorKind, ScriptedHttpResponse};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct Control {
    pub shutdown: bool,
    pub released: bool,
    pub blocked: Option<FakeHttpReleasePoint>,
    pub active: Option<TcpStream>,
    pub finished: bool,
}

#[derive(Debug, Default)]
pub struct Shared {
    pub control: Mutex<Control>,
    pub changed: Condvar,
}

/// One isolated loopback listener with an owned, joined worker.
#[derive(Debug)]
pub struct FakeHttpServer {
    address: SocketAddr,
    shared: Arc<Shared>,
    result: Receiver<Result<FakeHttpExchange, FakeHttpError>>,
    worker: Option<JoinHandle<()>>,
}

impl FakeHttpServer {
    /// Starts one listener on a fresh operating-system-assigned loopback port.
    ///
    /// # Errors
    ///
    /// Returns a typed bind or worker-spawn failure. The request and response constructors already
    /// enforce the supplied bounds.
    pub fn start(
        expected: ExpectedHttpRequest,
        response: ScriptedHttpResponse,
        limits: FakeHttpLimits,
    ) -> Result<Self, FakeHttpError> {
        expected.validate(limits)?;
        response.validate(limits)?;
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|_source| {
                FakeHttpError::new(
                    FakeHttpErrorKind::Bind,
                    "could not bind isolated loopback listener",
                )
            })?;
        let address = listener.local_addr().map_err(|_source| {
            FakeHttpError::new(FakeHttpErrorKind::Bind, "could not inspect loopback listener")
        })?;
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        let (sender, result) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("peritus-fake-http".to_owned())
            .spawn(move || {
                let outcome =
                    super::wire::serve(&listener, &expected, &response, limits, &worker_shared);
                if let Ok(mut control) = worker_shared.control.lock() {
                    control.active = None;
                    control.finished = true;
                    worker_shared.changed.notify_all();
                }
                let _ignored = sender.send(outcome);
            })
            .map_err(|_source| {
                FakeHttpError::new(FakeHttpErrorKind::Spawn, "could not spawn owned HTTP worker")
            })?;
        Ok(Self { address, shared, result, worker: Some(worker) })
    }

    /// Returns the isolated listener address.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns an `http` base URL for the isolated listener.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Waits without polling until the script reaches its release point.
    ///
    /// # Errors
    ///
    /// Returns a timeout, completed-worker, or synchronization failure.
    pub fn wait_until_blocked(
        &self,
        timeout: Duration,
    ) -> Result<FakeHttpReleasePoint, FakeHttpError> {
        let (control, timed) = self
            .shared
            .changed
            .wait_timeout_while(self.lock_control()?, timeout, |state| {
                state.blocked.is_none() && !state.finished
            })
            .map_err(|_poisoned| sync_error())?;
        let blocked = control.blocked;
        let finished = control.finished;
        let timed_out = timed.timed_out();
        drop(control);
        if let Some(point) = blocked {
            return Ok(point);
        }
        if timed_out {
            return Err(FakeHttpError::new(
                FakeHttpErrorKind::Timeout,
                "fake HTTP release-point wait expired",
            ));
        }
        debug_assert!(finished);
        Err(FakeHttpError::new(
            FakeHttpErrorKind::ReleaseState,
            "fake HTTP worker completed before reaching a release point",
        ))
    }

    /// Releases the worker from its configured pause.
    ///
    /// # Errors
    ///
    /// Returns a release-state error unless the worker is currently paused.
    pub fn release(&self) -> Result<(), FakeHttpError> {
        let mut control = self.lock_control()?;
        if control.blocked.is_none() || control.finished || control.released {
            return Err(FakeHttpError::new(
                FakeHttpErrorKind::ReleaseState,
                "fake HTTP worker is not waiting for release",
            ));
        }
        control.released = true;
        self.shared.changed.notify_all();
        drop(control);
        Ok(())
    }

    /// Joins a naturally completed exchange and returns its direct observation.
    ///
    /// # Errors
    ///
    /// Returns a worker, protocol, request-limit, or result-channel failure. Calling this before a
    /// client completes its request intentionally waits for that owned exchange.
    pub fn finish(mut self) -> Result<FakeHttpExchange, FakeHttpError> {
        self.join_worker()?;
        self.result.recv().map_err(|_disconnected| {
            FakeHttpError::new(FakeHttpErrorKind::MissingResult, "HTTP worker omitted its result")
        })?
    }

    fn lock_control(&self) -> Result<MutexGuard<'_, Control>, FakeHttpError> {
        self.shared.control.lock().map_err(|_poisoned| sync_error())
    }

    fn join_worker(&mut self) -> Result<(), FakeHttpError> {
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_panic| {
                FakeHttpError::new(FakeHttpErrorKind::WorkerPanic, "owned HTTP worker panicked")
            })?;
        }
        Ok(())
    }

    fn shutdown_and_join(&mut self) {
        if self.worker.is_none() {
            return;
        }
        if let Ok(mut control) = self.shared.control.lock() {
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

impl Drop for FakeHttpServer {
    fn drop(&mut self) {
        self.shutdown_and_join();
    }
}

const fn sync_error() -> FakeHttpError {
    FakeHttpError::new(FakeHttpErrorKind::Io, "fake HTTP worker synchronization failed")
}
