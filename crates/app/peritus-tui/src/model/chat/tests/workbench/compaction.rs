use super::*;
use peritus_app_protocol::{WorkbenchCompactionEntry, WorkbenchCompactionPreview, WorkbenchIntent};

fn opened() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchCompaction)
            .expect("feature"),
    );
    let query = peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([64; 16]).expect("conversation"),
        model.product.as_ref().expect("product").launch.workspace_id(),
    );
    model.chat.workbench.selected = Some(query);
    model.chat.workbench.snapshot = Some(
        WorkbenchSnapshot::new(
            query,
            9,
            ConversationTitle::new("Compaction fixture".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("snapshot"),
    );
    model
}

#[test]
fn compact_previews_then_confirms_the_exact_revision_fenced_source_handles() {
    let mut model = opened();
    model.chat.buffer = "/compact preserve decisions".to_owned();
    let preview_request = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::PreviewWorkbenchCompaction(compaction_request) =
        preview_request.payload()
    else {
        panic!("preview request");
    };
    assert_eq!(compaction_request.revision(), 9);
    assert_eq!(compaction_request.focus().expect("focus").as_str(), "preserve decisions");
    let entries = [70_u8, 71]
        .into_iter()
        .map(|id| {
            WorkbenchCompactionEntry::new(
                peritus_app_protocol::WorkbenchInvocationId::new([id; 16]).expect("invocation"),
                peritus_types::Sha256Digest::new([id + 1; 32]),
                800,
                peritus_types::Sha256Digest::new([id + 2; 32]),
                220,
            )
            .expect("entry")
        })
        .collect();
    let preview =
        WorkbenchCompactionPreview::new(compaction_request.clone(), 2, 2, 1, 0, 1, entries)
            .expect("preview");
    respond(
        &mut model,
        &preview_request,
        AppResponsePayload::WorkbenchCompactionPreview(preview.clone()),
    );
    assert_eq!(model.chat.buffer, "/compact preserve decisions");
    let apply_request = request(&key(&mut model, KeyCode::Char('c')));
    let AppRequestPayload::WorkbenchCommand(command) = apply_request.payload() else {
        panic!("apply command");
    };
    assert_eq!(command.expected_revision(), 9);
    assert!(
        matches!(command.intent(), WorkbenchIntent::ApplyCompaction(value) if value == &preview)
    );
    assert_eq!(model.chat.buffer, "/compact preserve decisions");

    let refresh = request(&respond(&mut model, &apply_request, receipt(command)));
    assert!(matches!(refresh.payload(), AppRequestPayload::QueryWorkbench(_)));
    assert!(model.chat.buffer.is_empty());
}
