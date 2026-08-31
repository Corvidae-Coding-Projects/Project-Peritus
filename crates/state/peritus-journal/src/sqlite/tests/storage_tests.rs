use peritus_codec::{CodecLimits, encode_frame};
use peritus_types::{EventSequence, Sha256Digest};
use tempfile::TempDir;

use crate::{AggregateKind, AppendRequest, EventDraft, ExactFrame, HeadExpectation};

use super::{command, event, key, open, store_id};

#[test]
fn page_ceiling_returns_exact_storage_exhaustion_without_partial_append() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let baseline = journal.storage_pages().expect("storage pages");
    let limited =
        journal.limit_storage_pages(baseline.page_count()).expect("limit to current pages");
    assert_eq!(limited.maximum_pages(), baseline.page_count());
    assert_eq!(limited.maximum_bytes(), limited.maximum_pages() * limited.page_size());

    let aggregate = key(AggregateKind::Kernel, 11);
    let frame = ExactFrame::new(
        encode_frame(301, 1, &vec![7; 2 * 1024 * 1024], CodecLimits::PRODUCTION)
            .expect("large canonical frame"),
    )
    .expect("large exact frame");
    let event = EventDraft::new(
        aggregate,
        EventSequence::first(),
        event(11),
        None,
        frame,
        Sha256Digest::new([11; 32]),
        Vec::new(),
    )
    .expect("large event draft");
    let request = AppendRequest::new(
        store_id(),
        command(11),
        Sha256Digest::new([12; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("large append plan");
    let error = journal.append(request).expect_err("page ceiling rejects growth");
    assert!(error.is_storage_exhausted());
    assert!(journal.head(aggregate).expect("head remains readable").is_none());
    assert_eq!(journal.integrity_scan().expect("journal remains valid").event_count(), 0);
}
