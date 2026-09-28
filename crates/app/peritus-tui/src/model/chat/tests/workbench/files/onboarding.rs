use super::*;

fn fresh(command: &str) -> AppModel {
    let mut model = opened();
    model.select_workbench_conversation(None);
    model.chat.workbench.open = false;
    model.chat.buffer = command.to_owned();
    model
}

fn create_attachment(model: &mut AppModel) -> (AppRequestEnvelope, WorkbenchCommand) {
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("metadata creation")
    };
    assert!(matches!(command.intent(), WorkbenchIntent::CreateConversation(_)));
    (sent.clone(), command.clone())
}

fn snapshot(command: &WorkbenchCommand) -> AppResponsePayload {
    AppResponsePayload::Workbench(
        WorkbenchSnapshot::new(
            command.query(),
            1,
            ConversationTitle::new("File conversation".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("metadata"),
    )
}

#[test]
fn first_file_command_and_at_path_create_one_session_but_wait_for_explicit_preview() {
    for draft in ["/files src/reference.txt", "@src/reference.txt"] {
        let mut model = fresh(draft);
        let (sent, command) = create_attachment(&mut model);
        assert_eq!(model.chat.buffer, draft);
        let refresh = request(&respond(&mut model, &sent, receipt(&command)));
        assert_eq!(refresh.payload(), &AppRequestPayload::QueryWorkbench(command.query()));
        assert!(respond(&mut model, &refresh, snapshot(&command)).is_empty());
        assert!(model.chat.workbench.files.open);
        assert_eq!(model.chat.workbench.files.path, "src/reference.txt");
        assert!(model.chat.buffer.is_empty());
        assert!(model.chat.run_id.is_none());
        let sent = request(&key(&mut model, KeyCode::Char('p')));
        assert_eq!(sent.payload(), &AppRequestPayload::QueryWorkbench(command.query()));
        let preview = request(&respond(&mut model, &sent, snapshot(&command)));
        assert!(matches!(preview.payload(), AppRequestPayload::PreviewWorkbenchFile(_)));
    }
}

#[test]
fn escaped_or_edited_initial_attachment_does_not_reopen_the_file_panel() {
    for escape in [false, true] {
        let mut model = fresh("@src/reference.txt");
        let (sent, command) = create_attachment(&mut model);
        if escape {
            key(&mut model, KeyCode::Esc);
        }
        model.chat.buffer = "a different idea".to_owned();
        let refresh = request(&respond(&mut model, &sent, receipt(&command)));
        assert!(respond(&mut model, &refresh, snapshot(&command)).is_empty());
        assert!(!model.chat.workbench.files.open);
        assert_eq!(model.chat.buffer, "a different idea");
        assert!(!model.workbench_request_pending());
    }
}

#[test]
fn unavailable_file_feature_does_not_create_a_session() {
    let mut model = fresh("@src/reference.txt");
    model
        .features
        .retain(|feature| feature.as_str() != WellKnownProtocolFeature::WorkbenchFiles.as_str());
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.workbench.selected.is_none());
    assert_eq!(model.chat.buffer, "@src/reference.txt");
}
