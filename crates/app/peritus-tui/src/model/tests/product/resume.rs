use super::super::{AppModel, context};
use crate::{
    action::{Action, Effect},
    runtime::ProductLaunchContext,
};
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
    AppResponsePayload, ConversationId, ConversationLibraryItem, ConversationLibraryPage,
    ConversationTitle, ProtocolFeatureName, WellKnownProtocolFeature, WorkbenchExecutionState,
    WorkbenchQuery, WorkbenchSnapshot,
};
use peritus_types::WorkspaceId;

fn model() -> AppModel {
    let launch = ProductLaunchContext::new(
        WorkspaceId::new([0x91; 16]).unwrap(),
        "/managed/project".to_owned(),
        Vec::new(),
        None,
    )
    .unwrap()
    .with_latest_conversation();
    let mut model = AppModel::with_product([0x92; 32], Some(launch));
    let _ = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    model
}

fn features() -> Vec<ProtocolFeatureName> {
    [
        WellKnownProtocolFeature::WorkbenchControl,
        WellKnownProtocolFeature::WorkbenchInputs,
        WellKnownProtocolFeature::WorkbenchExecution,
        WellKnownProtocolFeature::WorkbenchConversation,
        WellKnownProtocolFeature::WorkbenchContinuationReceipts,
        WellKnownProtocolFeature::ConversationLibrary,
    ]
    .into_iter()
    .map(|feature| ProtocolFeatureName::well_known(feature).unwrap())
    .collect()
}

fn request(effects: &[Effect]) -> AppRequestEnvelope {
    let [Effect::Send(AppMessage::Request(request))] = effects else {
        panic!("expected one request, got {effects:?}")
    };
    request.clone()
}

fn respond(
    model: &mut AppModel,
    request: &AppRequestEnvelope,
    payload: AppResponsePayload,
) -> Vec<Effect> {
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        payload,
    ))))
}

fn item(query: WorkbenchQuery, activity: u64, pinned: bool) -> ConversationLibraryItem {
    ConversationLibraryItem::new(
        query,
        ConversationTitle::new(format!("Conversation {activity}")).unwrap(),
        pinned,
        false,
        activity,
        None,
        false,
        String::new(),
        None,
        None,
    )
    .unwrap()
}

#[test]
fn resume_scans_every_page_and_opens_highest_workspace_activity_without_starting_work() {
    let mut model = model();
    let first_request = request(
        &model.update(Action::NegotiatedFeatures { context: context(), features: features() }),
    );
    let AppRequestPayload::QueryConversationLibrary(first_query) = first_request.payload() else {
        panic!("latest conversation query")
    };
    assert_eq!(first_query.offset(), 0);
    assert_eq!(first_query.limit(), 64);
    assert!(first_query.include_archived());

    let workspace = first_query.workspace();
    let pinned = (1_u8..=64)
        .map(|value| {
            item(
                WorkbenchQuery::new(ConversationId::new([value; 16]).unwrap(), workspace),
                u64::from(value),
                true,
            )
        })
        .collect();
    let first_page =
        ConversationLibraryPage::new(first_query.clone(), 65, Some(64), pinned).unwrap();
    let second_request = request(&respond(
        &mut model,
        &first_request,
        AppResponsePayload::ConversationLibrary(first_page),
    ));
    let AppRequestPayload::QueryConversationLibrary(second_query) = second_request.payload() else {
        panic!("second latest conversation page")
    };
    assert_eq!(second_query.offset(), 64);
    assert!(model.chat.workbench.selected.is_none());

    let newest = WorkbenchQuery::new(ConversationId::new([0x95; 16]).unwrap(), workspace);
    let second_page =
        ConversationLibraryPage::new(second_query.clone(), 65, None, vec![item(newest, 65, false)])
            .unwrap();
    let opening = request(&respond(
        &mut model,
        &second_request,
        AppResponsePayload::ConversationLibrary(second_page),
    ));
    assert_eq!(opening.payload(), &AppRequestPayload::QueryWorkbenchExecution(newest));
    assert_eq!(model.chat.workbench.selected, Some(newest));
    assert!(model.chat.run_id.is_none());

    let snapshot = WorkbenchSnapshot::new(
        newest,
        12,
        ConversationTitle::new("Most recent".to_owned()).unwrap(),
        false,
        false,
    )
    .unwrap();
    assert!(
        respond(
            &mut model,
            &opening,
            AppResponsePayload::WorkbenchExecution(
                WorkbenchExecutionState::new(snapshot.clone(), None, false).unwrap(),
            ),
        )
        .is_empty()
    );
    assert_eq!(model.chat.workbench.snapshot, Some(snapshot));
    assert!(model.chat.run_id.is_none(), "opening history must not start execution");
    assert!(
        model
            .update(Action::NegotiatedFeatures { context: context(), features: features() })
            .is_empty(),
        "the one-shot selection must not repeat after reconnect"
    );
}

#[test]
fn resume_with_no_saved_conversation_leaves_a_visible_new_conversation() {
    let mut model = model();
    let sent = request(
        &model.update(Action::NegotiatedFeatures { context: context(), features: features() }),
    );
    let AppRequestPayload::QueryConversationLibrary(query) = sent.payload() else {
        panic!("latest conversation query")
    };
    let page = ConversationLibraryPage::new(query.clone(), 0, None, Vec::new()).unwrap();
    assert!(respond(&mut model, &sent, AppResponsePayload::ConversationLibrary(page)).is_empty());
    assert!(model.chat.workbench.selected.is_none());
    assert!(model.chat.run_id.is_none());
    assert!(model.notice.as_ref().is_some_and(|notice| {
        notice.text == "No saved conversation exists for this folder. Started a new conversation."
    }));
    assert!(
        model
            .update(Action::NegotiatedFeatures { context: context(), features: features() })
            .is_empty()
    );
}

#[test]
fn rejected_resume_lookup_is_visible_and_does_not_repeat() {
    let mut model = model();
    let sent = request(
        &model.update(Action::NegotiatedFeatures { context: context(), features: features() }),
    );
    let error = peritus_app_protocol::AppProtocolError::new(
        peritus_app_protocol::AppErrorCode::Internal,
        None,
    );
    assert!(respond(&mut model, &sent, AppResponsePayload::Error(error)).is_empty());
    assert!(model.notice.as_ref().is_some_and(|notice| {
        notice.text.contains("Could not find this folder's most recent conversation")
            && notice.text.contains("run peritus resume again")
    }));
    assert!(
        model
            .update(Action::NegotiatedFeatures { context: context(), features: features() })
            .is_empty()
    );
}

#[test]
fn resume_requires_both_library_and_workbench_support_before_querying() {
    let mut model = model();
    let library =
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationLibrary).unwrap();
    assert!(
        model
            .update(Action::NegotiatedFeatures { context: context(), features: vec![library] })
            .is_empty()
    );
    assert!(model.notice.as_ref().is_some_and(|notice| {
        notice.text.contains("cannot find the latest saved conversation")
    }));
    assert!(
        model
            .update(Action::NegotiatedFeatures { context: context(), features: features() })
            .is_empty(),
        "an unsupported one-shot launch must not run later against a different connection"
    );
}
