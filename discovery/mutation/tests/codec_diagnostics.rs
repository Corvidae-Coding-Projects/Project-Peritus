//! Human diagnostics must retain a distinguishable cause and the reported offset.
//! Expectations concern usable errors, not exact production wording.
use std::collections::{BTreeMap, BTreeSet};

use peritus_codec::{
    CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, CodecLimits, decode_frame,
};

fn error<T: std::fmt::Debug>(result: Result<T, CodecError>) -> CodecError {
    result.expect_err("the malformed input must be rejected")
}

#[test]
fn reachable_codec_errors_have_actionable_distinct_diagnostics() {
    let limits = CodecLimits::PRODUCTION;
    let mut errors = vec![error(CanonicalReader::new(&[], limits).read_u8())];
    let header = *b"PRTS\0\x01\0\x01\0\x01\0\0\0\0\0\0";
    for (offset, value) in [(0, b'X'), (5, 0), (7, 0), (9, 0), (11, 1)] {
        let mut bytes = header;
        bytes[offset] = value;
        errors.push(error(decode_frame(&bytes, limits)));
    }
    let mut trailing = header.to_vec();
    trailing.push(7);
    errors.push(error(decode_frame(&trailing, limits)));
    errors.push(error(CanonicalReader::new(&[0, 0, 0, 1, 255], limits).read_str()));
    for prefix in [0, 1, 7, 63, 1024] {
        let mut bytes = vec![0; prefix];
        bytes.push(2);
        let mut reader = CanonicalReader::new(&bytes, limits);
        for _ in 0..prefix {
            reader.read_u8().unwrap();
        }
        errors.push(error(reader.read_bool()));
        let mut reader = CanonicalReader::new(&bytes, limits);
        for _ in 0..prefix {
            reader.read_u8().unwrap();
        }
        errors.push(error(reader.read_option_tag()));
    }
    let constrained = CodecLimits::new(17, 1, 0, 0, 0, 0);
    errors.push(error(CanonicalWriter::new(constrained).write_str("a")));
    errors.push(error(CanonicalWriter::new(constrained).write_bytes(&[1])));
    errors.push(error(CanonicalWriter::new(constrained).write_collection_len(1)));
    errors.push(error(CanonicalWriter::new(constrained).write_fixed(&[1, 2])));
    errors.push(error(CanonicalWriter::new(constrained).nested(|_| Ok(()))));
    errors.push(error(decode_frame(&header, CodecLimits { max_frame_bytes: 1, ..limits })));
    let mut declared = header;
    declared[15] = 2;
    errors.push(error(decode_frame(&declared, CodecLimits { max_payload_bytes: 1, ..limits })));

    let mut messages = BTreeMap::new();
    let mut kinds = BTreeSet::new();
    for failure in errors {
        let message = failure.to_string();
        assert!(
            message.chars().any(char::is_alphanumeric),
            "empty/noninformative diagnostic for {failure:?}"
        );
        assert!(
            message.contains(&failure.offset().to_string()),
            "diagnostic lost the reported byte offset: {failure:?}: {message:?}"
        );
        let identity = format!("{:?}/{:?}/{}", failure.kind(), failure.limit(), failure.offset());
        if let Some(previous) = messages.insert(message.clone(), identity.clone()) {
            assert_eq!(
                previous, identity,
                "distinct causes or offsets became the same human diagnostic: {message:?}"
            );
        }
        kinds.insert(format!("{:?}", failure.kind()));
    }
    assert!(kinds.len() >= 10, "error generation did not reach the promised failure families");
    // This public constructor is the protocol-layer path for unknown domain tags.
    for kind in [CodecErrorKind::UnknownTag, CodecErrorKind::InvalidDomainValue] {
        let failure = CodecError::at(kind, 42);
        assert!(!failure.to_string().trim().is_empty());
        assert!(failure.to_string().contains("42"));
    }
    eprintln!("MUTATION_DIAGNOSTICS meaningful_reachable_error_families={}", kinds.len());
}
