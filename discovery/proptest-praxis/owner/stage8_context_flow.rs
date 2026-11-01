//! One durable lineage through source archive, conservative decision view and plan receipts.

use super::*;
use peritus_model_protocol::ToolResult;
use proptest::test_runner::{
    Config, FileFailurePersistence, RngAlgorithm, RngSeed, TestCaseError, TestRunner,
};

#[derive(Clone, Debug)]
struct FlowCase {
    source: u8,
    plan_len: u8,
    payload_units: u8,
    reopen_before_plan: bool,
    reopen_after_plan: bool,
    revoke_before_readback: bool,
}

fn strategy() -> impl Strategy<Value = FlowCase> {
    (0_u8..20, 1_u8..=4, 0_u8..=8, any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
        |(
            source,
            plan_len,
            payload_units,
            reopen_before_plan,
            reopen_after_plan,
            revoke_before_readback,
        )| FlowCase {
            source,
            plan_len,
            payload_units,
            reopen_before_plan,
            reopen_after_plan,
            revoke_before_readback,
        },
    )
}

fn evidence_card(messages: &[Message], source_id: u64) -> Result<String, String> {
    let marker = format!("sequence={source_id}\n");
    let mut cards = Vec::new();
    for message in messages {
        for block in message.content() {
            let ContentBlock::Text(text) = block else { continue };
            let body = text.expose_for_wire();
            if body.starts_with("DECISION TOOL EVIDENCE V1\n") && body.contains(&marker) {
                cards.push((message.role(), body.to_owned()));
            }
        }
    }
    if cards.len() != 1 || cards[0].0 != Role::User {
        let previews = messages
            .iter()
            .flat_map(Message::content)
            .filter_map(|block| match block {
                ContentBlock::Text(text) => {
                    Some(text.expose_for_wire().chars().take(240).collect::<String>())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        return Err(format!(
            "expected one untrusted source card for {source_id}; got {cards:?}; bodies {previews:?}"
        ));
    }
    let source = cards[0]
        .1
        .lines()
        .nth(2)
        .and_then(|line| line.strip_prefix("source="))
        .and_then(|line| line.split_whitespace().next())
        .ok_or("source card lacks locator")?;
    if source.is_empty() {
        return Err("source card has empty locator".into());
    }
    Ok(source.to_owned())
}

fn decision_view(
    owner: &mut LocalMemory,
    tools: &[peritus_model_protocol::ToolDefinition],
) -> Result<Vec<Message>, String> {
    let policy = text(Role::System, "CURRENT POLICY")?;
    let state = text(Role::Developer, "CURRENT HOST STATE")?;
    let input = text(Role::User, "CURRENT USER INPUT")?;
    owner
        .prepare_view_with_governing(
            &profile()?,
            tools,
            Some(&policy),
            Some(&state),
            Some(&input),
            None,
        )
        .map_err(|e| e.to_string())
}

fn archive_read(fixture: &Fixture, source: u8, serial: u8) -> Result<u64, String> {
    let path = format!("fixture-{source}.txt");
    let expected_file = fs::read(fixture.workspace.join(&path)).map_err(|e| e.to_string())?;
    let read = call(
        &format!("stage8_archive_read_{serial}"),
        "workspace_read",
        &json!({"path":path,"max_bytes":32768}),
    )?;
    let read_message = Message::new(
        Role::Assistant,
        vec![ContentBlock::ToolCall(read.clone())],
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|e| e.to_string())?;
    {
        let mut owner = fixture.handle()?.lock().map_err(|e| e.to_string())?;
        owner.observe_message(&read_message).map_err(|e| e.to_string())?;
    }
    let mut reader =
        crate::developer_tools::WorkspaceDeveloperTools::read_only(fixture.workspace.clone())
            .with_task_contract(&fixture.control.render())
            .with_protection_view(fixture.control.clone());
    let result = reader.execute(&read).map_err(|e| e.to_string())?;
    let read_value: Value =
        serde_json::from_slice(result.output.canonical_bytes()).map_err(|e| e.to_string())?;
    let expected_text = std::str::from_utf8(&expected_file).map_err(|e| e.to_string())?.trim_end();
    if result.is_error
        || !read_value["content"].as_str().is_some_and(|content| content.contains(expected_text))
    {
        return Err("ordinary file read did not return the expected exact fixture bytes".into());
    }
    let source_id = {
        let mut owner = fixture.handle()?.lock().map_err(|e| e.to_string())?;
        let id = owner.observe_tool(&read, &result).map_err(|e| e.to_string())?;
        let message = Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult::new(
                read.id().clone(),
                result.output,
                false,
            ))],
            ProtocolLimits::PRODUCTION,
        )
        .map_err(|e| e.to_string())?;
        owner.observe_message(&message).map_err(|e| e.to_string())?;
        id
    };
    Ok(source_id)
}

fn check(case: &FlowCase) -> Result<(), String> {
    let mut fixture = Fixture::with_decision(true, true, Stop::None, 0)?;
    let source_id = archive_read(&fixture, case.source, 1)?;
    archive_read(&fixture, (case.source + 1) % 20, 2)?;
    let mut plan_tools =
        crate::developer_tools::read_only_definitions().map_err(|e| e.to_string())?;
    plan_tools.extend(super::super::definitions().map_err(|e| e.to_string())?);
    let view = {
        let mut owner = fixture.handle()?.lock().map_err(|e| e.to_string())?;
        let messages = decision_view(&mut owner, &plan_tools)?;
        owner.publish(&messages).map_err(|e| e.to_string())?;
        messages
    };
    let locator = evidence_card(&view, source_id)?;
    if case.reopen_before_plan {
        fixture.reopen(true)?;
        let mut owner = fixture.handle()?.lock().map_err(|e| e.to_string())?;
        let replayed = decision_view(&mut owner, &plan_tools)?;
        drop(owner);
        let reopened = evidence_card(&replayed, source_id)?;
        if reopened != locator {
            return Err("source locator changed after restart".into());
        }
    }

    let specifications = basic_nodes(usize::from(case.plan_len));
    let expected = interpret(&specifications, u16::from(case.payload_units), Stop::None, 0);
    let mut base = RecordingExecutor::new(
        fixture.control.clone(),
        specifications.len(),
        u16::from(case.payload_units),
    );
    let graph = nodes(&specifications);
    let mut request = fixture.request(Some(&graph), None, 4096)?;
    let mut handles = Vec::new();
    let mut host_calls = 0;
    let mut completed = false;
    for _ in 0..=specifications.len() {
        let response =
            fixture.invoke("inspection_plan", &request, &mut base, &mut host_calls, true)?;
        if response.is_error {
            return Err("valid inspection plan rejected".into());
        }
        let value: Value =
            serde_json::from_slice(response.output.canonical_bytes()).map_err(|e| e.to_string())?;
        validate_page(&value, &specifications, &expected, &mut handles)?;
        match value["status"].as_str() {
            Some("completed") => {
                completed = true;
                break;
            }
            Some("page_complete") => {
                let cursor = value["continuation"].as_str().ok_or("missing plan cursor")?;
                request = fixture.request(None, Some(cursor), 4096)?;
            }
            other => return Err(format!("unexpected plan status {other:?}")),
        }
    }
    if !completed || handles.len() != specifications.len() {
        return Err("plan did not complete every child".into());
    }
    validate_execution_log(&fixture, &expected, host_calls, Stop::None, 0)?;
    if case.reopen_after_plan {
        fixture.reopen(true)?;
    }
    let after_view = {
        let mut owner = fixture.handle()?.lock().map_err(|e| e.to_string())?;
        decision_view(&mut owner, &plan_tools)?
    };
    if evidence_card(&after_view, source_id)? != locator {
        return Err("prior source locator changed after plan".into());
    }
    if case.revoke_before_readback {
        fixture.control.read.store(false, Ordering::Release);
        let denied = fixture.invoke(
            "inspection_result_read",
            &json!({"step":handles[0],"offset":0,"max_bytes":256}),
            &mut base,
            &mut 0,
            true,
        )?;
        if !denied.is_error {
            return Err("revoked read retrieved a plan result".into());
        }
        fixture.control.read.store(true, Ordering::Release);
    }
    let mut metrics = Metrics::new();
    for (index, handle) in handles.iter().enumerate() {
        retrieve(&mut fixture, &mut base, handle, &expected.outputs[index].0, &mut metrics)?;
    }
    if fixture.control.log.lock().map_err(|_| "log poisoned")?.executed.len()
        != specifications.len()
    {
        return Err("receipt retrieval dispatched an extra child".into());
    }
    Ok(())
}

#[test]
fn stage8_archive_decision_plan_receipt_restart_fixed() {
    for case in [
        FlowCase {
            source: 0,
            plan_len: 1,
            payload_units: 0,
            reopen_before_plan: true,
            reopen_after_plan: true,
            revoke_before_readback: true,
        },
        FlowCase {
            source: 19,
            plan_len: 4,
            payload_units: 8,
            reopen_before_plan: false,
            reopen_after_plan: true,
            revoke_before_readback: false,
        },
    ] {
        check(&case).unwrap_or_else(|error| panic!("{case:?}: {error}"));
    }
}

#[test]
fn stage8_archive_decision_plan_receipt_restart_generated() {
    let mut runner = TestRunner::new(Config {
        cases: 128,
        rng_seed: RngSeed::Fixed(8_008),
        rng_algorithm: RngAlgorithm::ChaCha,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../discovery/proptest-praxis/cases/stage8-context-failures.txt"
        )))),
        ..Config::default()
    });
    runner.run(&strategy(), |case| check(&case).map_err(TestCaseError::fail)).unwrap();
}
