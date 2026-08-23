#![allow(missing_docs, reason = "integration-test crate")]

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits, canonical_sha256, decode_message, encode_message,
};

#[derive(Debug, Eq, PartialEq)]
struct Example(u64);

impl CanonicalEncode for Example {
    const FAMILY: u16 = 77;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_u64(self.0)
    }
}

impl CanonicalDecode for Example {
    const FAMILY: u16 = 77;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        reader.read_u64().map(Self)
    }
}

#[test]
fn typed_messages_round_trip_and_hash_complete_frames() {
    let limits = CodecLimits::PRODUCTION;
    let encoded = encode_message(&Example(42), limits).unwrap();
    assert_eq!(decode_message::<Example>(&encoded, limits).unwrap(), Example(42));
    assert_eq!(canonical_sha256(&Example(42), limits).unwrap(), peritus_codec::sha256(&encoded));
}

#[test]
fn typed_decode_rejects_wrong_family_schema_and_payload_trailing_bytes() {
    let limits = CodecLimits::PRODUCTION;
    let encoded = peritus_codec::encode_frame(78, 1, &[0; 8], limits).unwrap();
    assert_eq!(
        decode_message::<Example>(&encoded, limits).unwrap_err().kind(),
        CodecErrorKind::WrongFamily
    );
    let encoded = peritus_codec::encode_frame(77, 2, &[0; 8], limits).unwrap();
    assert_eq!(
        decode_message::<Example>(&encoded, limits).unwrap_err().kind(),
        CodecErrorKind::WrongSchemaVersion
    );
    let encoded = peritus_codec::encode_frame(77, 1, &[0; 9], limits).unwrap();
    assert_eq!(
        decode_message::<Example>(&encoded, limits).unwrap_err().kind(),
        CodecErrorKind::TrailingBytes
    );
}
