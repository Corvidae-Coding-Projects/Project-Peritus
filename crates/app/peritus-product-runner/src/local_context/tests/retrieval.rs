//! Bounded exact readback, middle diagnostics, literal search, and scope rejection.

use super::{super::*, support::*};
use serde_json::Value;

#[test]
fn advertised_context_read_bounds_match_default_execution() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "advertised-read-bounds");
    let definition = tools::definitions()
        .unwrap()
        .into_iter()
        .find(|tool| tool.name().as_str() == "context_read")
        .unwrap();
    let schema: Value = serde_json::from_slice(definition.parameters().canonical_bytes()).unwrap();
    let maximum = schema["properties"]["max_bytes"]["maximum"].as_u64().unwrap();
    let request = serde_json::json!({
        "observation_ids": [], "query": null, "cursor": null,
        "offset": 0, "max_bytes": maximum
    });
    let result = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
    assert!(result.get("rejected").is_none(), "advertised maximum rejected: {result}");
    assert_eq!(usize::try_from(maximum).unwrap(), memory.config.max_read_bytes);
    assert_eq!(schema["properties"]["query"]["minLength"], 1);
}

#[test]
fn configured_context_read_schema_admits_the_actual_boundary_and_rejects_more() {
    for maximum in [8192, 16384, 65536] {
        let fixture = Fixture::new();
        let mut memory = fixture.open();
        begin(&mut memory, "configured-read-bounds");
        memory.config.max_read_bytes = maximum;
        let definition = tools::definitions_with_config(&memory.config)
            .unwrap()
            .into_iter()
            .find(|tool| tool.name().as_str() == "context_read")
            .unwrap();
        let schema: Value =
            serde_json::from_slice(definition.parameters().canonical_bytes()).unwrap();
        assert_eq!(schema["properties"]["max_bytes"]["maximum"], maximum);
        let mut request = serde_json::json!({
            "observation_ids": [], "query": null, "cursor": null,
            "offset": 0, "max_bytes": maximum
        });
        let result = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
        assert!(result.get("rejected").is_none(), "{result}");
        assert!(result.to_string().len() <= maximum);
        request["max_bytes"] = (maximum + 1).into();
        let rejected = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
        assert!(rejected["rejected"].as_str().unwrap().contains(&maximum.to_string()));
        assert_eq!(memory.retrieval_calls, 1);
    }
}

#[test]
fn rejected_context_reads_explain_bounds_and_null_query_recovery() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "read-bound-recovery");
    memory.config.max_read_bytes = 8192;
    let mut request = serde_json::json!({
        "observation_ids": [], "query": null, "cursor": null,
        "offset": 0, "max_bytes": 20000
    });
    let result = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
    let detail = result["rejected"].as_str().unwrap();
    assert!(detail.contains("256..=8192"), "{detail}");
    assert_eq!(memory.retrieval_calls, 0);
    request["max_bytes"] = Value::from(8192);
    request["query"] = Value::from("");
    let result = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
    let detail = result["rejected"].as_str().unwrap();
    assert!(detail.contains("null"), "{detail}");
    assert_eq!(memory.retrieval_calls, 0);
    request["query"] = Value::Null;
    let repaired = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
    assert!(repaired.get("rejected").is_none(), "{repaired}");
    assert_eq!(memory.retrieval_calls, 1);
}

#[test]
fn bounded_pages_report_unreturned_handles_and_state_cursors_are_revision_bound() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "paging");
    let first = observation(&mut memory, "first", &"\\\"λ".repeat(2000), false);
    let second = observation(&mut memory, "second", "short second source", false);
    let request = Value::from_iter([
        (
            "observation_ids",
            Value::Array(vec![
                Value::from(format!("obs:{first:06}")),
                Value::from(format!("obs:{second:06}")),
            ]),
        ),
        ("query", Value::Null),
        ("cursor", Value::Null),
        ("offset", Value::from(0)),
        ("max_bytes", Value::from(700)),
    ]);
    let page = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
    assert!(page.get("rejected").is_none(), "{page}");
    assert!(page.to_string().len() <= 700);
    assert_eq!(page["next_handle_index"], 1);
    assert!(page["sources"][0]["next_offset"].as_u64().unwrap() > 0);
    for index in 0..12 {
        let proposal = update(
            memory.model_revision,
            first,
            &format!("fact-{index}"),
            &format!("Hypothesis {index}: {}", "detail ".repeat(25)),
        );
        assert!(
            tools::update::execute(&mut memory, proposal.to_string().as_bytes())
                .unwrap()
                .get("rejected")
                .is_none()
        );
    }
    let mut request = Value::from_iter([
        ("observation_ids", Value::Array(vec![])),
        ("query", Value::Null),
        ("cursor", Value::Null),
        ("offset", Value::from(0)),
        ("max_bytes", Value::from(900)),
    ]);
    let mut count = 0;
    let mut saved = None;
    loop {
        let page = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
        assert!(page.get("rejected").is_none(), "{page}");
        assert!(page.to_string().len() <= 900);
        count += page["entries"].as_array().unwrap().len();
        assert!(count <= 12, "pagination must advance");
        if page["cursor"].is_null() {
            break;
        }
        saved = Some(page["cursor"].clone());
        request["cursor"] = page["cursor"].clone();
    }
    assert_eq!(count, 12);
    let proposal =
        update(memory.model_revision, first, "new", "A later update invalidates entry pagination.");
    tools::update::execute(&mut memory, proposal.to_string().as_bytes()).unwrap();
    request["cursor"] = saved.unwrap();
    assert!(
        tools::read::execute(&mut memory, request.to_string().as_bytes())
            .unwrap()
            .get("rejected")
            .is_some()
    );
}

#[test]
fn exact_read_and_search_find_diagnostics_beyond_any_prefix_preview() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "one");
    let text = format!("{}DECISIVE-DIAGNOSTIC{}", "x".repeat(32_000), "y".repeat(32_000));
    let id = observation(&mut memory, "large", &text, true);
    let exact = memory.artifact(id).unwrap();
    let offset =
        exact.windows(19).position(|part| part == b"DECISIVE-DIAGNOSTIC").unwrap_or(32_015);
    let request = Value::from_iter([
        ("observation_ids", Value::Array(vec![Value::from(format!("obs:{id:06}"))])),
        ("query", Value::Null),
        ("cursor", Value::Null),
        ("offset", Value::from(offset)),
        ("max_bytes", Value::from(2048)),
    ]);
    let response = tools::read::execute(&mut memory, request.to_string().as_bytes()).unwrap();
    assert!(response.get("rejected").is_none(), "{response}");
    assert!(response.to_string().len() <= 2048);
    assert_eq!(response["sources"][0]["encoding"], Value::from("utf8"));
    assert!(
        response["sources"][0]["data"]
            .as_str()
            .unwrap()
            .as_bytes()
            .starts_with(&exact[offset..offset + 19])
    );
    let query = Value::from_iter([
        ("observation_ids", Value::Array(vec![])),
        ("query", Value::from("DECISIVE-DIAGNOSTIC")),
        ("cursor", Value::Null),
        ("offset", Value::from(0)),
        ("max_bytes", Value::from(2048)),
    ]);
    let response = tools::read::execute(&mut memory, query.to_string().as_bytes()).unwrap();
    assert_eq!(response["sources"][0]["total_bytes"], Value::from(exact.len()));
    assert!(response["sources"][0]["data"].as_str().unwrap().contains("DECISIVE-DIAGNOSTIC"));
    let calls = memory.retrieval_calls;
    let cross = Value::from_iter([
        (
            "observation_ids",
            Value::Array(vec![Value::from(format!("obs:{}:{id:06}", "ff".repeat(32)))]),
        ),
        ("query", Value::Null),
        ("cursor", Value::Null),
        ("offset", Value::from(0)),
        ("max_bytes", Value::from(2048)),
    ]);
    assert!(
        tools::read::execute(&mut memory, cross.to_string().as_bytes())
            .unwrap()
            .get("rejected")
            .is_some()
    );
    assert_eq!(memory.retrieval_calls, calls);
}
