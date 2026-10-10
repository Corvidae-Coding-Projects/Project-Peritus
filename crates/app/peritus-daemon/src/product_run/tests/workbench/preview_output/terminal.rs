//! Real C4 preview attachment, reconnect, input, resize, and exact terminal settlement.

use super::*;
use crate::terminal::{TerminalBridgeEvent, TerminalRegistry, TerminalRegistryLimits};
use peritus_app_protocol::{
    AppRequestPayload, RequestId, TerminalAttachmentId, TerminalBinding, TerminalInput,
    TerminalOutput, TerminalResize, TerminalStream,
};
use peritus_types::SessionId;

pub(super) struct Attached {
    registry: TerminalRegistry,
    binding: TerminalBinding,
    session: SessionId,
}

pub(super) async fn attach_and_reconnect(
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
    wait_for_output_text(
        || registry.poll(actor(), session, binding).expect("poll terminal output"),
        "NAME?",
    )
    .await;
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
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            let mut output = Vec::new();
            let mut exits = 0;
            loop {
                for event in self.registry.poll(actor(), self.session, self.binding).expect("poll")
                {
                    match event {
                        TerminalBridgeEvent::Output(chunk) => {
                            output.extend_from_slice(chunk.bytes());
                        }
                        TerminalBridgeEvent::Exited(_) => exits += 1,
                    }
                }
                if exits > 0 {
                    return (output, exits);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("terminal did not exit before deadline"));
        let (output, exits) = result;
        assert_eq!(exits, 1, "terminal exited more than once");
        assert!(String::from_utf8_lossy(&output).contains("HELLO Ada"));
        assert!(self.registry.retire(self.binding.process_id()).expect("retire C4 lease"));
        assert_eq!(self.registry.counts(), (0, 0));
    }
}

async fn wait_for_output_text(mut poll: impl FnMut() -> Vec<TerminalBridgeEvent>, expected: &str) {
    let mut observed = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let mut exited = false;
            for event in poll() {
                match event {
                    TerminalBridgeEvent::Output(chunk) => observed.extend_from_slice(chunk.bytes()),
                    TerminalBridgeEvent::Exited(_) => exited = true,
                }
            }
            if contains_bytes(&observed, expected.as_bytes()) {
                return Ok(());
            }
            if exited {
                return Err(format!(
                    "terminal exited before emitting {expected:?}; {}",
                    output_summary(&observed)
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("{error}"),
        Err(error) => panic!(
            "timed out waiting for terminal text {expected:?} ({error}); {}",
            output_summary(&observed)
        ),
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

fn output_summary(observed: &[u8]) -> String {
    const TAIL_BYTES: usize = 256;
    let tail = &observed[observed.len().saturating_sub(TAIL_BYTES)..];
    format!(
        "observed {} bytes; final {tail_len} bytes: {:?}",
        observed.len(),
        String::from_utf8_lossy(tail),
        tail_len = tail.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn prompt_wait_retries_empty_pages_and_accumulates_split_chunks() {
        interaction::block_on(async {
            let binding = TerminalBinding::new(
                TerminalAttachmentId::new([1; 16]).expect("attachment"),
                peritus_types::ProcessId::new([2; 16]).expect("process"),
                RequestId::new([3; 16]).expect("request"),
            );
            let output = |sequence, offset, bytes: &[u8]| {
                TerminalBridgeEvent::Output(
                    TerminalOutput::new(
                        binding,
                        sequence,
                        offset,
                        TerminalStream::Terminal,
                        bytes.to_vec(),
                        64,
                    )
                    .expect("output chunk"),
                )
            };
            let mut pages = VecDeque::from([
                Vec::new(),
                vec![output(1, 0, b"prefix NA")],
                Vec::new(),
                vec![output(2, 9, b"ME? suffix")],
            ]);
            let mut polls = 0;
            wait_for_output_text(
                || {
                    polls += 1;
                    pages.pop_front().unwrap_or_default()
                },
                "NAME?",
            )
            .await;
            assert_eq!(polls, 4, "poll through empty pages and a split prompt");
        });
    }
}
