//! Recovery when an observed run lacks durable workbench ownership.

use super::*;
use peritus_app_protocol::ProductInteractionBinding;

#[test]
fn run_without_a_durable_binding_detaches_and_routes_the_draft_to_a_new_conversation() {
    let mut model = durable_chat_model();
    let run = RunId::new([0x85; 16]).expect("run");
    let workspace = WorkspaceId::new([4; 16]).expect("workspace");
    let snapshot = ProductRunSnapshot::new(
        run,
        workspace,
        model.chat_providers().expect("providers"),
        ProductRunPhase::Complete,
        1,
        "Unbound run".to_owned(),
        "Complete".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("snapshot");
    let binding = ProductInteractionBinding::new(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            1,
            Vec::new(),
            None,
        )
        .expect("interaction"),
        None,
    )
    .expect("binding");
    let pending = PendingRequest::ChatBinding { run_id: run, opening: true };
    model.chat.run_id = Some(run);
    model.chat.buffer = "Retain this exact draft".to_owned();

    assert!(model.accept_chat_binding(&binding, Some(&pending)).is_empty());
    assert!(model.chat.run_id.is_none());
    assert!(model.chat.workbench.selected.is_none());
    assert!(model.chat.snapshot.is_none());
    assert_eq!(model.chat.buffer, "Retain this exact draft");
    assert!(model.notice.as_ref().expect("notice").text.contains("press Enter"));
    let effects = key(&mut model, KeyCode::Enter);
    assert!(matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(request))]
        if matches!(request.payload(), AppRequestPayload::WorkbenchCommand(command)
            if matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::CreateConversation(_)))));
}
