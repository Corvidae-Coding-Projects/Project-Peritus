//! Deterministic storage pressure, ordering, and failure contracts.

use super::*;
use crate::process::tokio_transport::output::read_bounded;
use core::future::Future as _;
use std::{
    future::poll_fn,
    sync::{Arc, Mutex},
};
use tokio::sync::oneshot;

type Batches = Arc<Mutex<Vec<Vec<u8>>>>;

struct GatedSink {
    started: Option<oneshot::Sender<()>>,
    release: Option<oneshot::Receiver<()>>,
    batches: Batches,
}

impl JournalSink for GatedSink {
    fn persist<'a>(&'a mut self, bytes: &'a [u8]) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async move {
            if let Some(started) = self.started.take() {
                started.send(()).expect("barrier observer");
            }
            if let Some(release) = self.release.take() {
                release.await.expect("release barrier");
            }
            self.batches.lock().expect("batches").push(bytes.to_vec());
            Ok(())
        })
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime")
}

#[test]
fn quiet_prefix_starts_persistence_and_pipe_drain_continues_during_its_barrier() {
    runtime().block_on(async {
        let (started_tx, mut started_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let batches = Batches::default();
        let (sender, mut writer) = JournalWriter::with_sink(GatedSink {
            started: Some(started_tx),
            release: Some(release_rx),
            batches: batches.clone(),
        });
        sender.send(b"thread.started\n".to_vec()).await.expect("prefix");
        poll_fn(|context| {
            assert!(writer.poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        assert_eq!(started_rx.try_recv(), Ok(()), "quiet prefix needs no size/timer/EOF trigger");

        let bytes = vec![b'x'; 3 * 8 * 1024];
        let mut reader =
            Box::pin(read_bounded(bytes.as_slice(), bytes.len(), "stdout", Some(sender)));
        // The storage barrier is explicitly held closed. Progress is observed without a sleep
        // or wall-clock assertion; the old per-read barrier cannot complete this pipe drain.
        poll_fn(|context| {
            assert_eq!(reader.as_mut().poll(context), Poll::Ready(Ok(bytes.clone())));
            assert!(writer.poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(reader);
        assert!(batches.lock().expect("batches").is_empty());
        release_tx.send(()).expect("release persistence");
        writer.finish().await.expect("settled journal");
        let batches = batches.lock().expect("batches");
        assert_eq!(batches.len(), 2, "queued output shares a barrier");
        assert_eq!(batches[0], b"thread.started\n");
        assert_eq!(batches[1], bytes);
        drop(batches);
    });
}

#[test]
fn full_physical_backlog_waits_then_retains_every_byte_in_order() {
    runtime().block_on(async {
        let (release_tx, release_rx) = oneshot::channel();
        let batches = Batches::default();
        let (sender, mut writer) = JournalWriter::with_sink(GatedSink {
            started: None,
            release: Some(release_rx),
            batches: batches.clone(),
        });
        sender.send(b"prefix".to_vec()).await.expect("prefix");
        poll_fn(|context| {
            assert!(writer.poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        let bytes: Vec<_> = (0_u8..=15).flat_map(|byte| vec![byte; 8 * 1024]).collect();
        let mut reader =
            Box::pin(read_bounded(bytes.as_slice(), bytes.len(), "stdout", Some(sender)));
        poll_fn(|context| {
            assert!(reader.as_mut().poll(context).is_pending(), "backlog applies backpressure");
            assert!(writer.poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        release_tx.send(()).expect("release persistence");
        let mut received = None;
        poll_fn(|context| {
            if received.is_none()
                && let Poll::Ready(output) = reader.as_mut().poll(context)
            {
                received = Some(output.expect("output"));
            }
            match writer.poll(context) {
                Poll::Ready(result) if received.is_some() => Poll::Ready(result),
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) | Poll::Pending => Poll::Pending,
            }
        })
        .await
        .expect("persistence");
        assert_eq!(received.expect("drained"), bytes);
        writer.finish().await.expect("already finished is safe");
        let retained = batches.lock().expect("batches").concat();
        assert_eq!(retained, [b"prefix".as_slice(), &bytes].concat());
    });
}

#[test]
fn dropping_reader_settles_received_bytes_without_waiting_for_eof() {
    runtime().block_on(async {
        let (release_tx, release_rx) = oneshot::channel();
        let batches = Batches::default();
        let (sender, mut writer) = JournalWriter::with_sink(GatedSink {
            started: None,
            release: Some(release_rx),
            batches: batches.clone(),
        });
        let (mut producer, consumer) = tokio::io::duplex(8 * 1024);
        producer.write_all(b"recoverable-thread\n").await.expect("prefix");
        let mut reader = Box::pin(read_bounded(consumer, 8 * 1024, "stdout", Some(sender)));
        poll_fn(|context| {
            assert!(reader.as_mut().poll(context).is_pending(), "pipe remains open");
            assert!(writer.poll(context).is_pending(), "writer awaits more pipe bytes");
            Poll::Ready(())
        })
        .await;
        assert!(batches.lock().expect("batches").is_empty(), "persistence is still in flight");
        drop(reader);
        release_tx.send(()).expect("release retained write after cancellation");
        writer.finish().await.expect("cancelled reader's journal");
        assert_eq!(batches.lock().expect("batches").concat(), b"recoverable-thread\n");
    });
}

struct FailingSink;
impl JournalSink for FailingSink {
    fn persist<'a>(&'a mut self, _bytes: &'a [u8]) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
        Box::pin(async {
            Err(ProviderCoreError::transport("process_journal", "injected persistence failure"))
        })
    }
}

#[test]
fn persistence_failure_is_observed_even_after_the_reader_is_cancelled() {
    runtime().block_on(async {
        let (sender, writer) = JournalWriter::with_sink(FailingSink);
        sender.send(b"received".to_vec()).await.expect("receive");
        drop(sender);
        let error = writer.finish().await.expect_err("persistence failure");
        assert_eq!(error.operation(), "process_journal");
        assert_eq!(error.detail(), "injected persistence failure");
    });
}
