use super::*;
use peritus_app_protocol::{AppErrorCode, AppProtocolError};

#[test]
fn denied_control_resolves_its_receipt_then_allows_recovery_without_losing_the_draft() {
    for code in
        [AppErrorCode::ReadOnly, AppErrorCode::InvalidIdentifier, AppErrorCode::LimitExceeded]
    {
        let mut model = enabled_model();
        let (sent, command) = create(&mut model);
        let draft = model.chat.buffer.clone();
        let lookup = request(&respond(
            &mut model,
            &sent,
            AppResponsePayload::Error(AppProtocolError::new(code, None)),
        ));
        assert_eq!(lookup.payload(), &AppRequestPayload::QueryWorkbenchReceipt(command));
        assert!(model.chat.workbench.unresolved.is_some());
        respond(
            &mut model,
            &lookup,
            AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::InvalidIdentifier, None)),
        );
        assert!(model.chat.workbench.unresolved.is_none());
        assert_eq!(model.chat.buffer, draft);
        key(&mut model, KeyCode::Esc);
        assert!(matches!(create(&mut model).0.payload(), AppRequestPayload::WorkbenchCommand(_)));
    }
}

#[test]
fn denial_during_retry_cannot_forget_an_earlier_durable_acceptance() {
    let mut model = enabled_model();
    let (sent, command) = create(&mut model);
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Backpressure, None)),
    );
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/sessions retry".to_owned();
    let retry = request(&key(&mut model, KeyCode::Enter));
    let lookup = request(&respond(
        &mut model,
        &retry,
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::ReadOnly, None)),
    ));
    model.chat.buffer = "Changed my mind while reconnecting".to_owned();
    respond(&mut model, &lookup, receipt(&command));
    assert!(model.chat.workbench.unresolved.is_none());
    assert_eq!(model.chat.workbench.selected, Some(command.query()));
    assert_eq!(model.chat.buffer, "Changed my mind while reconnecting");
}
