use super::*;

#[test]
fn line_scanner_uses_full_width_counts_and_skips_counting_after_selected_lines() {
    let line = u64::from(u32::MAX);
    let mut scan =
        Scan::new(Selection::Lines { first: line + 1, last: line + 1 }, None, 6).unwrap();
    scan.line = u128::from(line);
    let mut selected = Vec::new();
    scan.accept(b"a\nb\nc\n", &mut selected).unwrap();
    assert_eq!(selected, b"b\n");
    assert_eq!(scan.finish().unwrap(), ((2, 4), peritus_codec::sha256(b"b\n")));
    let mut scan = Scan::new(Selection::Lines { first: 1, last: 1 }, None, 4).unwrap();
    scan.accept(b"a\n", &mut Vec::new()).unwrap();
    scan.accept(b"b\n", &mut Vec::new()).unwrap();
    assert_eq!(scan.line, 2);
    assert_eq!(scan.last_seen_line, 1);
    assert_eq!(scan.offset(), 4);
    let mut scan =
        Scan::new(Selection::Lines { first: u64::MAX, last: u64::MAX }, None, 1).unwrap();
    scan.line = u128::from(u64::MAX);
    scan.accept(b"\n", &mut Vec::new()).unwrap();
    assert_eq!(scan.finish().unwrap(), ((0, 1), peritus_codec::sha256(b"\n")));
}

#[test]
fn byte_range_crossing_chunks_keeps_exact_interval_digest_and_rejects_absent_end() {
    for width in 1..=10 {
        let mut scan = Scan::new(Selection::Bytes { start: 3, end: 8 }, None, 10).unwrap();
        let mut selected = Vec::new();
        for chunk in b"0123456789".chunks(width) {
            scan.accept(chunk, &mut selected).unwrap();
        }
        assert_eq!(selected, b"34567");
        assert_eq!(scan.finish().unwrap(), ((3, 8), peritus_codec::sha256(b"34567")));
    }
    assert!(Scan::new(Selection::Bytes { start: 3, end: 11 }, None, 10).is_err());
}
