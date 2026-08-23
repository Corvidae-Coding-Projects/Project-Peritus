#![allow(missing_docs, reason = "integration-test crate")]

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecErrorKind, CodecLimit, CodecLimits};

const fn limits(payload: usize) -> CodecLimits {
    CodecLimits::new(payload + peritus_codec::HEADER_LEN, payload, 3, 4, 5, 2)
}

#[test]
fn primitive_encoding_is_fixed_width_big_endian_and_round_trips() {
    let limits = limits(128);
    let mut writer = CanonicalWriter::new(limits);
    writer.write_u8(0x12).unwrap();
    writer.write_u16(0x3456).unwrap();
    writer.write_u32(0x789a_bcde).unwrap();
    writer.write_u64(0x0123_4567_89ab_cdef).unwrap();
    writer.write_bool(true).unwrap();
    writer.write_option_tag(false).unwrap();
    writer.write_bytes(&[1, 2, 3]).unwrap();
    writer.write_str("rust").unwrap();
    writer.write_collection_len(3).unwrap();
    let bytes = writer.into_bytes();

    assert_eq!(
        &bytes[..15],
        &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
    );
    let mut reader = CanonicalReader::new(&bytes, limits);
    assert_eq!(reader.read_u8().unwrap(), 0x12);
    assert_eq!(reader.read_u16().unwrap(), 0x3456);
    assert_eq!(reader.read_u32().unwrap(), 0x789a_bcde);
    assert_eq!(reader.read_u64().unwrap(), 0x0123_4567_89ab_cdef);
    assert!(reader.read_bool().unwrap());
    assert!(!reader.read_option_tag().unwrap());
    assert_eq!(reader.read_bytes().unwrap(), [1, 2, 3]);
    assert_eq!(reader.read_str().unwrap(), "rust");
    assert_eq!(reader.read_collection_len().unwrap(), 3);
    reader.finish().unwrap();
}

#[test]
fn exact_limits_succeed_and_one_over_fails_without_partial_write() {
    let limits = limits(32);
    let mut writer = CanonicalWriter::new(limits);
    writer.write_bytes(&[0; 5]).unwrap();
    let before = writer.as_slice().to_vec();
    let error = writer.write_bytes(&[0; 6]).unwrap_err();
    assert_eq!(error.kind(), CodecErrorKind::LimitExceeded);
    assert_eq!(error.limit(), Some(CodecLimit::OpaqueBytes));
    assert_eq!(writer.as_slice(), before);

    let mut writer = CanonicalWriter::new(limits);
    writer.write_str("rust").unwrap();
    let before = writer.as_slice().to_vec();
    let error = writer.write_str("rust!").unwrap_err();
    assert_eq!(error.limit(), Some(CodecLimit::StringBytes));
    assert_eq!(writer.as_slice(), before);

    let mut writer = CanonicalWriter::new(limits);
    writer.write_collection_len(3).unwrap();
    assert_eq!(
        writer.write_collection_len(4).unwrap_err().limit(),
        Some(CodecLimit::CollectionItems)
    );
}

#[test]
fn invalid_closed_tags_utf8_and_trailing_bytes_are_rejected() {
    let limits = limits(32);
    assert_eq!(
        CanonicalReader::new(&[2], limits).read_bool().unwrap_err().kind(),
        CodecErrorKind::InvalidBoolean
    );
    assert_eq!(
        CanonicalReader::new(&[3], limits).read_option_tag().unwrap_err().kind(),
        CodecErrorKind::InvalidOption
    );
    let mut invalid_utf8 = CanonicalReader::new(&[0, 0, 0, 1, 0xff], limits);
    assert_eq!(invalid_utf8.read_str().unwrap_err().kind(), CodecErrorKind::InvalidUtf8);
    assert_eq!(
        CanonicalReader::new(&[1], limits).finish().unwrap_err().kind(),
        CodecErrorKind::TrailingBytes
    );
}

#[test]
fn nesting_limit_is_exact_for_reader_and_writer() {
    let limits = limits(32);
    let mut writer = CanonicalWriter::new(limits);
    writer.nested(|writer| writer.nested(|writer| writer.write_u8(1))).unwrap();
    let error =
        writer.nested(|writer| writer.nested(|writer| writer.nested(|_| Ok(())))).unwrap_err();
    assert_eq!(error.limit(), Some(CodecLimit::NestingDepth));

    let mut reader = CanonicalReader::new(&[], limits);
    reader.nested(|reader| reader.nested(|_| Ok(()))).unwrap();
    let error =
        reader.nested(|reader| reader.nested(|reader| reader.nested(|_| Ok(())))).unwrap_err();
    assert_eq!(error.limit(), Some(CodecLimit::NestingDepth));
}
