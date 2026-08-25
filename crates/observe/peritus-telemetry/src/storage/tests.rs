//! Checkpoint publication fault-seam tests.

use std::{cell::Cell, num::NonZeroUsize};

use peritus_types::Sha256Digest;
use tempfile::TempDir;

use super::{CheckpointStore, ExportCheckpoint, storage_error};
use crate::{BufferCounters, ExportStreamId, TelemetryErrorKind};

#[test]
fn identical_retry_reenters_durability_finalization_after_prior_failure() {
    let temporary = TempDir::new().expect("temporary directory");
    let stream_id = ExportStreamId::new([71; 16]).expect("stream identity");
    let store = CheckpointStore::open(
        temporary.path(),
        stream_id,
        NonZeroUsize::new(1).expect("retention"),
    )
    .expect("checkpoint store");
    let checkpoint = ExportCheckpoint {
        stream_id,
        disposed_through_sequence: 0,
        prefix_digest: Sha256Digest::new([0; 32]),
        counters: BufferCounters::default(),
    };
    let finalizations = Cell::new(0_u64);

    let error = store
        .persist_with_finalize(checkpoint, |_| {
            finalizations.set(finalizations.get().checked_add(1).expect("finalization count"));
            Err(storage_error("injected directory synchronization failure"))
        })
        .expect_err("publication finalization fails after rename");
    assert_eq!(error.kind(), TelemetryErrorKind::Storage);
    assert_eq!(finalizations.get(), 1);

    store
        .persist_with_finalize(checkpoint, |_| {
            finalizations.set(finalizations.get().checked_add(1).expect("finalization count"));
            Ok(())
        })
        .expect("identical retry re-enters synchronization and pruning seam");
    assert_eq!(finalizations.get(), 2);
}

#[test]
fn checkpoint_rejects_a_prefix_without_exact_final_disposition_accounting() {
    let stream_id = ExportStreamId::new([72; 16]).expect("stream identity");
    let counters = BufferCounters::from_parts(2, 2, 0, 1).expect("generally bounded counters");
    let checkpoint = ExportCheckpoint {
        stream_id,
        disposed_through_sequence: 2,
        prefix_digest: Sha256Digest::new([1; 32]),
        counters,
    };

    assert_eq!(
        checkpoint.validate().expect_err("one record has no final disposition").kind(),
        TelemetryErrorKind::InvalidCheckpoint,
    );
}
