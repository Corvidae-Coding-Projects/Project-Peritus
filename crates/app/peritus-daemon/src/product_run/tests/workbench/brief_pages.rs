//! Real journal publications exercise metadata paging, exact body reads, and revision fencing.
use super::*;
use peritus_app_protocol::{WorkbenchBriefPageRequest, WorkbenchBriefProposalRequest};
use peritus_product_runner::control::{ControlIntent, ControlOperation, InvocationId, OperationId};

#[test]
fn brief_pages_cover_all_large_control_bearing_replies_and_reject_changed_revision() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x21, "chat", Vec::new());
        let reviewer = scripted(0x22, "review", Vec::new());
        let fixer = scripted(0x23, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x24; 16]).expect("workspace");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let text = format!("{}\u{1b}[31mEXACT END\n", "🦉 λ criterion\n".repeat(4000));
        publish_replies(&service, workspace, &text);
        let first_request =
            WorkbenchBriefPageRequest::new(query(workspace), 0, 0, 0).expect("request");
        let AppResponsePayload::WorkbenchBriefPage(first) =
            service.workbench_brief_page(actor(), first_request)
        else {
            panic!("first page")
        };
        assert_eq!(first.proposal_total(), 20);
        assert_eq!(first.proposals().len(), 16);
        let second_request =
            WorkbenchBriefPageRequest::new(query(workspace), first.revision(), 16, 0)
                .expect("next");
        let AppResponsePayload::WorkbenchBriefPage(second) =
            service.workbench_brief_page(actor(), second_request)
        else {
            panic!("second page")
        };
        assert_eq!(second.proposals().len(), 4);
        let reference = second.proposals()[3];
        assert_eq!(reference.bytes(), text.len() as u64);
        let mut assembled = String::new();
        let mut offset = 0;
        loop {
            let request = WorkbenchBriefProposalRequest::new(
                query(workspace),
                first.revision(),
                reference,
                offset,
            )
            .expect("body request");
            let AppResponsePayload::WorkbenchBriefProposal(body) =
                service.workbench_brief_proposal(actor(), request)
            else {
                panic!("body page")
            };
            assembled.push_str(body.text());
            let Some(next) = body.next() else { break };
            offset = next;
        }
        assert_eq!(assembled, text);
        assert_eq!(peritus_codec::sha256(assembled.as_bytes()), reference.digest());
        assert!(matches!(
            service
                .workbench_brief_page(ActorId::new([99; 16]).expect("other actor"), first_request),
            AppResponsePayload::Error(_)
        ));
        let changed = command(
            workspace,
            94,
            first.revision(),
            WorkbenchIntent::SetBrief {
                field: WorkbenchBriefField::Constraints,
                text: WorkbenchInputText::new("User update".into()).expect("text"),
            },
        );
        assert!(matches!(
            service.workbench_command(actor(), &changed).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        assert!(
            matches!(service.workbench_brief_page(actor(), second_request), AppResponsePayload::Error(error) if error.code() == peritus_app_protocol::AppErrorCode::StaleRevision)
        );
        let request =
            WorkbenchBriefProposalRequest::new(query(workspace), first.revision(), reference, 0)
                .expect("stale body request");
        assert!(
            matches!(service.workbench_brief_proposal(actor(), request), AppResponsePayload::Error(error) if error.code() == peritus_app_protocol::AppErrorCode::StaleRevision)
        );
    });
}

fn publish_replies(service: &ProductRunService, workspace: WorkspaceId, text: &str) {
    service
        .with_controls(false, |store| {
            let conversation = peritus_product_runner::control::ConversationId::new(
                *query(workspace).conversation().as_bytes(),
            )?;
            let start = ControlOperation::new(
                OperationId::new([91; 16])?,
                conversation,
                actor(),
                workspace,
                3,
                ControlIntent::StartExecution { run: [92; 16], settings_digest: [93; 32] },
            );
            store.accept(&start)?;
            for index in 100..120 {
                let capture = store.capture_execution(&start)?;
                let request = crate::product_control::test_request(capture.inputs().conversation());
                assert!(matches!(
                    store
                        .prepare_inputs(&capture, InvocationId::new([index; 16])?, &request)?
                        .developer_admission(),
                    peritus_agent::DeveloperRequestAdmission::Accepted
                ));
                store.publish_reply(&start, text)?;
            }
            Ok(())
        })
        .expect("publish exact large replies");
}
