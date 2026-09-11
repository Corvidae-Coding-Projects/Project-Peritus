//! Repair provenance survives the real conversation, tool-admission, and persistence path.

use super::*;
use peritus_app_protocol::{
    ProductActivityKind, ProductInteractionMode, ProductInteractionRequest, ProductRoleModels,
    ProductRunConversationQuery,
};
use peritus_model_protocol::{EventEnvelope, ModelEvent, ProtocolLimits};

fn healed_read(arguments: &str) -> std::collections::VecDeque<EventEnvelope> {
    let events = support::named_tool_response("workspace_read", b"{}".to_vec());
    let events = events.into_iter().flat_map(|event| match event.event() {
        ModelEvent::ToolArgumentDelta { call_id, .. } => {
            peritus_provider_core::healing::tool_arguments(
                arguments.as_bytes(),
                call_id,
                ProtocolLimits::PRODUCTION,
            )
            .unwrap()
        }
        event => vec![event.clone()],
    });
    events
        .enumerate()
        .map(|(index, event)| {
            EventEnvelope::new(
                u64::try_from(index + 1).unwrap(),
                None,
                None,
                peritus_types::Sha256Digest::new([64; 32]),
                event,
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn healed_tool_arguments_keep_schema_validation_and_durable_private_provenance() {
    interaction::block_on(async {
        for (arguments, expected_error) in [
            ("{path:\"src/lib.rs\",start_line:1,end_line:10,}", false),
            ("{start_line:1,end_line:10,}", true),
        ] {
            let repository = repository();
            let before = fs::read(repository.path().join("src/lib.rs")).unwrap();
            let state = tempfile::tempdir().unwrap();
            let writer = scripted(
                0x61,
                "chat",
                vec![healed_read(arguments), support::text_response(b"Finished inspecting.")],
            );
            let reviewer = scripted(0x62, "review", Vec::new());
            let fixer = scripted(0x63, "fix", Vec::new());
            let workspace = WorkspaceId::new([0x64; 16]).unwrap();
            let run = RunId::new([0x65; 16]).unwrap();
            let service =
                service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
            service
                .interact(ProductInteractionRequest::new(
                    ProductRunRequest::new(
                        run,
                        workspace,
                        ProductProviderSelection::new(
                            writer.profile.profile_id(),
                            reviewer.profile.profile_id(),
                            fixer.profile.profile_id(),
                        ),
                        "Read the source; do not change it.".to_owned(),
                    )
                    .unwrap(),
                    ProductInteractionMode::Chat,
                    ProductRoleModels::default(),
                ))
                .await
                .unwrap();
            let terminal = wait_for_terminal(&service, run).await;
            assert_eq!(terminal.phase(), ProductRunPhase::WaitingForUser, "{}", terminal.summary());
            let snapshot =
                service.query_interaction(ProductRunConversationQuery::new(run)).unwrap();
            let notice = snapshot
                .activities()
                .iter()
                .find(|activity| activity.text() == "Repaired model JSON formatting")
                .expect("visible repair notice");
            assert!(!notice.detail().contains(arguments));
            assert_eq!(
                snapshot
                    .activities()
                    .iter()
                    .any(|activity| activity.kind() == ProductActivityKind::Tool
                        && activity.text().starts_with("Failed")),
                expected_error
            );
            let requests = writer.requests.lock().unwrap().clone();
            assert!(requests.last().unwrap().messages().iter().flat_map(peritus_model_protocol::Message::content).any(|block| matches!(block, peritus_model_protocol::ContentBlock::ToolResult(result) if result.is_error() == expected_error)));
            assert_eq!(fs::read(repository.path().join("src/lib.rs")).unwrap(), before);
            let records =
                super::super::persistence::load_records(&service.inner.directory).unwrap();
            assert_eq!(
                records[&run].interaction.as_ref().unwrap().activities,
                snapshot.activities()
            );
            let traces: Vec<_> = fs::read_dir(&service.inner.directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "trace"))
                .collect();
            assert_eq!(traces.len(), 1);
            let trace = fs::read(&traces[0]).unwrap();
            let encoded_original = serde_json::Value::from(arguments).to_string();
            assert!(
                trace
                    .windows(encoded_original.len())
                    .any(|window| window == encoded_original.as_bytes()),
                "original repair input is retained in the private trace"
            );
            service.shutdown(Duration::from_secs(5)).await;
        }
    });
}
