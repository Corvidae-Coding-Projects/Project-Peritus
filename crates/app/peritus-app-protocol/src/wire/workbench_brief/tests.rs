use super::*;
use peritus_codec::CodecLimits;

#[test]
fn brief_wire_checks_field_count_before_allocating_or_reading_sources() {
    let query = crate::WorkbenchQuery::new(
        crate::ConversationId::new([1; 16]).expect("id"),
        peritus_types::WorkspaceId::new([2; 16]).expect("workspace"),
    );
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    super::super::workbench::write_query(&mut writer, query).expect("query");
    writer.write_u64(1).expect("revision");
    writer.write_u16(5).expect("count");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(
        read_brief(&mut reader).expect_err("excess count").kind(),
        CodecErrorKind::LimitExceeded
    );
}
#[test]
fn unknown_brief_field_rejects_without_consuming_source_data() {
    let bytes = u16::MAX.to_le_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(
        read_field(&mut reader).expect_err("unknown field").kind(),
        CodecErrorKind::UnknownTag
    );
}
