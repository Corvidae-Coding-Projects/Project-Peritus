use super::*;
use peritus_app_protocol::{
    AppResponseEnvelope, AppResponsePayload, DoctorFinding, DoctorReport, DoctorStatus,
    ProtocolFeatureName, WellKnownProtocolFeature,
};

fn enabled_model() -> AppModel {
    let mut model = model();
    model.features = vec![
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ProductDiagnostics)
            .expect("feature"),
    ];
    model
}

#[test]
fn diagnostic_capability_absence_and_invalid_arguments_retain_drafts_without_effects() {
    let mut model = model();
    for command in ["/doctor", "/doctor repair", "/doctor --upload"] {
        model.chat.buffer = command.to_owned();
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        assert_eq!(model.chat.buffer, command);
        assert!(model.chat.doctor.is_none());
    }
}

#[test]
fn diagnostic_report_is_scoped_correlated_and_preserves_a_newer_draft() {
    let mut model = enabled_model();
    model.chat.buffer = "/doctor".to_owned();
    let effects = key(&mut model, KeyCode::Enter);
    let [Effect::Send(AppMessage::Request(request))] = effects.as_slice() else {
        panic!("one typed request")
    };
    let AppRequestPayload::Doctor(query) = request.payload() else { panic!("diagnostic query") };
    assert_eq!(query.workspace(), WorkspaceId::new([4; 16]).expect("workspace"));
    assert!(model.chat.doctor.is_some());
    assert!(model.chat.run_id.is_none(), "diagnostics must not create a conversation/run");
    assert!(key(&mut model, KeyCode::Char('r')).is_empty(), "no duplicate pending probe");
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "preserve this new draft".to_owned();
    let report = DoctorReport::new(
        *query,
        vec![
            DoctorFinding::new(
                "provider-authentication".to_owned(),
                DoctorStatus::Unsupported,
                "Not probed".to_owned(),
                String::new(),
            )
            .expect("finding"),
        ],
    )
    .expect("report");
    let response = AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::Doctor(report),
    ));
    model.update(Action::Message(response));
    assert!(model.chat.doctor.is_none(), "late reply must not reopen a closed panel");
    assert_eq!(model.chat.buffer, "preserve this new draft");
    assert!(model.chat.run_id.is_none());
}

#[test]
fn panel_navigation_leaves_composer_and_transcript_scroll_unchanged() {
    let mut model = enabled_model();
    model.chat.buffer = "retained task draft".to_owned();
    model.chat.cursor = 3;
    model.chat.scroll = 7;
    assert_eq!(model.slash_command("/doctor").len(), 1);
    key(&mut model, KeyCode::PageDown);
    assert_eq!(model.chat.doctor.as_ref().expect("panel").scroll, 5);
    key(&mut model, KeyCode::Char('x'));
    key(&mut model, KeyCode::Esc);
    assert_eq!(model.chat.buffer, "retained task draft");
    assert_eq!(model.chat.cursor, 3);
    assert_eq!(model.chat.scroll, 7);
}

#[test]
fn diagnostic_layout_keeps_return_and_composer_visible_at_supported_sizes() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = enabled_model();
    model.chat.buffer = "retained draft λ".to_owned();
    model.slash_command("/doctor");
    let query = model.chat.doctor.as_ref().expect("panel").query;
    let report = DoctorReport::new(
        query,
        vec![
            DoctorFinding::new(
                "provider-authentication".to_owned(),
                DoctorStatus::Unsupported,
                "Not probed; no network request was made.".to_owned(),
                "Configure authentication explicitly if required.".to_owned(),
            )
            .expect("finding"),
        ],
    )
    .expect("report");
    model.accept_doctor(query, report);
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Doctor", "Esc back", "Message Peritus", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height} missing {expected}: {text}");
        }
    }
}
