use super::*;

#[test]
fn pasted_command_is_not_executed_until_explicit_catalog_selection() {
    for command in ["/quit", "/stop", "/discard", "/runs", "/plan inspect"] {
        let mut model = model();
        let effects = model.update(Action::TerminalEvent(Event::Paste(command.to_owned())));
        assert!(effects.is_empty());
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        assert_eq!(model.chat.buffer, command);
        assert!(!model.quitting);
        assert_eq!(model.chat.mode, ProductInteractionMode::Chat);
        assert_eq!(model.view, View::Conversation);
    }
    let mut model = model();
    model.update(Action::TerminalEvent(Event::Paste("/runs".to_owned())));
    key(&mut model, KeyCode::Tab);
    key(&mut model, KeyCode::Enter);
    assert_eq!(model.view, View::Runs);
}

#[test]
fn keyboard_command_accepts_pasted_arguments_but_not_pasted_token_edits() {
    let mut model = model();
    for character in "/plan ".chars() {
        key(&mut model, KeyCode::Char(character));
    }
    model.update(Action::TerminalEvent(Event::Paste("inspect only".to_owned())));
    let effects = key(&mut model, KeyCode::Enter);
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::Send(AppMessage::Request(request)) if matches!(request.payload(),
            AppRequestPayload::Interact(value) if value.mode() == ProductInteractionMode::Plan))));

    let mut model = super::model();
    key(&mut model, KeyCode::Char('/'));
    model.update(Action::TerminalEvent(Event::Paste("quit".to_owned())));
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(!model.quitting);
}
