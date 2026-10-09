//! Exact persisted ordering, including admission without completed delivery.
use super::*;
use std::collections::BTreeMap;

fn id(value: u8) -> ControlOperationId {
    ControlOperationId::new([value; 16]).unwrap()
}

#[test]
fn capture_requires_delivered_input_before_pixels_and_a_later_check() {
    for (observed, completed, capture, check, expected) in [
        (true, 2, 3, 4, true),
        (false, 2, 3, 4, false),
        (true, 0, 3, 4, false),
        (true, 3, 2, 4, false),
        (true, 3, 3, 4, false),
        (true, 2, 3, 3, false),
        (true, 2, 3, 2, false),
        (true, 0, 0, 4, false),
    ] {
        let mut operations = BTreeMap::new();
        for (operation, result_sequence, completed_sequence) in
            [(id(1), 1, completed), (id(2), capture, 0), (id(3), check, 0)]
        {
            operations.insert(
                operation,
                super::super::super::super::PreviewOperationRecord {
                    fingerprint: peritus_codec::sha256(b"binding"),
                    accepted_revision: 7,
                    result_sequence,
                    completed_sequence,
                },
            );
        }
        let input =
            WorkbenchInteractionReceipt::new(id(1), peritus_codec::sha256(b"input"), observed);
        assert_eq!(ordered_capture(id(2), id(3), &[input], &operations), expected);
        assert!(!ordered_capture(id(2), id(3), &[], &operations));
        assert!(!ordered_capture(id(9), id(3), &[input], &operations));
        assert!(!ordered_capture(id(2), id(9), &[input], &operations));
    }
}
