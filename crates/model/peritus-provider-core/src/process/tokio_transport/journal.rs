//! Ordered persistence with physical backlog backpressure, independent of child pipe reads.

use std::task::{Context, Poll};
use tokio::{io::AsyncWriteExt as _, sync::mpsc};

use crate::{BoxFuture, ProviderCoreError};

// Eight pipe buffers bound transient journal memory, not accepted output or execution duration.
// A full backlog waits for storage progress; it never rejects a turn or discards a byte.
const BACKLOG_CHUNKS: usize = 8;
pub(super) type JournalSender = mpsc::Sender<Vec<u8>>;

pub(super) struct JournalWriter {
    operation: BoxFuture<'static, Result<(), ProviderCoreError>>,
    result: Option<Result<(), ProviderCoreError>>,
}

impl JournalWriter {
    pub(super) fn new(file: Option<tokio::fs::File>) -> (Option<JournalSender>, Self) {
        file.map_or_else(
            || (None, Self { operation: Box::pin(async { Ok(()) }), result: None }),
            |file| {
                let (sender, writer) = Self::with_sink(file);
                (Some(sender), writer)
            },
        )
    }

    fn with_sink(sink: impl JournalSink + Send + 'static) -> (JournalSender, Self) {
        let (sender, receiver) = mpsc::channel(BACKLOG_CHUNKS);
        let operation = Box::pin(persist(receiver, sink));
        (sender, Self { operation, result: None })
    }

    pub(super) fn poll(
        &mut self,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), ProviderCoreError>> {
        if let Some(result) = &self.result {
            return Poll::Ready(result.clone());
        }
        match self.operation.as_mut().poll(context) {
            Poll::Ready(result) => {
                self.result = Some(result.clone());
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    pub(super) async fn finish(mut self) -> Result<(), ProviderCoreError> {
        std::future::poll_fn(|context| self.poll(context)).await
    }
}

trait JournalSink {
    fn persist<'a>(&'a mut self, bytes: &'a [u8]) -> BoxFuture<'a, Result<(), ProviderCoreError>>;
}

impl JournalSink for tokio::fs::File {
    fn persist<'a>(&'a mut self, bytes: &'a [u8]) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async move {
            self.write_all(bytes).await.map_err(|_| {
                ProviderCoreError::transport(
                    "process_journal",
                    "owned subprocess journal write failed",
                )
            })?;
            self.sync_data().await.map_err(|_| {
                ProviderCoreError::transport(
                    "process_journal",
                    "owned subprocess journal persistence failed",
                )
            })
        })
    }
}

async fn persist(
    mut receiver: mpsc::Receiver<Vec<u8>>,
    mut sink: impl JournalSink,
) -> Result<(), ProviderCoreError> {
    while let Some(mut bytes) = receiver.recv().await {
        // Drain what is already available. A quiet thread-start prefix is persisted immediately;
        // busy output shares a durability barrier without waiting for a timer, size, or EOF.
        while let Ok(next) = receiver.try_recv() {
            bytes.try_reserve(next.len()).map_err(|_| allocation_error())?;
            bytes.extend_from_slice(&next);
        }
        sink.persist(&bytes).await?;
    }
    Ok(())
}

pub(super) const fn allocation_error() -> ProviderCoreError {
    ProviderCoreError::transport("process_journal", "owned subprocess journal allocation failed")
}

#[cfg(test)]
mod tests;
