//! Setup controls create metadata, then resume the exact inspected command without inference.
use super::*;

const CASES: &[(&str, WellKnownProtocolFeature)] = &[
    ("/permissions", WellKnownProtocolFeature::WorkbenchPermissions),
    ("/init", WellKnownProtocolFeature::WorkbenchInit),
    ("/memory", WellKnownProtocolFeature::WorkbenchMemory),
    ("/brief", WellKnownProtocolFeature::WorkbenchBrief),
    ("/queue", WellKnownProtocolFeature::WorkbenchInputs),
    ("/context", WellKnownProtocolFeature::WorkbenchContext),
    ("/goal Inspect the current project", WellKnownProtocolFeature::WorkbenchGoals),
];

fn ready(command: &str, feature: WellKnownProtocolFeature) -> AppModel {
    let mut model = enabled_model();
    for feature in [feature, WellKnownProtocolFeature::WorkbenchBrief] {
        model.features.push(ProtocolFeatureName::well_known(feature).expect("feature"));
    }
    model.chat.buffer = command.to_owned();
    model
}

fn start(model: &mut AppModel) -> (AppRequestEnvelope, WorkbenchCommand) {
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("metadata creation")
    };
    assert!(matches!(
        command.intent(),
        peritus_app_protocol::WorkbenchIntent::CreateConversation(_)
    ));
    (sent.clone(), command.clone())
}

fn metadata(command: &WorkbenchCommand) -> AppResponsePayload {
    AppResponsePayload::Workbench(
        WorkbenchSnapshot::new(
            command.query(),
            1,
            ConversationTitle::new("New conversation".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("metadata"),
    )
}

#[test]
fn initial_setup_controls_continue_into_inspection_without_execution_or_authority_changes() {
    for &(draft, feature) in CASES {
        let mut model = ready(draft, feature);
        let (sent, command) = start(&mut model);
        let refresh = request(&respond(&mut model, &sent, receipt(&command)));
        assert_eq!(refresh.payload(), &AppRequestPayload::QueryWorkbench(command.query()));
        assert_eq!(model.chat.buffer, draft);
        let inspect = request(&respond(&mut model, &refresh, metadata(&command)));
        assert!(
            matches!(
                inspect.payload(),
                AppRequestPayload::QueryWorkbenchPermissions(_)
                    | AppRequestPayload::QueryWorkbench(_)
                    | AppRequestPayload::QueryWorkbenchBrief(_)
                    | AppRequestPayload::QueryWorkbenchQueue(_)
                    | AppRequestPayload::QueryWorkbenchContext(_)
            ),
            "{draft}: {inspect:?}"
        );
        assert!(model.chat.run_id.is_none());
        assert!(!model.chat.workbench.message.contains("Creating a conversation"));
        assert!(model.chat.workbench.unresolved.is_none());
        if draft.starts_with("/goal") {
            assert_eq!(
                model.chat.workbench.goal_draft.as_ref().expect("local draft").objective().as_str(),
                "Inspect the current project"
            );
            assert!(model.chat.workbench.goal.is_none(), "confirmation remains explicit");
        }
    }
}

#[test]
fn escape_before_creation_receipt_preserves_unchanged_setup_draft_and_abandons_continuation() {
    for &(draft, feature) in CASES {
        let mut model = ready(draft, feature);
        let (sent, command) = start(&mut model);
        key(&mut model, KeyCode::Esc);
        let refresh = request(&respond(&mut model, &sent, receipt(&command)));
        assert!(respond(&mut model, &refresh, metadata(&command)).is_empty());
        assert_eq!(model.chat.buffer, draft);
        assert!(!model.chat.workbench.open);
        assert!(model.chat.workbench.goal_draft.is_none());
        assert!(!model.workbench_request_pending());
    }
}

#[test]
fn unsupported_initial_setup_controls_never_create_conversations() {
    for &(draft, _) in CASES {
        let mut model = enabled_model();
        model.chat.buffer = draft.to_owned();
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        assert!(model.chat.workbench.selected.is_none());
        assert_eq!(model.chat.buffer, draft);
    }
}

#[test]
fn resumed_inspection_clears_only_its_accepted_composer_command() {
    for changed_draft in [false, true] {
        let mut model = ready("/init", WellKnownProtocolFeature::WorkbenchInit);
        let (sent, command) = start(&mut model);
        let refresh = request(&respond(&mut model, &sent, receipt(&command)));
        let inspect = request(&respond(&mut model, &refresh, metadata(&command)));
        if changed_draft {
            model.chat.buffer = "my next question".to_owned();
        }
        respond(&mut model, &inspect, metadata(&command));
        assert_eq!(model.chat.buffer, if changed_draft { "my next question" } else { "" });
    }
}
