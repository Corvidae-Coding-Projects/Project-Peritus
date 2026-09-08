//! Exact final-message binding and tool-proposal admission regressions.
use super::*;
use serde_json::json;

fn transcript(messages: &[&str]) -> Vec<u8> {
    let mut events =
        vec![json!({"type":"thread.started", "thread_id":"test"}), json!({"type":"turn.started"})];
    for (index, text) in messages.iter().enumerate() {
        events.push(json!({"type":"item.completed", "item":{"id":format!("message-{index}"), "type":"agent_message", "text":text}}));
    }
    events.push(json!({"type":"turn.completed", "usage":{"input_tokens":10,"output_tokens":2}}));
    events.into_iter().map(|value| value.to_string() + "\n").collect::<String>().into_bytes()
}

#[test]
fn preliminary_prose_is_accepted_only_with_an_exact_unambiguous_final_binding() {
    let final_message = r#"{"content":"done","tool_calls":[]}"#;
    let bytes = transcript(&["I will inspect the workspace.", final_message]);
    let allowed = BTreeSet::new();
    assert!(matches!(decode(&bytes, &allowed, 0..=0, None), Err(DecodeFailure::MultipleMessages)));
    let result = decode(&bytes, &allowed, 0..=0, Some(final_message)).unwrap();
    assert_eq!(result.content, "done");
    assert_eq!(result.usage.input_tokens(), Some(10));
    let competing = transcript(&[r#"{"content":"different","tool_calls":[]}"#, final_message]);
    assert!(matches!(
        decode(&competing, &allowed, 0..=0, Some(final_message)),
        Err(DecodeFailure::MultipleMessages)
    ));
    let after = transcript(&[final_message, "More prose after the supposedly final result."]);
    assert!(matches!(
        decode(&after, &allowed, 0..=0, Some(final_message)),
        Err(DecodeFailure::InvalidLifecycle)
    ));
    assert!(
        decode(&bytes, &allowed, 0..=0, Some(r#"{"content":"forged","tool_calls":[]}"#)).is_err()
    );
}

#[test]
fn required_tool_and_exact_argument_object_are_enforced_without_repairs() {
    let allowed = BTreeSet::from(["lookup".to_owned()]);
    let absent = transcript(&[r#"{"content":"I will do it later","tool_calls":[]}"#]);
    assert!(matches!(
        decode(&absent, &allowed, 1..=1, None),
        Err(DecodeFailure::InvalidToolChoice)
    ));
    for arguments in ["{bad}", "[]", r#"{"key":1,"key":2}"#, r#""double encoded""#] {
        let message =
            json!({"content":"", "tool_calls":[{"name":"lookup", "arguments_json":arguments}]})
                .to_string();
        assert!(
            matches!(
                decode(&transcript(&[&message]), &allowed, 1..=1, None),
                Err(DecodeFailure::InvalidToolArguments)
            ),
            "{arguments}"
        );
    }
    let message = json!({"content":"", "tool_calls":[{"name":"lookup", "arguments_json":json!({"content":"quotes \" and newlines\n and Unicode é"}).to_string()}]}).to_string();
    let result = decode(&transcript(&[&message]), &allowed, 1..=1, Some(&message)).unwrap();
    assert_eq!(result.tool_calls.len(), 1);
    let forbidden = message.replace("lookup", "undeclared");
    assert!(matches!(
        decode(&transcript(&[&forbidden]), &allowed, 1..=1, None),
        Err(DecodeFailure::InvalidToolChoice)
    ));
    assert!(matches!(
        decode(&transcript(&[&message]), &allowed, 0..=0, None),
        Err(DecodeFailure::InvalidToolChoice)
    ));
}

#[test]
fn inert_updates_are_accepted_but_native_updates_still_fail_closed() {
    let final_message = r#"{"content":"done","tool_calls":[]}"#;
    let base = String::from_utf8(transcript(&[final_message])).unwrap();
    let with_update = |kind| {
        base.replace(
            "{\"type\":\"turn.started\"}\n",
            &format!(
                "{{\"type\":\"turn.started\"}}\n{}\n",
                json!({"type":"item.updated","item":{"type":kind}}),
            ),
        )
    };
    let inert = with_update("reasoning");
    assert!(decode(inert.as_bytes(), &BTreeSet::new(), 0..=0, Some(final_message)).is_ok());
    let native = with_update("command_execution");
    assert!(matches!(
        decode(native.as_bytes(), &BTreeSet::new(), 0..=0, Some(final_message)),
        Err(DecodeFailure::NativeTool)
    ));
}
