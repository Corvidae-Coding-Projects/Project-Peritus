use super::*;
use peritus_codec::CodecLimits;

fn scope() -> crate::WorkbenchQuery {
    crate::WorkbenchQuery::new(
        crate::ConversationId::new([1; 16]).expect("conversation"),
        peritus_types::WorkspaceId::new([2; 16]).expect("workspace"),
    )
}

#[test]
fn image_page_rejects_oversized_row_count_before_reading_any_rows() {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_query(&mut writer, WorkbenchImageQuery::new(scope(), 1, 0).expect("query"))
        .expect("query");
    writer.write_u32(33).expect("total");
    writer.write_u16(33).expect("count");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(
        read_page(&mut reader).expect_err("bounded before rows").kind(),
        CodecErrorKind::LimitExceeded
    );
}

#[test]
fn image_query_rejects_unfenced_continuation_and_accepts_large_fenced_offsets() {
    {
        let (revision, offset) = (0, 1);
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        super::super::workbench::write_query(&mut writer, scope()).expect("scope");
        writer.write_u64(revision).expect("revision");
        writer.write_u32(offset).expect("offset");
        let bytes = writer.into_bytes();
        let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
        assert_eq!(
            read_query(&mut reader).expect_err("reject").kind(),
            CodecErrorKind::InvalidDomainValue
        );
    }

    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    super::super::workbench::write_query(&mut writer, scope()).expect("scope");
    writer.write_u64(1).expect("revision");
    writer.write_u32(257).expect("offset beyond legacy page cap");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    let query = read_query(&mut reader).expect("valid fenced continuation beyond legacy cap");
    assert_eq!(query.revision(), 1);
    assert_eq!(query.offset(), 257);
}
