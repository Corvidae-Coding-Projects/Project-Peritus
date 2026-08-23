#![allow(missing_docs, reason = "integration-test crate")]

use peritus_codec::{
    CodecErrorKind, CodecLimit, CodecLimits, FORMAT_VERSION, HEADER_LEN, MAGIC, decode_frame,
    encode_frame, sha256,
};

const fn limits() -> CodecLimits {
    CodecLimits::new(64, 48, 8, 8, 8, 4)
}

#[test]
fn version_one_frame_has_exact_header_and_round_trips() {
    let frame = encode_frame(0x0102, 0x0304, &[5, 6, 7], limits()).unwrap();
    assert_eq!(&frame[..4], &MAGIC);
    assert_eq!(&frame[4..6], &FORMAT_VERSION.to_be_bytes());
    assert_eq!(&frame[6..8], &[1, 2]);
    assert_eq!(&frame[8..10], &[3, 4]);
    assert_eq!(&frame[10..12], &[0, 0]);
    assert_eq!(&frame[12..16], &[0, 0, 0, 3]);
    assert_eq!(&frame[16..], &[5, 6, 7]);

    let decoded = decode_frame(&frame, limits()).unwrap();
    assert_eq!(decoded.header().family(), 0x0102);
    assert_eq!(decoded.header().schema_version(), 0x0304);
    assert_eq!(decoded.header().payload_len(), 3);
    assert_eq!(decoded.payload(), [5, 6, 7]);
    assert_eq!(sha256(&frame), sha256(&frame));
}

#[test]
fn frame_rejects_reserved_fields_and_resource_overruns() {
    assert_eq!(
        encode_frame(0, 1, &[], limits()).unwrap_err().kind(),
        CodecErrorKind::InvalidFamily
    );
    assert_eq!(
        encode_frame(1, 0, &[], limits()).unwrap_err().kind(),
        CodecErrorKind::InvalidSchemaVersion
    );
    assert_eq!(
        encode_frame(1, 1, &[0; 49], limits()).unwrap_err().limit(),
        Some(CodecLimit::PayloadBytes)
    );
    let small_frame = CodecLimits::new(HEADER_LEN, 48, 8, 8, 8, 4);
    assert_eq!(
        encode_frame(1, 1, &[1], small_frame).unwrap_err().limit(),
        Some(CodecLimit::FrameBytes)
    );
}

#[test]
fn every_header_and_payload_truncation_is_rejected() {
    let frame = encode_frame(1, 1, &[1, 2, 3, 4], limits()).unwrap();
    for length in 0..frame.len() {
        assert_eq!(
            decode_frame(&frame[..length], limits()).unwrap_err().kind(),
            CodecErrorKind::Truncated,
            "length {length}"
        );
    }
}

#[test]
fn malformed_headers_and_trailing_data_fail_exactly() {
    let frame = encode_frame(1, 1, &[1], limits()).unwrap();
    let cases = [
        (0usize, 0u8, CodecErrorKind::InvalidMagic),
        (5, 2, CodecErrorKind::UnsupportedFormatVersion),
        (7, 0, CodecErrorKind::InvalidFamily),
        (9, 0, CodecErrorKind::InvalidSchemaVersion),
        (11, 1, CodecErrorKind::NonzeroFlags),
    ];
    for (index, value, kind) in cases {
        let mut corrupt = frame.clone();
        corrupt[index] = value;
        assert_eq!(decode_frame(&corrupt, limits()).unwrap_err().kind(), kind);
    }
    let mut trailing = frame;
    trailing.push(9);
    assert_eq!(
        decode_frame(&trailing, limits()).unwrap_err().kind(),
        CodecErrorKind::TrailingBytes
    );
}
