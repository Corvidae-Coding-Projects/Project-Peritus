//! Private final-turn format and public prose policy boundaries.

use super::*;

#[test]
fn public_prose_policy_keeps_private_terminal_transport_and_single_turn() {
    use peritus_model_protocol::{
        BoundedText, ContentBlock, Message, ModelRequest, ProtocolLimits, Role,
    };
    let policy = "Return ordinary public prose, not a build JSON envelope.";
    let content = "The program prints \"Hello, world!\" without changing files.";
    let turn = serde_json::json!({"content":content,"tool_calls":[]});
    for (result, expected_complete) in [(turn.to_string(), true), (content.to_owned(), false)] {
        let raw = serde_json::json!({"type":"result","subtype":"success","is_error":false,"result":result});
        let fake = state(vec![
            output("runtime_auth_true.json"),
            Script::Output {
                success: true,
                stdout: raw.to_string().into_bytes(),
                stderr: Vec::new(),
            },
        ]);
        let provider = runtime_provider(Arc::clone(&fake));
        let original = runtime_request(provider.profile(), false);
        let system = Message::new(
            Role::System,
            vec![ContentBlock::Text(
                BoundedText::new(policy.to_owned(), ProtocolLimits::PRODUCTION).unwrap(),
            )],
            ProtocolLimits::PRODUCTION,
        )
        .unwrap();
        let request = ModelRequest::new(
            provider.profile(),
            original.negotiated(),
            original.request_id().clone(),
            vec![system, original.messages()[1].clone()],
            Vec::new(),
            original.tool_choice().clone(),
            original.parallel_tool_policy(),
            original.options().clone(),
            ProtocolLimits::PRODUCTION,
        )
        .unwrap();
        let observed = events(&provider, request, CancellationToken::new());
        assert!(
            !observed
                .iter()
                .any(|event| matches!(event.event(), ModelEvent::ToolCallStarted { .. }))
        );
        if expected_complete {
            assert!(matches!(observed.last().unwrap().event(), ModelEvent::ResponseCompleted));
            let text: Vec<_> = observed
                .iter()
                .filter_map(|event| match event.event() {
                    ModelEvent::TextDelta { fragment, .. } => Some(fragment.expose()),
                    _ => None,
                })
                .flatten()
                .copied()
                .collect();
            assert_eq!(text, content.as_bytes());
        } else {
            assert!(
                matches!(observed.last().unwrap().event(), ModelEvent::ResponseFailed(failure) if failure.category() == FailureCategory::IncompleteStream)
            );
        }
        let captures = fake.captures.lock().unwrap();
        assert_eq!(captures.len(), 2, "one auth check and one inference only");
        let system = std::str::from_utf8(&captures[1].system).unwrap();
        let wire = system.find("PERITUS WIRE FORMAT FOR EVERY TURN").unwrap();
        let public = system.find(policy).unwrap();
        let closing = system.find("PERITUS TRANSPORT CONTRACT").unwrap();
        assert!(wire < public && public < closing);
        assert!(
            system[..public]
                .contains(r#"{"content":"your complete answer to the user","tool_calls":[]}"#)
        );
        assert!(argument_pair(&captures[1].arguments, "--max-turns", "1"));
        drop(captures);
    }
}

#[test]
fn task_json_report_survives_private_transport_without_a_prose_override() {
    use peritus_model_protocol::{
        BoundedText, ContentBlock, Message, ModelRequest, ProtocolLimits, Role,
    };
    let policy = "Return a JSON report with kind=complete, summary, and run_instructions.";
    let report = serde_json::json!({
        "kind":"complete", "summary":"All checks passed.",
        "run_instructions":"python3 -B -m unittest -v"
    });
    let content = report.to_string();
    let turn = serde_json::json!({"content":content,"tool_calls":[]});
    let raw = serde_json::json!({
        "type":"result", "subtype":"success", "is_error":false,
        "result":turn.to_string()
    });
    let fake = state(vec![
        output("runtime_auth_true.json"),
        Script::Output { success: true, stdout: raw.to_string().into_bytes(), stderr: Vec::new() },
    ]);
    let provider = runtime_provider(Arc::clone(&fake));
    let original = runtime_request(provider.profile(), false);
    let system = Message::new(
        Role::System,
        vec![ContentBlock::Text(
            BoundedText::new(policy.to_owned(), ProtocolLimits::PRODUCTION).unwrap(),
        )],
        ProtocolLimits::PRODUCTION,
    )
    .unwrap();
    let request = ModelRequest::new(
        provider.profile(),
        original.negotiated(),
        original.request_id().clone(),
        vec![system, original.messages()[1].clone()],
        Vec::new(),
        original.tool_choice().clone(),
        original.parallel_tool_policy(),
        original.options().clone(),
        ProtocolLimits::PRODUCTION,
    )
    .unwrap();
    let observed = events(&provider, request, CancellationToken::new());
    assert!(matches!(observed.last().unwrap().event(), ModelEvent::ResponseCompleted));
    assert!(
        !observed.iter().any(|event| matches!(event.event(), ModelEvent::ToolCallStarted { .. }))
    );
    let text: Vec<_> = observed
        .iter()
        .filter_map(|event| match event.event() {
            ModelEvent::TextDelta { fragment, .. } => Some(fragment.expose()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&text).unwrap(), report);
    let captures = fake.captures.lock().unwrap();
    assert_eq!(captures.len(), 2, "one auth check and one inference only");
    assert!(std::str::from_utf8(&captures[1].system).unwrap().contains(policy));
    let prompt = std::str::from_utf8(&captures[1].stdin).unwrap();
    let reminder = prompt.rsplit_once("\n\n").unwrap().1;
    assert!(!reminder.contains("your public prose"), "final reminder overrides task JSON");
    let nested_example = serde_json::json!({"content":r#"{"answer":42}"#,"tool_calls":[]});
    assert!(reminder.contains(&nested_example.to_string()), "missing nested task JSON example");
    assert!(argument_pair(&captures[1].arguments, "--max-turns", "1"));
    drop(captures);
}
