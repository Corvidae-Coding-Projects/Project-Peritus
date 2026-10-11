//! Cooperative cancellation for streamed host-owned gate reads.

use std::{
    io::{self, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use peritus_provider_core::CancellationToken;

#[derive(Clone, Default)]
pub struct GateCancellation {
    cancelled: Arc<AtomicBool>,
    provider: CancellationToken,
}

impl GateCancellation {
    pub(crate) const fn new(cancelled: Arc<AtomicBool>, provider: CancellationToken) -> Self {
        Self { cancelled, provider }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.provider.is_cancelled()
    }

    pub(crate) fn reader<R>(&self, inner: R) -> CancellableReader<R> {
        CancellableReader { inner, cancellation: self.clone() }
    }
}

pub struct CancellableReader<R> {
    inner: R,
    cancellation: GateCancellation,
}

impl<R: Read> Read for CancellableReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::other("run was cancelled"));
        }
        self.inner.read(buffer)
    }
}
