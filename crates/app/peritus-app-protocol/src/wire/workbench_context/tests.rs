use super::*;
use peritus_codec::CodecLimits;

fn query() -> WorkbenchContextQuery {
    WorkbenchContextQuery::new(
        crate::WorkbenchQuery::new(
            crate::ConversationId::new([1; 16]).expect("conversation"),
            peritus_types::WorkspaceId::new([2; 16]).expect("workspace"),
        ),
        1,
        0,
        V::Next,
    )
    .expect("query")
}

#[test]
fn rejects_an_excessive_row_count_before_attempting_to_read_any_row() {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_query(&mut writer, query()).expect("query");
    writer.write_u32(33).expect("total");
    writer.write_bool(false).expect("no seal");
    writer.write_u16(33).expect("count");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    let error = read_page(&mut reader).expect_err("reject count");
    assert_eq!(error.kind(), CodecErrorKind::LimitExceeded);
}

#[test]
fn context_query_unknown_view_rejects_without_interpreting_additional_fields() {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    super::super::workbench::write_query(&mut writer, query().query()).expect("scope");
    writer.write_u64(1).expect("revision");
    writer.write_u32(0).expect("offset");
    writer.write_u16(u16::MAX).expect("unknown view");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert!(read_query(&mut reader).is_err());
}
