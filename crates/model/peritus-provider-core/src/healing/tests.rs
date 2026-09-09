use super::*;

#[test]
fn repairs_complete_formatting_and_keeps_original_provenance() {
    for input in [
        "```json\n{\"path\":\"src/é.rs\",}\n```",
        "Here are the arguments:\n{path: \"src/é.rs\"}",
        "{path: \"src/é.rs\",}",
        r#""{\"path\":\"src/é.rs\"}""#,
    ] {
        let (value, audit) =
            object(input, "call-1", ProtocolLimits::PRODUCTION).unwrap().into_parts();
        assert_eq!(value.canonical_bytes(), "{\"path\":\"src/é.rs\"}".as_bytes());
        let Some(ModelEvent::ProviderEvent(audit)) = audit else { panic!("missing audit") };
        let record: Value = serde_json::from_slice(audit.value().canonical_bytes()).unwrap();
        assert_eq!(record["original"], input);
        assert_eq!(record["target"], "call-1");
        assert_eq!(record["policy"], "syntax-only-v1");
        assert!(
            object(record["repaired"].as_str().unwrap(), "call-1", ProtocolLimits::PRODUCTION)
                .unwrap()
                .into_parts()
                .1
                .is_none()
        );
    }
}

#[test]
fn valid_json_and_opaque_string_contents_are_never_rewritten() {
    let input = r#"{"code":"{bare: 1,} \\"quoted\\"", "nested":[{"x":true},],}"#;
    // Use independent expected string values, including quotes, escapes and literal commas.
    let expected = Value::from_iter([("code", Value::from("{bare: 1,} \"quoted\"\n é"))]);
    let valid = expected.to_string();
    let (value, audit) = object(&valid, "tool", ProtocolLimits::PRODUCTION).unwrap().into_parts();
    assert!(audit.is_none());
    assert_eq!(serde_json::from_slice::<Value>(value.canonical_bytes()).unwrap(), expected);
    let trailing = format!("{},}}", valid.strip_suffix('}').unwrap());
    let healed = object(&trailing, "tool", ProtocolLimits::PRODUCTION).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(healed.value().canonical_bytes()).unwrap(),
        expected
    );
    assert!(object(input, "tool", ProtocolLimits::PRODUCTION).is_err());
}

#[test]
fn rejects_truncation_ambiguity_duplicate_keys_types_and_invented_values() {
    for input in [
        "{\"x\":1",
        "{x:",
        "{ x: }",
        "{,}",
        "{x:[,]}",
        "{ x:tru }",
        "{x:1 x:2}",
        "{x:1,x:2}",
        "{\"x\":1,\"x\":2}",
        "{ x:1, }{ y:2 }",
        "{ x:1 } true",
        "null { x:1 }",
        "[1,2,]",
        "{x:'value'}",
        "{ x:NaN }",
        "{ x:undefined }",
        "```json\n{ x:1 }\n```\n{ x:2 }",
    ] {
        assert!(object(input, "tool", ProtocolLimits::PRODUCTION).is_err(), "{input}");
    }
    let oversized = format!("{{x:\"{}\",}}", "a".repeat(MAX_REPAIR_BYTES));
    assert!(object(&oversized, "tool", ProtocolLimits::PRODUCTION).is_err());
    let deep = format!("{{x:{}0{}}}", "[".repeat(65), "]".repeat(65));
    assert!(object(&deep, "tool", ProtocolLimits::PRODUCTION).is_err());
}

#[test]
fn tool_events_are_audited_before_valid_arguments_and_invalid_utf8_rejects() {
    let id = ToolCallId::new("call".to_owned()).unwrap();
    let events = tool_arguments(b"{path:\"a\",}", &id, ProtocolLimits::PRODUCTION).unwrap();
    assert!(matches!(&events[0], ModelEvent::ProviderEvent(_)));
    assert!(matches!(&events[1], ModelEvent::ToolArgumentDelta { call_id, .. } if call_id == &id));
    assert!(tool_arguments(&[0xff], &id, ProtocolLimits::PRODUCTION).is_err());
}

#[test]
fn nested_arrays_keep_literal_values_and_valid_fragment_bytes() {
    let item = ItemId::new("result".to_owned()).unwrap();
    let raw = b"[true,false,null,{value:[1,2,],},]";
    let events = structured_output(raw, &item, ProtocolLimits::PRODUCTION).unwrap();
    assert!(
        matches!(&events[1], ModelEvent::TextDelta { fragment, .. } if fragment.expose() == br#"[true,false,null,{"value":[1,2]}]"#)
    );
    let id = ToolCallId::new("call".to_owned()).unwrap();
    let mut buffer = ToolArgumentBuffer::new();
    let parts = [b" { \"x\":".as_slice(), b" [true, false] } ".as_slice()];
    buffer.append(parts[0], ProtocolLimits::PRODUCTION).unwrap();
    assert!(buffer.complete(&id, ProtocolLimits::PRODUCTION).is_err());
    buffer.append(parts[1], ProtocolLimits::PRODUCTION).unwrap();
    let events = buffer.complete(&id, ProtocolLimits::PRODUCTION).unwrap();
    let observed: Vec<_> = events
        .iter()
        .map(|event| match event {
            ModelEvent::ToolArgumentDelta { fragment, .. } => fragment.expose(),
            _ => panic!("valid arguments require no repair event"),
        })
        .collect();
    assert_eq!(observed, parts);
    assert!(buffer.append(b"", ProtocolLimits::PRODUCTION).is_err());
}
