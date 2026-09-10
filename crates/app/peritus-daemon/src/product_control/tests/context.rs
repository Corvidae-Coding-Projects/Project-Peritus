use super::*;
use peritus_app_protocol::{
    WorkbenchContextDisposition as D, WorkbenchContextQuery, WorkbenchContextSource as S,
    WorkbenchContextView as V, WorkbenchInvocationId, WorkbenchQuery,
};
use peritus_product_runner::control::{InputId, InvocationId, QueueIntent};
use rusqlite::params;

fn actor() -> ActorId {
    ActorId::new([3; 16]).expect("actor")
}
fn query(view: V, revision: u64, offset: u32) -> WorkbenchContextQuery {
    WorkbenchContextQuery::new(
        WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([2; 16]).expect("conversation"),
            WorkspaceId::new([4; 16]).expect("workspace"),
        ),
        revision,
        offset,
        view,
    )
    .expect("query")
}
fn enqueue(journal: &mut ControlStore, index: u32, revision: u64, id: u8, text: &str) {
    journal
        .accept(&operation(
            index,
            revision,
            ControlIntent::Queue(QueueIntent::Enqueue {
                id: InputId::new([id; 16]).expect("input"),
                text: ControlText::new(text.to_owned()).expect("text"),
                dependencies: Vec::new(),
            }),
        ))
        .expect("enqueue");
}

#[test]
fn sealed_context_is_exact_content_free_read_only_scoped_and_restart_stable() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    enqueue(&mut journal, 2, 1, 10, "PUBLIC_INPUT_MARKER");
    let captured = journal
        .capture_inputs(create().conversation(), actor(), query(V::Next, 0, 0).query().workspace())
        .expect("capture");
    let request = crate::product_control::inputs::tests::request(&format!(
        "{}\nPRIVATE_MESSAGE_MARKER",
        captured.inputs().conversation()
    ));
    let invocation = InvocationId::new([11; 16]).expect("invocation");
    journal.prepare_inputs(&captured, invocation, &request).expect("prepare");
    let view = V::Invocation(WorkbenchInvocationId::new([11; 16]).expect("invocation"));
    let sealed = journal.context_page(actor(), query(view, 0, 0)).expect("sealed");
    assert_eq!(sealed.rows().len(), 2);
    assert_eq!(
        sealed.seal().expect("seal").request_digest(),
        request.fingerprint().expect("digest").digest()
    );
    assert_eq!(sealed.rows()[0].digest(), sha256(b"PUBLIC_INPUT_MARKER"));
    let message = peritus_model_protocol::encode_messages(
        request.messages(),
        peritus_model_protocol::ProtocolLimits::PRODUCTION,
    )
    .expect("message archive");
    assert_eq!(sealed.rows()[1].digest(), sha256(&message));
    assert_eq!(sealed.rows()[1].bytes(), message.len() as u64);
    let debug = format!("{sealed:?}");
    assert!(!debug.contains("PUBLIC_INPUT_MARKER"));
    assert!(!debug.contains("PRIVATE_MESSAGE_MARKER"));
    assert!(matches!(
        journal.context_page(ActorId::new([19; 16]).expect("wrong actor"), query(view, 0, 0)),
        Err(Error::Control(ControlError::ScopeMismatch))
    ));
    let wrong = WorkbenchContextQuery::new(
        WorkbenchQuery::new(
            query(view, 0, 0).query().conversation(),
            WorkspaceId::new([20; 16]).expect("wrong workspace"),
        ),
        0,
        0,
        view,
    )
    .expect("query");
    assert!(matches!(
        journal.context_page(actor(), wrong),
        Err(Error::Control(ControlError::ScopeMismatch))
    ));
    assert_eq!(journal.load(create().conversation()).expect("root").expect("record").revision(), 3);
    drop(journal);
    let mut journal = store(root.path());
    assert_eq!(journal.context_page(actor(), query(view, 0, 0)).expect("restart"), sealed);
    enqueue(&mut journal, 3, 3, 12, "Later instruction");
    let later = journal.context_page(actor(), query(view, 0, 0)).expect("old sealed view");
    assert_eq!(later.rows(), sealed.rows());
    assert_eq!(later.seal(), sealed.seal());
    assert!(matches!(
        journal.context_page(actor(), query(view, 3, 0)),
        Err(Error::Control(ControlError::StaleRevision))
    ));
    let history = journal.context_page(actor(), query(V::History, 0, 0)).expect("history");
    assert_eq!(history.rows()[0].digest(), sealed.seal().expect("seal").request_digest());
    assert!(matches!(history.rows()[0].source(), S::Invocation { .. }));
}

#[test]
fn context_pagination_and_exclusions_track_exact_revisions_without_incorporation() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    for id in 10_u8..45 {
        enqueue(&mut journal, u32::from(id), u64::from(id - 9), id, "Exact source");
    }
    let before = journal.load(create().conversation()).expect("root").expect("record");
    let selected = peritus_product_runner::control::InputSelection::new(
        InputId::new([10; 16]).expect("input"),
        1,
    )
    .expect("selection");
    journal
        .accept(&operation(
            50,
            36,
            ControlIntent::Queue(QueueIntent::Hold { selected, held: true }),
        ))
        .expect("hold");
    let first = journal.context_page(actor(), query(V::Next, 0, 0)).expect("first");
    assert_eq!(first.total(), 35);
    assert_eq!(first.rows().len(), 32);
    assert_eq!(first.rows()[0].disposition(), D::Held);
    assert!(first.rows()[1..].iter().all(|row| row.disposition() == D::Eligible));
    let last = journal.context_page(actor(), query(V::Next, 37, 32)).expect("last");
    assert_eq!(last.rows().len(), 3);
    assert!(journal.context_page(actor(), query(V::Next, 37, 36)).is_err());
    assert!(
        journal.load(before.id()).expect("root").expect("record").inputs().invocations().is_empty()
    );
}

#[test]
fn manifest_metadata_tampering_is_detected_even_with_a_recomputed_artifact_digest() {
    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    enqueue(&mut journal, 2, 1, 10, "Exact input");
    let captured = journal
        .capture_inputs(create().conversation(), actor(), query(V::Next, 0, 0).query().workspace())
        .expect("capture");
    let invocation = InvocationId::new([11; 16]).expect("invocation");
    journal
        .prepare_inputs(
            &captured,
            invocation,
            &crate::product_control::inputs::tests::request(captured.inputs().conversation()),
        )
        .expect("prepare");
    let bytes = journal.invocation_manifest(create().conversation(), invocation).expect("manifest");
    let mut data: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    data["messages"][0]["encoded_bytes"] = serde_json::Value::from(9876_u64);
    let changed = serde_json::to_vec(&data).expect("changed");
    let connection =
        rusqlite::Connection::open(root.path().join("control.sqlite3")).expect("inspection");
    connection
        .execute(
            "UPDATE state_records SET value = ?1, value_digest = ?2 WHERE namespace = 3405",
            params![changed, sha256(&changed).as_bytes().as_slice()],
        )
        .expect("fixture tamper");
    assert!(journal.context_page(actor(), query(V::History, 0, 0)).is_err());
}

#[test]
fn context_preferences_change_only_future_request_views_and_survive_restart() {
    use peritus_product_runner::control::{ContextPreference, ContextTarget};

    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    enqueue(&mut journal, 2, 1, 10, "Original user question");
    let start =
        operation(3, 2, ControlIntent::StartExecution { run: [5; 16], settings_digest: [6; 32] });
    journal.accept(&start).expect("start");
    let captured = journal.capture_execution(&start).expect("capture");
    let invocation = InvocationId::new([11; 16]).expect("invocation");
    journal
        .prepare_inputs(
            &captured,
            invocation,
            &crate::product_control::inputs::tests::request(captured.inputs().conversation()),
        )
        .expect("prepare");
    let reply = "An exact public reply retained independently from the active prompt view.";
    journal.publish_reply(&start, reply).expect("reply");
    assert!(
        !journal
            .capture_execution(&start)
            .expect("ordinary view")
            .inputs()
            .conversation()
            .contains(reply)
    );

    let revision = journal.load(create().conversation()).expect("load").expect("record").revision();
    journal
        .accept(&operation(
            20,
            revision,
            ControlIntent::SetContext {
                target: ContextTarget::PublicReply(invocation),
                preference: Some(ContextPreference::Pinned),
            },
        ))
        .expect("pin");
    assert!(
        journal
            .capture_execution(&start)
            .expect("pinned view")
            .inputs()
            .conversation()
            .contains(reply)
    );
    let page = journal.context_page(actor(), query(V::Next, 0, 0)).expect("page");
    let row = page
        .rows()
        .iter()
        .find(|row| matches!(row.source(), S::PublicReply(id) if id.as_bytes() == invocation.as_bytes()))
        .expect("reply row");
    assert_eq!(row.disposition(), D::Eligible);
    assert_eq!(row.preference(), Some(peritus_app_protocol::WorkbenchContextPreference::Pinned));

    drop(journal);
    let mut journal = store(root.path());
    assert!(
        journal
            .capture_execution(&start)
            .expect("restart pinned view")
            .inputs()
            .conversation()
            .contains(reply)
    );
    let revision = journal.load(create().conversation()).expect("load").expect("record").revision();
    journal
        .accept(&operation(
            21,
            revision,
            ControlIntent::SetContext {
                target: ContextTarget::PublicReply(invocation),
                preference: Some(ContextPreference::Excluded),
            },
        ))
        .expect("exclude");
    assert!(
        !journal
            .capture_execution(&start)
            .expect("excluded view")
            .inputs()
            .conversation()
            .contains(reply)
    );
    let page = journal.context_page(actor(), query(V::Next, 0, 0)).expect("page");
    let row = page
        .rows()
        .iter()
        .find(|row| matches!(row.source(), S::PublicReply(id) if id.as_bytes() == invocation.as_bytes()))
        .expect("reply row");
    assert_eq!(row.disposition(), D::UserExcluded);
    assert_eq!(row.preference(), Some(peritus_app_protocol::WorkbenchContextPreference::Excluded));
}

#[test]
fn compact_preview_confirmation_changes_next_view_but_not_exact_archives() {
    use peritus_product_runner::control::{ContextPreference, ContextTarget};

    let root = tempfile::tempdir().expect("root");
    let mut journal = store(root.path());
    journal.accept(&create()).expect("create");
    enqueue(&mut journal, 2, 1, 20, "Initial request");
    let start =
        operation(3, 2, ControlIntent::StartExecution { run: [5; 16], settings_digest: [6; 32] });
    journal.accept(&start).expect("start");
    let mut invocations = Vec::new();
    let mut replies = Vec::new();
    let mut first_manifest = None;
    for turn in 0_u8..5 {
        let captured = journal.capture_execution(&start).expect("capture");
        let invocation = InvocationId::new([30 + turn; 16]).expect("invocation");
        journal
            .prepare_inputs(
                &captured,
                invocation,
                &crate::product_control::inputs::tests::request(captured.inputs().conversation()),
            )
            .expect("prepare");
        if turn == 0 {
            first_manifest = Some(
                journal.invocation_manifest(create().conversation(), invocation).expect("manifest"),
            );
        }
        let reply = format!("EXACT_REPLY_{turn}: {}", "source evidence ".repeat(48));
        journal.publish_reply(&start, &reply).expect("reply");
        invocations.push(invocation);
        replies.push(reply);
        let revision =
            journal.load(create().conversation()).expect("load").expect("record").revision();
        enqueue(
            &mut journal,
            100 + u32::from(turn),
            revision,
            40 + turn,
            &format!("Follow-up {turn}"),
        );
    }
    let revision = journal.load(create().conversation()).expect("load").expect("record").revision();
    journal
        .accept(&operation(
            200,
            revision,
            ControlIntent::SetContext {
                target: ContextTarget::PublicReply(invocations[0]),
                preference: Some(ContextPreference::Pinned),
            },
        ))
        .expect("pin first reply");
    let revision = journal.load(create().conversation()).expect("load").expect("record").revision();
    let request = peritus_app_protocol::WorkbenchCompactionRequest::new(
        query(V::Next, 0, 0).query(),
        revision,
        None,
    )
    .expect("request");
    let (view, preview) = journal.compaction_preview(actor(), &request, true).expect("preview");
    assert_eq!(preview.entries().len(), 2);
    assert_eq!(preview.pinned_preserved(), 1);
    assert_eq!(preview.recent_preserved(), 2);
    let apply =
        operation(201, revision, ControlIntent::ApplyPromptView(view.expect("applicable view")));
    journal.accept(&apply).expect("apply");

    let next = journal.capture_execution(&start).expect("next view");
    let text = next.inputs().conversation();
    assert!(text.contains("EXACT_REPLY_0"), "pinned reply must remain verbatim");
    assert!(text.contains("EXACT_REPLY_3"), "recent reply must remain verbatim");
    assert!(text.contains("EXACT_REPLY_4"), "recent reply must remain verbatim");
    assert!(!text.contains("EXACT_REPLY_1"));
    assert!(!text.contains("EXACT_REPLY_2"));
    assert!(text.contains("Structural source handle"));
    assert!(text.contains("not a semantic summary"));
    assert_eq!(
        journal
            .invocation_manifest(create().conversation(), invocations[0])
            .expect("unchanged manifest"),
        first_manifest.expect("first manifest")
    );
    let record = journal.load(create().conversation()).expect("load").expect("record");
    for (reference, exact) in record.replies().iter().zip(&replies) {
        assert_eq!(journal.reply_text(reference).expect("exact reply archive"), *exact);
    }
    drop(journal);
    let journal = store(root.path());
    assert!(
        journal
            .capture_execution(&start)
            .expect("restart view")
            .inputs()
            .conversation()
            .contains("Structural source handle")
    );
}
