//! Real C4 preview attachment, reconnect, input, resize, and exact terminal settlement.

use super::*;
use crate::terminal::{TerminalBridgeEvent, TerminalRegistry, TerminalRegistryLimits};
use peritus_app_protocol::{
    AppRequestPayload, RequestId, TerminalAttachmentId, TerminalBinding, TerminalInput,
    TerminalResize,
};
use peritus_types::SessionId;

pub(super) struct Attached {
    registry: TerminalRegistry,
    binding: TerminalBinding,
    session: SessionId,
}

pub(super) fn attach_and_reconnect(
    service: &ProductRunService,
    live: &WorkbenchPreviewSnapshot,
) -> Attached {
    let process = live.result().launches()[0].process().expect("process");
    let registry = TerminalRegistry::new(TerminalRegistryLimits::PRODUCTION).expect("registry");
    let session = SessionId::new([88; 16]).expect("session");
    assert!(
        service
            .register_preview_terminal(
                ActorId::new([99; 16]).expect("stranger"),
                session,
                process,
                &registry
            )
            .is_err()
    );
    service
        .register_preview_terminal(actor(), session, process, &registry)
        .expect("register preview");
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([88; 16]).expect("attachment"),
        process,
        RequestId::new([88; 16]).expect("request"),
    );
    registry.attach(actor(), session, binding, 8192).expect("attach");
    let output = registry.poll(actor(), session, binding).expect("prompt");
    assert!(output.iter().any(|event| matches!(event, TerminalBridgeEvent::Output(output) if String::from_utf8_lossy(output.bytes()).contains("NAME?"))));
    registry.release_attachments(actor(), session, &[binding]);
    let session = SessionId::new([89; 16]).expect("reconnected session");
    service
        .register_preview_terminal(actor(), session, process, &registry)
        .expect("reconnect registration");
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([89; 16]).expect("attachment"),
        process,
        RequestId::new([89; 16]).expect("request"),
    );
    registry.attach(actor(), session, binding, 8192).expect("reattach");
    let resized = registry.resize(
        actor(),
        session,
        TerminalResize::new(binding, 100, 30, u16::MAX, u16::MAX).expect("size"),
    );
    if registry.uses_pipes(actor(), session, process).unwrap() {
        assert!(resized.is_err(), "pipe attachment must not claim PTY resize");
    } else {
        resized.expect("resize PTY");
    }
    service.authorize_preview_terminal_input(actor(), process).expect("input authority");
    assert!(
        service
            .authorize_preview_terminal_input(ActorId::new([99; 16]).expect("stranger"), process)
            .is_err()
    );
    registry
        .input(actor(), session, &TerminalInput::new(binding, b"A".to_vec(), 8192).expect("input"))
        .expect("terminal input");
    Attached { registry, binding, session }
}

impl Attached {
    pub(super) async fn check_permission_changes(
        &self,
        service: &ProductRunService,
        workspace: WorkspaceId,
    ) {
        for (index, allowed) in [(0_u8, false), (1, true)] {
            let AppResponsePayload::Workbench(scope) =
                service.workbench_query(actor(), query(workspace))
            else {
                panic!("scope")
            };
            let change = command(
                workspace,
                90 + index,
                scope.revision(),
                WorkbenchIntent::SetPermissions(
                    peritus_app_protocol::WorkbenchPermissionChange::new(
                        u64::from(index),
                        peritus_app_protocol::WorkbenchPermissionCapability::Write,
                        allowed,
                    ),
                ),
            );
            let response = service.workbench_command(actor(), &change).await;
            assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)), "{response:?}");
            let input = AppRequestPayload::TerminalInput(
                TerminalInput::new(self.binding, b"X".to_vec(), 8192).expect("input"),
            );
            assert_eq!(service.authorize_workbench_request(actor(), &input).is_ok(), allowed);
        }
    }

    pub(super) async fn finish(self) {
        let mut output = Vec::new();
        let mut exits = 0;
        for _ in 0..100 {
            for event in self.registry.poll(actor(), self.session, self.binding).expect("poll") {
                match event {
                    TerminalBridgeEvent::Output(chunk) => output.extend_from_slice(chunk.bytes()),
                    TerminalBridgeEvent::Exited(_) => exits += 1,
                }
            }
            if exits > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(exits, 1);
        assert!(String::from_utf8_lossy(&output).contains("HELLO Ada"));
        assert!(self.registry.retire(self.binding.process_id()).expect("retire C4 lease"));
        assert_eq!(self.registry.counts(), (0, 0));
    }
}
