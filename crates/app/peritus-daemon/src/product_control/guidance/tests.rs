use super::*;
use peritus_app_protocol::{
    ConversationId, WorkbenchGuidanceContent, WorkbenchGuidanceReason, WorkbenchGuidanceScope,
    WorkbenchGuidanceSource, WorkbenchGuidanceText, render_guidance_for_request,
};
use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_types::{CommandId, EventId, EventSequence};
use std::{path::Path, time::Duration};
use tempfile::TempDir;

fn operation(byte: u8) -> ControlOperationId {
    ControlOperationId::new([byte; 16]).expect("operation")
}

fn workspace() -> WorkspaceId {
    WorkspaceId::new([31; 16]).expect("workspace")
}

fn conversation() -> ConversationId {
    ConversationId::new([32; 16]).expect("conversation")
}

fn store() -> StoreId {
    StoreId::new([33; 16]).expect("store")
}

fn aggregate() -> AggregateKey {
    AggregateKey::new(AggregateKind::Application, AggregateId::new([34; 16]).expect("aggregate"))
}

fn open(path: &Path) -> SqliteJournal {
    SqliteJournal::open(
        path.join("guidance.sqlite3"),
        store(),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .expect("journal")
}

fn content(text: &str) -> WorkbenchGuidanceContent {
    WorkbenchGuidanceContent::new(
        WorkbenchGuidanceText::new(text.to_owned()).expect("text"),
        WorkbenchGuidanceSource::UserAuthored,
        WorkbenchGuidanceScope::Project,
    )
    .expect("content")
}

fn commit(
    journal: &mut SqliteJournal,
    operation: ControlOperationId,
    audit: &str,
    installs: Vec<StateInstall>,
) {
    let aggregate = aggregate();
    let head = journal.head(aggregate).expect("head");
    let sequence = head.as_ref().map_or(1, |value| value.sequence().get() + 1);
    let event_id =
        EventId::new([u8::try_from(100 + sequence).expect("small sequence"); 16]).expect("event");
    let frame = ExactFrame::new(
        encode_frame(3541, 1, audit.as_bytes(), CodecLimits::PRODUCTION).expect("audit frame"),
    )
    .expect("exact frame");
    let event = EventDraft::new(
        aggregate,
        EventSequence::new(sequence).expect("sequence"),
        event_id,
        head.map(peritus_journal::AggregateHead::event_id),
        frame,
        sha256(audit.as_bytes()),
        Vec::new(),
    )
    .expect("event draft");
    let expected = head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present);
    let plan = AppendRequest::new(
        store(),
        CommandId::new(operation.into_bytes()).expect("command"),
        sha256(operation.as_bytes()),
        vec![expected],
        vec![event],
        installs,
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .expect("append plan");
    journal.append(plan).expect("append");
}

fn observed<'a>(
    catalog: Option<&'a DurableStateRecord>,
    guidance: Option<&'a DurableStateRecord>,
    tombstone: Option<&'a DurableStateRecord>,
) -> GuidanceObservation<'a> {
    GuidanceObservation::new(catalog, guidance, tombstone)
}

#[test]
fn c0_reopen_retains_tombstone_and_audit_while_future_render_excludes_guidance() {
    let temp = TempDir::new().expect("temporary state");
    let project = workspace();
    let identity = operation(1);
    let key = guidance_key(project, identity);
    let catalog = catalog_key(project);
    let mut journal = open(temp.path());

    let save = plan_guidance(
        identity,
        project,
        GuidanceMutation::Save(WorkbenchGuidanceSave::new(
            0,
            content("Use the focused acceptance gate."),
            false,
        )),
        observed(None, None, None),
    )
    .expect("save plan");
    assert_eq!(save.catalog().revision(), 1);
    assert!(matches!(save.projection(), GuidanceProjection::Active(_)));
    commit(&mut journal, identity, "save:Use the focused acceptance gate.", save.into_installs());

    let catalog_row = journal
        .state_record(GUIDANCE_CONTROL_NAMESPACE, &catalog)
        .expect("catalog read")
        .expect("catalog");
    let guidance_row =
        journal.state_record(GUIDANCE_NAMESPACE, &key).expect("guidance read").expect("guidance");
    let revised = plan_guidance(
        operation(2),
        project,
        GuidanceMutation::Revise(WorkbenchGuidanceRevision::new(
            peritus_app_protocol::WorkbenchGuidanceSelection::new(identity, 1).expect("selection"),
            1,
            content("Use the exact focused acceptance gate."),
        )),
        observed(Some(&catalog_row), Some(&guidance_row), None),
    )
    .expect("revise plan");
    commit(
        &mut journal,
        operation(2),
        "revise:Use the exact focused acceptance gate.",
        revised.into_installs(),
    );

    let catalog_row = journal
        .state_record(GUIDANCE_CONTROL_NAMESPACE, &catalog)
        .expect("catalog read")
        .expect("catalog");
    let guidance_row =
        journal.state_record(GUIDANCE_NAMESPACE, &key).expect("guidance read").expect("guidance");
    let pinned = plan_guidance(
        operation(3),
        project,
        GuidanceMutation::Pin(WorkbenchGuidancePin::new(
            peritus_app_protocol::WorkbenchGuidanceSelection::new(identity, 2).expect("selection"),
            2,
            true,
        )),
        observed(Some(&catalog_row), Some(&guidance_row), None),
    )
    .expect("pin plan");
    commit(&mut journal, operation(3), "pin:true", pinned.into_installs());

    let catalog_row = journal
        .state_record(GUIDANCE_CONTROL_NAMESPACE, &catalog)
        .expect("catalog read")
        .expect("catalog");
    let guidance_row =
        journal.state_record(GUIDANCE_NAMESPACE, &key).expect("guidance read").expect("guidance");
    let forget = plan_guidance(
        operation(4),
        project,
        GuidanceMutation::Forget(WorkbenchGuidanceForget::new(
            peritus_app_protocol::WorkbenchGuidanceSelection::new(identity, 3).expect("selection"),
            3,
            WorkbenchGuidanceReason::new("Superseded project practice.".to_owned())
                .expect("reason"),
        )),
        observed(Some(&catalog_row), Some(&guidance_row), None),
    )
    .expect("forget plan");
    assert!(matches!(forget.projection(), GuidanceProjection::Forgotten(_)));
    commit(&mut journal, operation(4), "forget:future retrieval only", forget.into_installs());
    drop(journal);

    let reopened = open(temp.path());
    let catalog_row = reopened
        .state_record(GUIDANCE_CONTROL_NAMESPACE, &catalog)
        .expect("reopened catalog read")
        .expect("reopened catalog");
    let rebuilt = replay_catalog(project, Some(&catalog_row)).expect("catalog replay");
    assert_eq!(rebuilt.revision(), 4);
    assert_eq!(rebuilt.identities(), &[identity]);
    let guidance_row = reopened
        .state_record(GUIDANCE_NAMESPACE, &key)
        .expect("reopened guidance read")
        .expect("reopened guidance");
    let tombstone_row = reopened
        .state_record(GUIDANCE_CONTROL_NAMESPACE, &key)
        .expect("reopened tombstone read")
        .expect("reopened tombstone");
    let WorkbenchMemoryRow::Forgotten(tombstone) =
        replay_guidance(project, identity, &guidance_row, Some(&tombstone_row))
            .expect("forgotten replay")
    else {
        panic!("forgotten row expected");
    };
    assert_eq!(tombstone.prior().revision(), 3);
    assert_eq!(tombstone.dependency_revision(), 4);
    let render = render_guidance_for_request(project, conversation(), 4, &[], &[tombstone])
        .expect("future render");
    assert!(render.identities().is_empty());
    assert!(!render.text().contains("focused acceptance gate"));

    let audit = reopened.records_for_aggregate(aggregate()).expect("immutable audit");
    assert_eq!(audit.len(), 4);
    assert_eq!(
        decode_frame(audit[0].frame_bytes(), CodecLimits::PRODUCTION)
            .expect("saved audit frame")
            .payload(),
        b"save:Use the focused acceptance gate."
    );
}
