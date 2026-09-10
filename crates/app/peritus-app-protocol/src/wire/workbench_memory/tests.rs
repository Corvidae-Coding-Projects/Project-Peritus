use super::*;
use peritus_codec::CodecLimits;

fn operation(byte: u8) -> crate::ControlOperationId {
    crate::ControlOperationId::new([byte; 16]).expect("operation")
}

fn query() -> crate::WorkbenchQuery {
    crate::WorkbenchQuery::new(
        crate::ConversationId::new([21; 16]).expect("conversation"),
        WorkspaceId::new([22; 16]).expect("workspace"),
    )
}

fn content(text: &str, scope: WorkbenchGuidanceScope) -> WorkbenchGuidanceContent {
    WorkbenchGuidanceContent::new(
        WorkbenchGuidanceText::new(text.to_owned()).expect("text"),
        WorkbenchGuidanceSource::UserAuthored,
        scope,
    )
    .expect("content")
}

fn roundtrip<T: Eq + std::fmt::Debug>(
    value: &T,
    write: impl FnOnce(&mut CanonicalWriter, &T) -> Result<(), CodecError>,
    read: impl FnOnce(&mut CanonicalReader<'_>) -> Result<T, CodecError>,
) {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write(&mut writer, value).expect("encode");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(read(&mut reader).expect("decode"), *value);
    reader.finish().expect("complete value");
}

#[test]
fn all_five_guidance_intent_bodies_roundtrip_canonically() {
    let selected = WorkbenchGuidanceSelection::new(operation(1), 7).expect("selection");
    let save = WorkbenchGuidanceSave::new(
        6,
        content("Save exact guidance", WorkbenchGuidanceScope::Project),
        true,
    );
    roundtrip(&save, write_save, read_save);

    let revision = WorkbenchGuidanceRevision::new(
        selected,
        8,
        content(
            "Revise exact guidance",
            WorkbenchGuidanceScope::Conversation(query().conversation()),
        ),
    );
    roundtrip(&revision, write_revision, read_revision);

    let pin = WorkbenchGuidancePin::new(selected, 9, false);
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_pin(&mut writer, pin).expect("pin encode");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(read_pin(&mut reader).expect("pin decode"), pin);
    reader.finish().expect("pin complete");

    let scope = WorkbenchGuidanceScopeChange::new(
        selected,
        10,
        WorkbenchGuidanceScope::Conversation(query().conversation()),
    );
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_scope_change(&mut writer, scope).expect("scope encode");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(read_scope_change(&mut reader).expect("scope decode"), scope);
    reader.finish().expect("scope complete");

    let forget = WorkbenchGuidanceForget::new(
        selected,
        11,
        WorkbenchGuidanceReason::new("Future retrieval only".to_owned()).expect("reason"),
    );
    roundtrip(&forget, write_forget, read_forget);
}

#[test]
fn active_and_content_free_forgotten_rows_roundtrip() {
    let project = query().workspace();
    let active = WorkbenchGuidanceRecord::save(
        operation(1),
        project,
        WorkbenchGuidanceSave::new(0, content("Active", WorkbenchGuidanceScope::Project), true),
    )
    .expect("active");
    let doomed = WorkbenchGuidanceRecord::save(
        operation(2),
        project,
        WorkbenchGuidanceSave::new(1, content("Doomed", WorkbenchGuidanceScope::Project), false),
    )
    .expect("doomed");
    let tombstone = doomed
        .forget(
            operation(3),
            WorkbenchGuidanceForget::new(
                WorkbenchGuidanceSelection::new(operation(2), 1).expect("selection"),
                2,
                WorkbenchGuidanceReason::new("Obsolete".to_owned()).expect("reason"),
            ),
        )
        .expect("tombstone");
    let memory_query = WorkbenchMemoryQuery::new(query(), 3, 0, true).expect("query");
    let memory = WorkbenchMemory::new(
        memory_query,
        3,
        2,
        vec![WorkbenchMemoryRow::Active(active), WorkbenchMemoryRow::Forgotten(tombstone)],
    )
    .expect("memory");
    roundtrip(&memory, write_memory, read_memory);
}

#[test]
fn excessive_row_count_rejects_before_allocating_rows() {
    let memory_query = WorkbenchMemoryQuery::new(query(), 1, 0, true).expect("query");
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_query(&mut writer, memory_query).expect("query encode");
    writer.write_u64(1).expect("revision");
    writer.write_u32(33).expect("total");
    writer.write_u16(33).expect("row count");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(
        read_memory(&mut reader).expect_err("excess rows").kind(),
        CodecErrorKind::LimitExceeded
    );
}
