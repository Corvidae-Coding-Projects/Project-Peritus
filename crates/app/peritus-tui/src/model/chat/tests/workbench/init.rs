use super::*;
use peritus_app_protocol::{
    InitDiscoveryRequest, InitProposal, ProtocolFeatureName, WellKnownProtocolFeature,
    WorkbenchIntent,
};

fn init_model() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchInit).expect("feature"),
    );
    model
}

fn proposal(request: InitDiscoveryRequest) -> InitProposal {
    InitProposal::from_discovery(
        request,
        peritus_types::Sha256Digest::new([0x91; 32]),
        Vec::new(),
        None,
        Vec::new(),
    )
    .expect("proposal")
}

fn open_conversation(model: &mut AppModel) -> WorkbenchCommand {
    let (created, command) = create(model);
    let query_request = request(&respond(model, &created, receipt(&command)));
    let snapshot = WorkbenchSnapshot::new(
        command.query(),
        1,
        ConversationTitle::new("Init fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot");
    respond(model, &query_request, AppResponsePayload::Workbench(snapshot));
    command
}

fn discover_request(
    model: &mut AppModel,
    query: peritus_app_protocol::WorkbenchQuery,
    revision: u64,
) -> AppRequestEnvelope {
    let snapshot_request = request(&model.slash_command("/init"));
    assert_eq!(snapshot_request.payload(), &AppRequestPayload::QueryWorkbench(query));
    let snapshot = WorkbenchSnapshot::new(
        query,
        revision,
        ConversationTitle::new("Init fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot");
    request(&respond(model, &snapshot_request, AppResponsePayload::Workbench(snapshot)))
}

#[test]
fn init_decline_is_local_and_apply_submits_only_the_inspected_exact_proposal() {
    let mut model = init_model();
    let command = open_conversation(&mut model);

    let discover = discover_request(&mut model, command.query(), 1);
    let AppRequestPayload::DiscoverInit(discovery) = discover.payload() else {
        panic!("discovery")
    };
    assert_eq!(discovery.query(), command.query());
    assert_eq!(discovery.revision(), 1);
    let first = proposal(*discovery);
    respond(&mut model, &discover, AppResponsePayload::InitProposal(first));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/init decline".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.buffer.is_empty());
    assert!(model.chat.workbench.init.is_none());

    key(&mut model, KeyCode::Esc);
    let discover = discover_request(&mut model, command.query(), 1);
    let AppRequestPayload::DiscoverInit(discovery) = discover.payload() else {
        panic!("discovery")
    };
    let reviewed = proposal(*discovery);
    respond(&mut model, &discover, AppResponsePayload::InitProposal(reviewed.clone()));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/init apply".to_owned();
    let apply = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(apply) = apply.payload() else {
        panic!("apply command")
    };
    assert_eq!(apply.expected_revision(), 1);
    assert!(matches!(
        apply.intent(),
        WorkbenchIntent::ApplyInitDiff(proposal) if proposal == &reviewed
    ));
    assert!(!model.chat.buffer.is_empty(), "durable receipt is still required");
}

#[test]
fn init_refreshes_the_selected_conversation_before_discovery() {
    let mut model = init_model();
    let command = open_conversation(&mut model);

    let discover = discover_request(&mut model, command.query(), 4);
    let AppRequestPayload::DiscoverInit(request) = discover.payload() else {
        panic!("discovery request")
    };
    assert_eq!(request.query(), command.query());
    assert_eq!(request.revision(), 4);
    assert_eq!(model.chat.workbench.snapshot.as_ref().expect("fresh snapshot").revision(), 4);
}

#[test]
fn artifact_init_paginates_review_and_submits_only_its_complete_manifest_reference() {
    use peritus_app_protocol::{InitArtifactPage, InitArtifactProposal, InitContentReference};
    let mut model = init_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchInitArtifacts)
            .expect("feature"),
    );
    let command = open_conversation(&mut model);
    let discover = discover_request(&mut model, command.query(), 1);
    let AppRequestPayload::DiscoverInitArtifacts(discovery) = discover.payload() else {
        panic!("artifact discovery")
    };
    let reference = InitArtifactProposal::new(
        discovery.request(),
        InitContentReference::new(peritus_types::Sha256Digest::new([7; 32]), 100),
        InitContentReference::new(peritus_types::Sha256Digest::new([8; 32]), 70_000),
    );
    let first = request(&respond(
        &mut model,
        &discover,
        AppResponsePayload::InitArtifactProposal(reference),
    ));
    let AppRequestPayload::QueryInitArtifactPage(page_request) = first.payload() else {
        panic!("page request")
    };
    assert_eq!(page_request.offset(), 0);
    respond(
        &mut model,
        &first,
        AppResponsePayload::InitArtifactPage(
            InitArtifactPage::new(*page_request, vec![b'x'; page_request.maximum() as usize])
                .expect("page"),
        ),
    );
    key(&mut model, KeyCode::Esc);
    let second = request(&model.slash_command("/init next"));
    let AppRequestPayload::QueryInitArtifactPage(next_request) = second.payload() else {
        panic!("next page")
    };
    assert_eq!(next_request.offset(), 32 * 1024);
    respond(
        &mut model,
        &second,
        AppResponsePayload::InitArtifactPage(
            InitArtifactPage::new(*next_request, vec![b'y'; next_request.maximum() as usize])
                .expect("page"),
        ),
    );
    key(&mut model, KeyCode::Esc);
    let apply = request(&model.slash_command("/init apply"));
    let AppRequestPayload::WorkbenchCommand(apply) = apply.payload() else { panic!("apply") };
    assert_eq!(apply.intent(), &WorkbenchIntent::ApplyInitArtifact(reference));
    assert_eq!(apply.expected_revision(), 1);
}
