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

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Read},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use super::*;

    struct CancelAfterFirstByte {
        cancelled: Arc<AtomicBool>,
        sent: bool,
    }

    impl Read for CancelAfterFirstByte {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.sent || buffer.is_empty() {
                return Ok(0);
            }
            buffer[0] = b'x';
            self.sent = true;
            self.cancelled.store(true, Ordering::Release);
            Ok(1)
        }
    }

    fn cancelled_reader() -> CancellableReader<CancelAfterFirstByte> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = GateCancellation::new(cancelled.clone(), CancellationToken::default());
        cancellation.reader(CancelAfterFirstByte { cancelled, sent: false })
    }

    #[test]
    fn read_exact_stops_on_cancellation_instead_of_retrying_interrupted_reads() {
        let mut reader = cancelled_reader();
        let mut bytes = [0; 2];

        let error = reader.read_exact(&mut bytes).expect_err("cancellation must stop the read");

        assert_eq!(error.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn read_to_end_stops_on_cancellation_instead_of_retrying_interrupted_reads() {
        let mut reader = cancelled_reader();
        let mut bytes = Vec::new();

        let error = reader.read_to_end(&mut bytes).expect_err("cancellation must stop the read");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(bytes, b"x");
    }
}
