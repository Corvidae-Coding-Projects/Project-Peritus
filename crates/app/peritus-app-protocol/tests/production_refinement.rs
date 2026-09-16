//! Boundary checks for the production functions now selected by Verus.

use peritus_app_protocol::{
    acknowledgement_is_legal, chunk_is_contiguous, completion_is_conserved, cursor_advances,
    delivery_window_is_safe, output_is_contiguous, output_position_is_valid,
};

#[test]
fn production_chunk_and_terminal_arithmetic_matches_wide_integer_accounting() {
    let offsets = [0, 1, u64::MAX - 1, u64::MAX];
    let lengths = [0, 1, 2, usize::MAX];
    for conserved in offsets {
        for offset in offsets {
            for bytes in lengths {
                let after = u128::from(conserved).checked_add(bytes as u128);
                for declared in offsets {
                    let expected = offset == conserved
                        && after.is_some_and(|after| after <= u128::from(declared));
                    assert_eq!(
                        chunk_is_contiguous(conserved, offset, bytes, declared),
                        expected,
                        "artifact {conserved}/{offset}/{bytes}/{declared}"
                    );
                    assert_eq!(completion_is_conserved(conserved, declared), conserved == declared);
                }
                let expected = offset == conserved
                    && bytes != 0
                    && after.is_some_and(|after| after <= u128::from(u64::MAX));
                assert_eq!(output_is_contiguous(7, conserved, 7, offset, bytes), expected);
                assert!(!output_is_contiguous(7, conserved, 8, offset, bytes));
            }
        }
    }
    // The arithmetic predicate permits an empty chunk; ArtifactChunk enforces nonempty input.
    assert!(chunk_is_contiguous(u64::MAX, u64::MAX, 0, u64::MAX));
    assert!(!output_is_contiguous(7, u64::MAX, 7, u64::MAX, 0));
    assert!(output_position_is_valid(u64::MAX - 1, false));
    assert!(!output_position_is_valid(u64::MAX, false));
    assert!(!output_position_is_valid(0, true));
}

#[test]
fn production_acknowledgement_preserves_replay_gaps_and_maximum_cursor_boundaries() {
    assert!(cursor_advances(0, u64::MAX));
    assert!(!cursor_advances(u64::MAX, u64::MAX));
    assert!(!cursor_advances(u64::MAX, 0));
    assert!(acknowledgement_is_legal(u64::MAX, u64::MAX, u64::MAX, false, false));
    assert!(!acknowledgement_is_legal(u64::MAX, u64::MAX, 0, false, true));
    assert!(acknowledgement_is_legal(0, u64::MAX, u64::MAX, false, true));
    assert!(!acknowledgement_is_legal(0, u64::MAX, u64::MAX, false, false));
    assert!(!acknowledgement_is_legal(0, u64::MAX, u64::MAX, true, true));
    assert!(delivery_window_is_safe(u64::MAX, u64::MAX, usize::MAX, usize::MAX));
    assert!(!delivery_window_is_safe(u64::MAX, 0, 0, 1));
    assert!(!delivery_window_is_safe(0, u64::MAX, usize::MAX, usize::MAX - 1));
}
