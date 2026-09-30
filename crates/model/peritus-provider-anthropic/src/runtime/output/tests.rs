use super::*;

#[test]
fn reported_runtime_failures_keep_recovery_relevant_causes() {
    let allowed = BTreeSet::new();
    for (message, expected) in [
        ("OAuth session expired", DecodeFailure::Authentication),
        ("Selected model is at capacity", DecodeFailure::Capacity),
        ("prompt exceeds context window", DecodeFailure::ContextLimit),
        ("provider rejected the turn", DecodeFailure::Reported),
    ] {
        let mut value = Map::new();
        value.insert("is_error".to_owned(), Value::Bool(true));
        value.insert("result".to_owned(), Value::String(message.to_owned()));
        let bytes = serde_json::to_vec(&Value::Object(value)).expect("fixture JSON");
        match decode(&bytes, &allowed, 0) {
            Err(observed) => assert_eq!(observed, expected),
            Ok(_) => panic!("reported provider failure decoded as a successful turn"),
        }
    }
}

#[test]
fn sdk_error_arrays_preserve_recovery_causes_without_returning_partial_calls() {
    for (message, expected) in [
        ("OAuth session expired", DecodeFailure::Authentication),
        ("Selected model is at capacity", DecodeFailure::Capacity),
        ("prompt exceeds context window", DecodeFailure::ContextLimit),
        ("Invalid model: missing", DecodeFailure::InvalidModel),
        ("provider rejected the turn", DecodeFailure::Reported),
    ] {
        let output = serde_json::json!({
            "type":"result", "subtype":"error_during_execution", "is_error":true,
            "errors":["The request failed", message], "result":null,
            "structured_output":{"content":"partial", "tool_calls":[
                {"name":"workspace_read", "arguments":{"path":"README.md"}}
            ]}
        });
        assert_eq!(decode(output.to_string().as_bytes(), &allowed(), 1).err(), Some(expected));
    }
    for errors in [serde_json::json!("wrong shape"), serde_json::json!(["OAuth expired", 2])] {
        let output = serde_json::json!({"is_error":true, "errors":errors});
        assert!(matches!(
            decode(output.to_string().as_bytes(), &allowed(), 1),
            Err(DecodeFailure::Malformed)
        ));
    }
}

#[test]
fn sdk_error_subtypes_cannot_become_success_when_the_error_boolean_is_false() {
    for subtype in [
        "error_during_execution",
        "error_max_turns",
        "error_max_budget_usd",
        "error_max_structured_output_retries",
    ] {
        let output = serde_json::json!({
            "type":"result", "subtype":subtype, "is_error":false, "errors":[],
            "structured_output":{"content":"partial", "tool_calls":[
                {"name":"workspace_read", "arguments":{"path":"README.md"}}
            ]}
        });
        assert!(matches!(
            decode(output.to_string().as_bytes(), &allowed(), 1),
            Err(DecodeFailure::Reported)
        ));
    }
}

fn allowed() -> BTreeSet<String> {
    let mut tools = BTreeSet::new();
    tools.insert("workspace_read".to_owned());
    tools
}

#[test]
fn direct_single_turn_result_decodes_only_complete_declared_operations() {
    let result = r#"{"content":"Reading the file.","tool_calls":[{"name":"workspace_read","arguments":{"path":"README.md"}}]}"#;
    let output = serde_json::json!({"is_error":false,"num_turns":1,"result":result});
    let turn = decode(output.to_string().as_bytes(), &allowed(), 1).unwrap();
    assert_eq!(turn.tool_calls[0].name, "workspace_read");
    assert_eq!(turn.tool_calls[0].arguments["path"], "README.md");
    assert!(turn.repairs.is_empty());
    let fenced = serde_json::json!({"is_error":false,"result":format!("```json\n{result}\n```")});
    let healed = decode(fenced.to_string().as_bytes(), &allowed(), 1).unwrap();
    assert_eq!(healed.tool_calls[0].name, "workspace_read");
    assert_eq!(healed.repairs.len(), 1);
    for rejected in [
        format!("Example: ```json\n{result}\n```"),
        result.replace("workspace_read", "undeclared_tool"),
        result.replace("\"content\":", "\"content\":\"duplicate\",\"content\":"),
        result.replace("\"content\":", "\"extra\":true,\"content\":"),
        "unstructured output".to_owned(),
    ] {
        let output = serde_json::json!({"is_error":false,"result":rejected});
        assert!(decode(output.to_string().as_bytes(), &allowed(), 1).is_err());
    }
    let output = serde_json::json!({"is_error":true,"result":result});
    assert!(matches!(
        decode(output.to_string().as_bytes(), &allowed(), 1),
        Err(DecodeFailure::Reported)
    ));
}

#[test]
fn direct_private_result_accepts_one_complete_object_after_a_plain_prelude() {
    // Captured from a successful official-client turn for a read-only greeting task.
    let prelude =
        "I'll take a look at the project structure to see what this greeting project contains.";
    let result = r#"{"content":"I'll look at the project files.","tool_calls":[{"name":"workspace_list","arguments":{"path":".","depth":2}}]}"#;
    let output = serde_json::json!({
        "type":"result", "subtype":"success", "is_error":false,
        "result":format!("{prelude}\n\n{result}")
    });
    let tools = BTreeSet::from(["workspace_list".to_owned()]);
    let turn = decode(output.to_string().as_bytes(), &tools, 1).expect("private envelope");
    assert_eq!(turn.content, "I'll look at the project files.");
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "workspace_list");
    assert_eq!(turn.tool_calls[0].arguments["path"], ".");
    assert_eq!(turn.tool_calls[0].arguments["depth"], 2);
    assert_eq!(turn.repairs.len(), 1);
    let ModelEvent::ProviderEvent(audit) = &turn.repairs[0] else { panic!("missing audit") };
    let audit: Value = serde_json::from_slice(audit.value().canonical_bytes()).unwrap();
    assert_eq!(audit["original"], format!("{prelude}\n\n{result}"));

    for rejected in [
        format!("{prelude} {result}"),
        format!("{prelude}\n\n```json\n{result}\n```"),
        format!("\"quoted candidate\"\n\n{result}"),
        format!("{{}}\n\n{result}"),
        format!("{prelude}\n\n{result}\n{{}}"),
        format!("{prelude}\n\n{result} trailing prose"),
        format!("{prelude}\n\n{}", &result[..result.len() - 1]),
        format!("{prelude}\n\n{}", result.replace("workspace_list", "undeclared_tool")),
        format!("{prelude}\n\n{}", result.replace("\"depth\":2", "\"depth\":2,\"depth\":3")),
    ] {
        let output = serde_json::json!({"is_error":false,"result":rejected});
        assert!(decode(output.to_string().as_bytes(), &tools, 1).is_err());
    }
    assert!(decode(output.to_string().as_bytes(), &tools, 0).is_err());
    let public = serde_json::json!({
        "structured_output":{"content":format!("{prelude}\n\n{result}"),"tool_calls":[]}
    });
    let turn = decode(public.to_string().as_bytes(), &tools, 1).expect("ordinary public content");
    assert_eq!(turn.content, format!("{prelude}\n\n{result}"));
    assert!(turn.tool_calls.is_empty());
    assert!(turn.repairs.is_empty());
}

#[test]
fn fenced_tool_examples_in_public_content_are_not_promoted_by_healing() {
    let example =
        "```json\n{tool_calls:[{name:\"workspace_read\",arguments:{path:\"src/lib.rs\"}}]}\n```";
    let output = Value::from_iter([
        ("is_error", Value::from(false)),
        (
            "structured_output",
            Value::from_iter([
                ("content", Value::from(example)),
                ("tool_calls", Value::from(Vec::<Value>::new())),
            ]),
        ),
    ])
    .to_string();
    let turn = decode(output.as_bytes(), &allowed(), 1).unwrap();
    assert_eq!(turn.content, example);
    assert!(turn.tool_calls.is_empty());
    assert!(turn.repairs.is_empty());
}

#[test]
fn public_json_scalars_and_arrays_remain_ordinary_content() {
    for content in ["42", "true", "null", "[1,2]", "\"hello\""] {
        let output = Value::from_iter([(
            "structured_output",
            Value::from_iter([
                ("content", Value::from(content)),
                ("tool_calls", Value::from(Vec::<Value>::new())),
            ]),
        )])
        .to_string();
        let turn = decode(output.as_bytes(), &allowed(), 1).unwrap();
        assert_eq!(turn.content, content);
        assert!(turn.tool_calls.is_empty());
        assert!(turn.repairs.is_empty());
    }
}

#[test]
fn embedded_host_call_is_promoted_and_application_content_is_preserved() {
    let output = br#"{
      "is_error": false,
      "structured_output": {
        "content": "{\"summary\":\"need one more file\",\"tool_calls\":[{\"name\":\"workspace_read\",\"arguments\":{\"path\":\"src/lib.rs\"}}]}",
        "tool_calls": []
      }
    }"#;

    let turn = decode(output, &allowed(), 1).expect("embedded host call");

    assert_eq!(turn.content, r#"{"summary":"need one more file"}"#);
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].name, "workspace_read");
    assert_eq!(turn.tool_calls[0].arguments["path"], "src/lib.rs");
}

#[test]
fn empty_embedded_call_array_is_removed_from_terminal_application_json() {
    let output = br#"{
      "structured_output": {
        "content": "{\"summary\":\"verified\",\"findings\":[],\"tool_calls\":[]}",
        "tool_calls": []
      }
    }"#;

    let turn = decode(output, &allowed(), 1).expect("embedded terminal content");

    assert_eq!(turn.content, r#"{"findings":[],"summary":"verified"}"#);
    assert!(turn.tool_calls.is_empty());
}

#[test]
fn embedded_undeclared_host_call_still_fails_closed() {
    let output = br#"{
      "structured_output": {
        "content": "{\"summary\":\"bad call\",\"tool_calls\":[{\"name\":\"shell\",\"arguments\":{}}]}",
        "tool_calls": []
      }
    }"#;

    assert!(matches!(decode(output, &allowed(), 1), Err(DecodeFailure::Malformed)));
}

#[test]
fn direct_public_json_example_does_not_authorize_a_tool_call() {
    let example = r#"{"content":"example only","tool_calls":[{"name":"workspace_read","arguments":{"path":"README.md"}}]}"#;
    let envelope = serde_json::json!({"content":example,"tool_calls":[]});
    let output = serde_json::json!({"is_error":false,"result":envelope.to_string()});
    let turn = decode(output.to_string().as_bytes(), &allowed(), 1).unwrap();
    assert_eq!(turn.content, example);
    assert!(turn.tool_calls.is_empty());
}

#[test]
fn explicit_native_single_turn_completion_delivers_public_text_without_tool_authority() {
    let content =
        "The script prints \"Hello, {name}!\". Mentioning workspace_read does not run it.";
    let output = serde_json::json!({
        "type":"result", "subtype":"success", "is_error":false,
        "stop_reason":"end_turn", "terminal_reason":"completed", "num_turns":1,
        "permission_denials":[], "api_error_status":null, "result":content,
        "usage":{"input_tokens":12,"output_tokens":7}
    });
    let turn = decode(output.to_string().as_bytes(), &allowed(), 1).unwrap();
    assert_eq!(turn.content, content);
    assert!(turn.tool_calls.is_empty());
    assert!(turn.repairs.is_empty());
    assert_eq!(turn.usage.input_tokens(), Some(12));
    assert_eq!(turn.usage.output_tokens(), Some(7));

    for field in [
        "type",
        "subtype",
        "is_error",
        "stop_reason",
        "terminal_reason",
        "num_turns",
        "permission_denials",
    ] {
        let mut missing = output.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(decode(missing.to_string().as_bytes(), &allowed(), 1).is_err(), "{field}");
    }
    for (field, invalid) in [
        ("type", serde_json::json!("assistant")),
        ("subtype", serde_json::json!("error_max_turns")),
        ("is_error", serde_json::json!(true)),
        ("stop_reason", serde_json::json!("max_tokens")),
        ("stop_reason", serde_json::json!("tool_use")),
        ("terminal_reason", serde_json::json!("aborted_streaming")),
        ("terminal_reason", serde_json::json!("tool_deferred")),
        ("num_turns", serde_json::json!(0)),
        ("num_turns", serde_json::json!(2)),
        ("permission_denials", serde_json::json!([{"tool_name":"Read"}])),
        ("permission_denials", serde_json::json!(false)),
        ("errors", serde_json::json!(["unexpected error"])),
        ("errors", serde_json::json!(false)),
        ("api_error_status", serde_json::json!(429)),
        ("deferred_tool_use", serde_json::json!({"name":"Read"})),
        ("local_command", serde_json::json!("compact")),
        ("origin", serde_json::json!({"kind":"task-notification"})),
        ("origin", serde_json::json!(null)),
        ("queued_turn_count", serde_json::json!(1)),
        ("queued_turn_count", serde_json::json!(null)),
        ("subagent_stats", serde_json::json!({"spawned":1})),
        ("subagent_stats", serde_json::json!(null)),
        ("result", serde_json::json!(null)),
        ("result", serde_json::json!("  \n")),
    ] {
        let mut invalid_output = output.clone();
        invalid_output[field] = invalid;
        assert!(decode(invalid_output.to_string().as_bytes(), &allowed(), 1).is_err(), "{field}");
    }
    for private in [
        "{\"content\":\"answer\"}",
        "Here is the turn: {content : \"answer\", tool_calls : [",
        "I will read it.\n\n{\"content\":\"reading\",\"tool_calls\":[",
        "```json\n{\"content\":\"answer\"}\n```",
        "{\"content\":\"answer\",\"tool_calls\":[{\"name\":\"undeclared\",\"arguments\":{}}]}",
        "{\"content\":\"answer\",\"tool_calls\":[],\"extra\":true}",
        "{\"content\":\"answer\",\"content\":\"duplicate\",\"tool_calls\":[]}",
        "[1,2]",
    ] {
        let mut malformed = output.clone();
        malformed["result"] = serde_json::json!(private);
        assert!(decode(malformed.to_string().as_bytes(), &allowed(), 1).is_err(), "{private}");
    }
    let mut human = output;
    human["origin"] = serde_json::json!({"kind":"human"});
    assert_eq!(decode(human.to_string().as_bytes(), &allowed(), 0).unwrap().content, content);
}
