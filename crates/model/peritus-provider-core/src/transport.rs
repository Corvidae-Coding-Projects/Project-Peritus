//! Object-safe standard-library-future HTTP transport and body interfaces.

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use std::collections::VecDeque;

use crate::{
    CancellationToken, HttpLimits, HttpRequest, HttpResponse, ProviderCoreError,
    ProviderCoreErrorKind,
};

/// A sendable boxed future used by object-safe provider-core interfaces.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Pull-based owned byte stream.
pub trait ByteStream: Send {
    /// Pulls the next bounded chunk, or `None` at clean end-of-stream.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe transport, cancellation, or limit failure.
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, ProviderCoreError>>;
}

/// Object-safe HTTP transport consuming and returning only owned Peritus values.
pub trait HttpTransport: Send + Sync {
    /// Sends one request and returns response metadata plus a pull-based body.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe configuration, cancellation, limit, or transport failure.
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, ProviderCoreError>>;
}

/// Deterministic in-memory stream for tests and local adapters.
pub struct MemoryByteStream {
    chunks: VecDeque<Vec<u8>>,
}

impl fmt::Debug for MemoryByteStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let byte_count = self.chunks.iter().map(Vec::len).sum::<usize>();
        formatter
            .debug_struct("MemoryByteStream")
            .field("chunk_count", &self.chunks.len())
            .field("byte_count", &byte_count)
            .finish()
    }
}

impl MemoryByteStream {
    /// Creates a stream after validating every chunk and the cumulative byte count.
    ///
    /// # Errors
    ///
    /// Rejects chunks or cumulative bytes outside the HTTP limits.
    pub fn new(chunks: Vec<Vec<u8>>, limits: HttpLimits) -> Result<Self, ProviderCoreError> {
        let mut total = 0_usize;
        for chunk in &chunks {
            if chunk.len() > limits.max_chunk_bytes() {
                return Err(limit_error("memory body chunk exceeds its byte bound"));
            }
            total = total
                .checked_add(chunk.len())
                .ok_or_else(|| limit_error("memory body byte count overflowed"))?;
        }
        if total > limits.max_response_body_bytes() {
            return Err(limit_error("memory body exceeds its cumulative byte bound"));
        }
        Ok(Self { chunks: chunks.into() })
    }
}

impl ByteStream for MemoryByteStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, ProviderCoreError>> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ProviderCoreError::cancelled("read_body"));
            }
            Ok(self.chunks.pop_front())
        })
    }
}

pub struct BoundedByteStream {
    inner: Box<dyn ByteStream>,
    max_chunk_bytes: usize,
    max_response_body_bytes: usize,
    observed_bytes: usize,
    ended: bool,
}

impl BoundedByteStream {
    pub fn new(inner: Box<dyn ByteStream>, limits: HttpLimits) -> Self {
        Self {
            inner,
            max_chunk_bytes: limits.max_chunk_bytes(),
            max_response_body_bytes: limits.max_response_body_bytes(),
            observed_bytes: 0,
            ended: false,
        }
    }
}

impl fmt::Debug for BoundedByteStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedByteStream")
            .field("observed_bytes", &self.observed_bytes)
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

impl ByteStream for BoundedByteStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, ProviderCoreError>> {
        Box::pin(async move {
            if self.ended {
                return Ok(None);
            }
            if cancellation.is_cancelled() {
                return Err(ProviderCoreError::cancelled("read_body"));
            }
            let Some(chunk) = self.inner.next(cancellation).await? else {
                self.ended = true;
                return Ok(None);
            };
            if chunk.len() > self.max_chunk_bytes {
                self.ended = true;
                return Err(limit_error("response body chunk exceeds its byte bound"));
            }
            self.observed_bytes =
                self.observed_bytes.checked_add(chunk.len()).ok_or_else(|| {
                    self.ended = true;
                    limit_error("response body byte count overflowed")
                })?;
            if self.observed_bytes > self.max_response_body_bytes {
                self.ended = true;
                return Err(limit_error("response body exceeds its cumulative byte bound"));
            }
            Ok(Some(chunk))
        })
    }
}

const fn limit_error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(ProviderCoreErrorKind::LimitExceeded, "byte_stream", detail)
}
