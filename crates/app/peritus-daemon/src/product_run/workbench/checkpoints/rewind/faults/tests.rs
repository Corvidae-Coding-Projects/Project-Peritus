//! Identical operations in separate checkpoint owners never share injected failures.

use super::*;
use peritus_app_protocol::{
    ControlOperationId, ConversationId, ConversationTitle, WorkbenchIntent, WorkbenchQuery,
};
use peritus_types::WorkspaceId;

#[test]
fn checkpoint_faults_follow_service_clones_and_expire_with_the_owner() {
    let workspace = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let other_state = tempfile::tempdir().unwrap();
    let workspace_id = WorkspaceId::new([4; 16]).unwrap();
    let make_service = |root: &std::path::Path| {
        crate::product_run::tests::checkpoint_test_service(root, workspace.path(), workspace_id)
    };
    let command = WorkbenchCommand::new(
        ControlOperationId::new([12; 16]).unwrap(),
        WorkbenchQuery::new(ConversationId::new([2; 16]).unwrap(), workspace_id),
        0,
        WorkbenchIntent::CreateConversation(ConversationTitle::new("owner".to_owned()).unwrap()),
    );
    let service = make_service(state.path());
    let other = make_service(other_state.path());

    for point in [
        RewindFaultPoint::AfterPrepare,
        RewindFaultPoint::AfterFolderPatch,
        RewindFaultPoint::InsideFolderPatch,
    ] {
        service.inject_rewind_fault(command.operation().into_bytes(), point);
        assert!(other.check_rewind_fault(&command, point).is_ok());
        other.inject_rewind_fault(command.operation().into_bytes(), point);
        std::thread::scope(|scope| {
            let cloned = service.clone();
            let command = &command;
            assert!(
                scope
                    .spawn(move || cloned.check_rewind_fault(command, point))
                    .join()
                    .unwrap()
                    .is_err()
            );
        });
        assert!(service.check_rewind_fault(&command, point).is_ok(), "consumed once");
        assert!(
            other.check_rewind_fault(&command, point).is_err(),
            "other owner retained its fault"
        );
        assert!(other.check_rewind_fault(&command, point).is_ok());
    }

    service.inject_rewind_fault(command.operation().into_bytes(), RewindFaultPoint::AfterPrepare);
    drop(service);
    let reopened = make_service(state.path());
    assert!(reopened.check_rewind_fault(&command, RewindFaultPoint::AfterPrepare).is_ok());
}
