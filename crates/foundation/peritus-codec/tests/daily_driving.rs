#![allow(missing_docs, reason = "integration-test crate")]

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
    CodecLimits, decode_frame, decode_message, encode_frame, encode_message,
};

const MIB: usize = 1024 * 1024;

#[derive(Debug, Eq, PartialEq)]
struct Record {
    text: String,
    body: Vec<u8>,
    positions: Vec<u64>,
}

impl CanonicalEncode for Record {
    const FAMILY: u16 = 77;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        writer.write_str(&self.text)?;
        writer.write_bytes(&self.body)?;
        writer.write_collection_len(self.positions.len())?;
        for position in &self.positions {
            writer.write_u64(*position)?;
        }
        Ok(())
    }
}

impl CanonicalDecode for Record {
    const FAMILY: u16 = 77;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let text = reader.read_str()?.to_owned();
        let body = reader.read_bytes_owned()?;
        let count = reader.read_collection_len(8)?;
        let mut positions = reader.reserve_collection(count)?;
        for _ in 0..count {
            positions.push(reader.read_u64()?);
        }
        Ok(Self { text, body, positions })
    }
}

#[test]
fn production_string_round_trips_past_the_former_field_ceiling() {
    round_trip(&Record { text: "a".repeat(MIB + 1), body: Vec::new(), positions: Vec::new() });
}

#[test]
fn production_opaque_field_round_trips_past_the_former_field_ceiling() {
    round_trip(&Record { text: String::new(), body: vec![7; 8 * MIB + 1], positions: Vec::new() });
}

#[test]
fn production_collection_round_trips_past_the_former_item_ceiling() {
    round_trip(&Record { text: String::new(), body: Vec::new(), positions: (0..65_536).collect() });
}

fn round_trip(record: &Record) {
    let bytes = encode_message(record, CodecLimits::PRODUCTION).unwrap();
    assert_eq!(&decode_message::<Record>(&bytes, CodecLimits::PRODUCTION).unwrap(), record);
}

#[test]
fn production_frame_round_trips_past_the_former_whole_message_ceiling() {
    let body = vec![13; 16 * MIB + 1];
    let bytes = encode_frame(77, 1, &body, CodecLimits::PRODUCTION).unwrap();
    assert_eq!(decode_frame(&bytes, CodecLimits::PRODUCTION).unwrap().payload(), body);
}

#[test]
fn repeated_receive_windows_have_no_cumulative_frame_allowance() {
    let encoded = encode_frame(77, 1, &vec![31; 16 * MIB + 1], CodecLimits::PRODUCTION).unwrap();
    let mut receiver = peritus_codec::FrameReceiver::new(CodecLimits::PRODUCTION);
    let mut offset = 0;
    while let Some(buffer) = receiver.prepare_read().unwrap() {
        let count = buffer.len().min(encoded.len() - offset);
        buffer[..count].copy_from_slice(&encoded[offset..offset + count]);
        receiver.accept_read(count).unwrap();
        offset += count;
    }
    assert_eq!(offset, encoded.len());
    assert_eq!(receiver.take_frame(CodecLimits::PRODUCTION).unwrap(), encoded);
    assert!(receiver.is_empty());
}

#[test]
fn individually_legacy_sized_fields_do_not_exhaust_a_logical_payload_allowance() {
    let field = vec![17; 8 * MIB];
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(&field).unwrap();
    writer.write_bytes(&field).unwrap();
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(reader.read_bytes().unwrap(), field);
    assert_eq!(reader.read_bytes().unwrap(), field);
    reader.finish().unwrap();
}

fn write_nested(writer: &mut CanonicalWriter, depth: usize) -> Result<(), CodecError> {
    if depth == 0 {
        writer.write_u8(23)
    } else {
        writer.nested(|writer| write_nested(writer, depth - 1))
    }
}

fn read_nested(reader: &mut CanonicalReader<'_>, depth: usize) -> Result<u8, CodecError> {
    if depth == 0 {
        reader.read_u8()
    } else {
        reader.nested(|reader| read_nested(reader, depth - 1))
    }
}

#[test]
fn schema_defined_nesting_past_sixty_four_is_not_a_production_work_quota() {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_nested(&mut writer, 65).unwrap();
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(read_nested(&mut reader, 65).unwrap(), 23);
    reader.finish().unwrap();
}

#[test]
fn incomplete_declared_values_are_rejected_before_owned_decoding() {
    let bytes = u32::MAX.to_be_bytes();
    assert_eq!(
        CanonicalReader::new(&bytes, CodecLimits::PRODUCTION)
            .read_bytes_owned()
            .unwrap_err()
            .kind(),
        CodecErrorKind::Truncated,
    );
}

#[test]
fn impossible_collection_extent_rejects_before_reserving_owned_items() {
    let bytes = u32::MAX.to_be_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(reader.read_collection_len(16).unwrap_err().kind(), CodecErrorKind::Truncated);
    // A real representability/capacity failure remains typed, without attempting an allocation.
    assert_eq!(
        reader.reserve_collection::<u64>(usize::MAX).unwrap_err().kind(),
        CodecErrorKind::AllocationUnavailable
    );
}

#[test]
fn explicit_payload_contract_is_shared_by_direct_reader_and_writer() {
    let limits = CodecLimits::new(20, 4, 4, 4, 4, 4);
    let payload = [3; 5];
    assert_eq!(
        CanonicalWriter::new(limits).write_fixed(&payload).unwrap_err().kind(),
        CodecErrorKind::LimitExceeded
    );
    assert_eq!(
        CanonicalReader::new(&payload, limits).read_u8().unwrap_err().kind(),
        CodecErrorKind::LimitExceeded
    );
    assert_eq!(
        CanonicalReader::new(&payload, limits).finish().unwrap_err().kind(),
        CodecErrorKind::LimitExceeded
    );
}

#[test]
fn legacy_profile_keeps_exact_bytes_and_production_reopens_them() {
    let record = Record {
        text: "exact legacy bytes".to_owned(),
        body: vec![1, 2, 3],
        positions: vec![5, 8],
    };
    let legacy = encode_message(&record, CodecLimits::LEGACY_V1).unwrap();
    assert_eq!(encode_message(&record, CodecLimits::PRODUCTION).unwrap(), legacy);
    assert_eq!(decode_message::<Record>(&legacy, CodecLimits::PRODUCTION).unwrap(), record);
    let over = Record { text: "a".repeat(MIB + 1), body: Vec::new(), positions: Vec::new() };
    assert_eq!(
        encode_message(&over, CodecLimits::LEGACY_V1).unwrap_err().kind(),
        CodecErrorKind::LimitExceeded
    );
    let expanded = encode_message(&over, CodecLimits::PRODUCTION).unwrap();
    assert_eq!(
        decode_message::<Record>(&expanded, CodecLimits::LEGACY_V1).unwrap_err().kind(),
        CodecErrorKind::LimitExceeded
    );
}
