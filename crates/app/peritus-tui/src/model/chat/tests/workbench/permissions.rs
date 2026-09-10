use super::*;
use peritus_app_protocol::{
    ProtocolFeatureName, WellKnownProtocolFeature, WorkbenchPermissionCapability as Capability,
    WorkbenchPermissionEntry, WorkbenchPermissionProvenance as Provenance, WorkbenchPermissions,
    WorkbenchWorkspaceTrust,
};

fn permissions_model() -> AppModel {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchPermissions)
            .expect("feature"),
    );
    model
}

fn projection(command: &WorkbenchCommand) -> WorkbenchPermissions {
    let rows = [
        WorkbenchPermissionEntry::new(
            Capability::Read,
            true,
            true,
            Provenance::WorkspaceHostPolicy,
            false,
        )
        .expect("read"),
        WorkbenchPermissionEntry::new(
            Capability::Write,
            true,
            true,
            Provenance::WorkspaceHostPolicy,
            true,
        )
        .expect("write"),
        WorkbenchPermissionEntry::new(
            Capability::Process,
            true,
            true,
            Provenance::ToolHostPolicy,
            true,
        )
        .expect("process"),
        WorkbenchPermissionEntry::new(
            Capability::Network,
            false,
            false,
            Provenance::ProviderHostPolicy,
            false,
        )
        .expect("network"),
    ];
    WorkbenchPermissions::new(command.query(), 1, 5, WorkbenchWorkspaceTrust::Managed, rows)
        .expect("permissions")
}

#[test]
fn permissions_require_inspection_and_never_broaden_the_host_ceiling() {
    let mut model = permissions_model();
    let (created, command) = create(&mut model);
    let query_request = request(&respond(&mut model, &created, receipt(&command)));
    let snapshot = WorkbenchSnapshot::new(
        command.query(),
        1,
        ConversationTitle::new("Private fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot");
    respond(&mut model, &query_request, AppResponsePayload::Workbench(snapshot));

    let inspect = request(&model.slash_command("/permissions"));
    assert_eq!(inspect.payload(), &AppRequestPayload::QueryWorkbenchPermissions(command.query()));
    respond(&mut model, &inspect, AppResponsePayload::WorkbenchPermissions(projection(&command)));

    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/permissions grant network".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, "/permissions grant network");

    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/permissions restrict write".to_owned();
    let restrict = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(restriction) = restrict.payload() else {
        panic!("permission command")
    };
    assert_eq!(restriction.expected_revision(), 1);
    let peritus_app_protocol::WorkbenchIntent::SetPermissions(change) = restriction.intent() else {
        panic!("permission intent")
    };
    assert_eq!(change.expected_authority_revision(), 5);
    assert_eq!(change.capability(), Capability::Write);
    assert!(!change.allowed());
    assert!(!model.chat.buffer.is_empty(), "receipt is required before clearing the draft");

    let refreshed = request(&respond(&mut model, &restrict, receipt(restriction)));
    assert_eq!(refreshed.payload(), &AppRequestPayload::QueryWorkbenchPermissions(command.query()));
    assert!(model.chat.buffer.is_empty());
}
