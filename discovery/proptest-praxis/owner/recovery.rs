//! Replay the observed retrieval loop against real durable memory and workspace tools.
use crate::local_context::{
    praxis_support::fixture::{Fixture, call, profile, result_message, text},
    record::ArchiveKind,
};
use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest, DeveloperToolObservation};
use peritus_model_protocol::{
    CanonicalJson, ContentBlock, JsonBounds, Message, ProtocolLimits, Role,
};
use peritus_provider_core::CancellationToken;
use peritus_role::HarnessRole;
use serde_json::{Value, json};
use std::{fs, sync::atomic::Ordering};

pub(super) fn archive(f: &Fixture, name: &str, value: &Value, failed: bool) -> u64 {
    let serial = f.memory().unwrap().lock().unwrap().sources.len();
    let path = value["path"].as_str().unwrap_or("file.txt");
    let arguments = if name == "workspace_write" {
        json!({"path":path,"content":fs::read_to_string(f.workspace.join(path)).unwrap()})
    } else if name == "context_read" {
        json!({"observation_ids":value["sources"].as_array().into_iter().flatten().filter_map(|v|v["handle"].as_str()).collect::<Vec<_>>(),"query":null,"cursor":null,"offset":0,"max_bytes":16384})
    } else {
        json!({"path":path})
    };
    let call = call(&format!("replay-{serial}"), name, &arguments).unwrap();
    let output = DeveloperToolObservation {
        output: CanonicalJson::parse(
            &value.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )
        .unwrap(),
        is_error: failed,
    };
    let mut memory = f.memory().unwrap().lock().unwrap();
    memory
        .observe_message(
            &Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolCall(call.clone())],
                ProtocolLimits::PRODUCTION,
            )
            .unwrap(),
        )
        .unwrap();
    let id = memory.observe_tool(&call, &output).unwrap();
    memory.observe_message(&result_message(&call, &output).unwrap()).unwrap();
    id
}

#[test]
fn semantic_recovery_survives_restart_without_replaying_binary_or_receipts() {
    for role in [HarnessRole::Writer, HarnessRole::Reviewer, HarnessRole::Fixer] {
        let mut f = Fixture::new(true, true, role).unwrap();
        let original = format!("Continue editing transport.rs. {}", "μ🙂 safe text ".repeat(80));
        let id = f
            .memory()
            .unwrap()
            .lock()
            .unwrap()
            .observe_message(&text(Role::Assistant, &original).unwrap())
            .unwrap();
        let raw = f.memory().unwrap().lock().unwrap().artifact(id).unwrap();
        assert!(raw.starts_with(b"P5MS"));
        f.reopen().unwrap();
        let mut offset = 0;
        let mut recovered = String::new();
        loop {
            let (page, error, _) = f
                .execute(
                    "context_recover",
                    &json!({"source":format!("obs:{id:06}"),"offset":offset,"max_bytes":1024}),
                    true,
                )
                .unwrap();
            assert!(!error, "{page}");
            assert!(page.to_string().len() <= 1024);
            assert_eq!(page["representation"], "semantic-json-v1");
            recovered.push_str(page["data"].as_str().unwrap());
            if page["next_offset"].is_null() {
                break;
            }
            let next = usize::try_from(page["next_offset"].as_u64().unwrap()).unwrap();
            assert!(next > offset);
            offset = next;
            f.reopen().unwrap();
        }
        let decoded: Value = serde_json::from_str(&recovered).unwrap();
        assert_eq!(decoded["role"], "assistant");
        assert_eq!(decoded["content"][0]["text"], original);
        assert_eq!(f.memory().unwrap().lock().unwrap().artifact(id).unwrap(), raw);

        let receipt = archive(
            &f,
            "context_read",
            &json!({"sources":[{"handle":format!("obs:{id:06}"),"data":"WRAPPER_CANARY_DO_NOT_REPUBLISH"}]}),
            false,
        );
        let (value, failed, _) = f
            .execute(
                "context_recover",
                &json!({"source":format!("obs:{receipt:06}"),"offset":0,"max_bytes":4096}),
                true,
            )
            .unwrap();
        assert!(!failed);
        let data: Value = serde_json::from_str(value["data"].as_str().unwrap()).unwrap();
        assert_eq!(data["kind"], "retrieval_receipt");
        assert_eq!(data["task_evidence"], false);
        assert!(!value.to_string().contains("WRAPPER_CANARY"));
        f.reopen().unwrap();
        let mut memory = f.memory().unwrap().lock().unwrap();
        let mut index = std::mem::take(&mut memory.decision_index);
        index.refresh(&memory).unwrap();
        assert!(
            index
                .evidence()
                .iter()
                .all(|e| !memory.archived(e.source).unwrap().is_memory_receipt())
        );
        assert!(
            String::from_utf8(memory.artifact(receipt).unwrap())
                .unwrap()
                .contains("WRAPPER_CANARY")
        );
        drop(memory);
    }
}

#[test]
fn repeated_delivery_is_one_fact_but_changed_content_and_errors_are_distinct() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    let first = archive(
        &f,
        "workspace_read",
        &json!({"path":"file.txt","content":"first","local_context":{"handle":"old"},"read_reuse":{"state":"restored"}}),
        false,
    );
    let repeated = archive(
        &f,
        "workspace_read",
        &json!({"path":"file.txt","content":"first","local_context":{"handle":"new"},"read_reuse":{"state":"already_visible"}}),
        false,
    );
    let changed =
        archive(&f, "workspace_read", &json!({"path":"file.txt","content":"second"}), false);
    let failed = archive(&f, "workspace_read", &json!({"path":"file.txt","content":"first"}), true);
    f.reopen().unwrap();
    let mut memory = f.memory().unwrap().lock().unwrap();
    let mut index = std::mem::take(&mut memory.decision_index);
    index.refresh(&memory).unwrap();
    assert_eq!(index.representative(repeated), first);
    assert_eq!(index.evidence().iter().find(|e| e.source == first).unwrap().last_seen, repeated);
    assert_eq!(
        index.evidence().iter().map(|e| e.source).collect::<Vec<_>>(),
        vec![first, changed, failed]
    );
    assert_ne!(memory.artifact(first).unwrap(), memory.artifact(repeated).unwrap());
    drop(memory);
}

#[test]
fn recovery_dependencies_obey_current_policy_after_restart() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    fs::write(f.workspace.join("file.txt"), "SECRET_SOURCE_CANARY").unwrap();
    let (_, failed, id) =
        f.execute("workspace_read", &json!({"path":"file.txt","max_bytes":4096}), true).unwrap();
    assert!(!failed);
    let (page, failed, recovery) = f
        .execute(
            "context_recover",
            &json!({"source":format!("obs:{id:06}"),"offset":0,"max_bytes":4096}),
            true,
        )
        .unwrap();
    assert!(!failed);
    assert!(page.to_string().contains("SECRET_SOURCE_CANARY"));
    f.reopen().unwrap();
    // A protected path disallows writes; an opaque reference disallows reading its body.
    f.view.opaque.store(true, Ordering::Release);
    let tool_message = f
        .memory()
        .unwrap()
        .lock()
        .unwrap()
        .sources
        .iter()
        .find(|source| source.sequence > recovery && source.kind == ArchiveKind::ToolMessage)
        .unwrap()
        .sequence;
    for source in [id, recovery, tool_message] {
        let (page, failed, _) = f
            .execute(
                "context_recover",
                &json!({"source":format!("obs:{source:06}"),"offset":0,"max_bytes":4096}),
                true,
            )
            .unwrap();
        assert!(failed, "{page}");
        assert!(!page.to_string().contains("SECRET_SOURCE_CANARY"));
    }
    f.view.read.store(false, Ordering::Release);
    let (_, failed, _) = f
        .execute(
            "context_recover",
            &json!({"source":format!("obs:{id:06}"),"offset":0,"max_bytes":4096}),
            true,
        )
        .unwrap();
    assert!(failed);
}

#[test]
fn current_continuation_survives_a_long_recovery_history_and_capacity_changes() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    fs::write(f.workspace.join("file.txt"), "current source").unwrap();
    f.execute("workspace_read", &json!({"path":"file.txt","max_bytes":4096}), true).unwrap();
    for _ in 0..40 {
        archive(
            &f,
            "context_read",
            &json!({"sources":[],"data":"RECOVERY_WRAPPER_NOISE".repeat(300)}),
            false,
        );
    }
    f.execute("workspace_read", &json!({"path":"file.txt","max_bytes":4096}), true).unwrap();
    f.memory()
        .unwrap()
        .lock()
        .unwrap()
        .observe_message(
            &text(
                Role::Assistant,
                "NEXT_ACTION: patch transport.rs using the retained negotiation evidence.",
            )
            .unwrap(),
        )
        .unwrap();
    f.reopen().unwrap();
    for capacity in [8192, 200_000, 16_384] {
        let mut memory = f.memory().unwrap().lock().unwrap();
        let messages = memory
            .prepare_view_with_governing(
                &profile(capacity, 256).unwrap(),
                &[],
                None,
                None,
                None,
                None,
            )
            .unwrap();
        let rendered = messages
            .iter()
            .flat_map(Message::content)
            .filter_map(|b| {
                if let ContentBlock::Text(t) = b { Some(t.expose_for_wire()) } else { None }
            })
            .collect::<String>();
        assert!(rendered.contains("NEXT_ACTION: patch transport.rs"));
        assert!(!rendered.contains("RECOVERY_WRAPPER_NOISE"));
        assert!(peritus_agent::estimate_developer_request_tokens(&messages, &[]) <= capacity);
        assert!(
            memory
                .sources
                .iter()
                .any(|s| s.kind == ArchiveKind::ToolOutput && s.is_memory_receipt())
        );
        drop(memory);
    }
}

fn begin_resumed_invocation(f: &Fixture) {
    let request = DeveloperLoopRequest {
        request_prefix: "praxis-resumed-invocation".into(),
        system: "Fixture policy.".into(),
        prompt: "Continue the same task.".into(),
        cyber_access_program: None,
        attachments: Vec::new(),
        tools: Vec::new(),
        limits: DeveloperLoopLimits::new(2, 64).unwrap(),
        cancellation: CancellationToken::default(),
    };
    f.memory()
        .unwrap()
        .lock()
        .unwrap()
        .begin(
            &request,
            &[
                text(Role::System, "Fixture policy.").unwrap(),
                text(Role::User, "Continue the same task.").unwrap(),
            ],
        )
        .unwrap();
}

fn resumed_view(f: &Fixture) -> Vec<Message> {
    let mut memory = f.memory().unwrap().lock().unwrap();
    memory
        .prepare_view_with_governing(&profile(64_000, 256).unwrap(), &[], None, None, None, None)
        .unwrap()
}

#[test]
fn a_new_invocation_replays_the_prior_complete_exchange_window_after_restart() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    fs::write(f.workspace.join("file.txt"), "PRIOR_SESSION_EXACT_CANARY").unwrap();
    let (_, failed, _) =
        f.execute("workspace_read", &json!({"path":"file.txt","max_bytes":4096}), true).unwrap();
    assert!(!failed);
    f.memory()
        .unwrap()
        .lock()
        .unwrap()
        .observe_message(
            &text(
                Role::Assistant,
                "NEXT_UNFINISHED_ACTION: patch the parser using the retained file body.",
            )
            .unwrap(),
        )
        .unwrap();

    begin_resumed_invocation(&f);
    // Publish once in the new invocation, then reconstruct from that checkpoint as a daemon
    // restart does. The prior window must not depend on process-local fields.
    {
        let mut memory = f.memory().unwrap().lock().unwrap();
        let messages = memory
            .prepare_view_with_governing(
                &profile(64_000, 256).unwrap(),
                &[],
                None,
                None,
                None,
                None,
            )
            .unwrap();
        memory.publish(&messages).unwrap();
    }
    f.reopen().unwrap();

    let messages = resumed_view(&f);
    let rendered = messages
        .iter()
        .flat_map(Message::content)
        .map(|block| match block {
            ContentBlock::Text(text) => text.expose_for_wire().to_owned(),
            ContentBlock::ToolResult(result) => result.output().to_wire_string(),
            _ => String::new(),
        })
        .collect::<String>();
    assert!(rendered.contains("PRIOR_SESSION_EXACT_CANARY"));
    assert!(rendered.contains("NEXT_UNFINISHED_ACTION: patch the parser"));
    assert_eq!(
        messages
            .iter()
            .flat_map(Message::content)
            .filter(|block| matches!(block, ContentBlock::ToolCall(call) if call.name().as_str() == "workspace_read"))
            .count(),
        1,
        "the prior read should be replayed once, not rediscovered or duplicated"
    );
    fs::write(f.workspace.join("file.txt"), "changed after the retained read").unwrap();
    let messages = resumed_view(&f);
    let retained = messages
        .iter()
        .flat_map(Message::content)
        .find_map(|block| match block {
            ContentBlock::ToolResult(result) if result.call_id().expose_for_wire() == "call_1" => {
                serde_json::from_slice::<Value>(result.output().canonical_bytes()).ok()
            }
            _ => None,
        })
        .expect("prior read result remains represented after the source changes");
    assert_eq!(retained["cached_observation_stale"], true);
    assert_eq!(retained["path"], "file.txt");
}

#[test]
fn visible_reuse_receipt_keeps_its_real_page_without_growing_a_receipt_chain() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    fs::write(f.workspace.join("file.txt"), "ONE_ORIGINAL_FACT").unwrap();
    let (_, failed, source) =
        f.execute("workspace_read", &json!({"path":"file.txt","max_bytes":4096}), true).unwrap();
    assert!(!failed);
    let args = json!({"source":format!("obs:{source:06}"),"offset":0,"max_bytes":4096});
    let (_, failed, _) = f.execute("context_recover", &args, true).unwrap();
    assert!(!failed);
    for _ in 0..5 {
        f.reopen().unwrap();
        let mut memory = f.memory().unwrap().lock().unwrap();
        let messages = memory
            .prepare_view_with_governing(&profile(8192, 256).unwrap(), &[], None, None, None, None)
            .unwrap();
        let actual_pages=messages.iter().flat_map(Message::content).filter(|block| matches!(block,ContentBlock::ToolResult(result) if serde_json::from_slice::<Value>(result.output().canonical_bytes()).unwrap()["representation"]=="semantic-json-v1")).count();
        assert_eq!(actual_pages, 1, "reuse must retain one real page, not a receipt chain");
        memory.activity.observe_view(&messages);
        drop(memory);
        let (value, failed, _) = f.execute("context_recover", &args, true).unwrap();
        assert!(!failed);
        assert_eq!(value["observation_reuse"]["state"], "already_visible", "{value}");
    }
    // A new task action ends the active retrieval dependency; old receipt bodies are optional.
    f.execute("workspace_read", &json!({"path":"file.txt","max_bytes":4096}), true).unwrap();
    let mut memory = f.memory().unwrap().lock().unwrap();
    let messages = memory
        .prepare_view_with_governing(&profile(8192, 256).unwrap(), &[], None, None, None, None)
        .unwrap();
    assert!(!messages.iter().flat_map(Message::content).any(|block|matches!(block,ContentBlock::ToolResult(result) if serde_json::from_slice::<Value>(result.output().canonical_bytes()).unwrap()["representation"]=="semantic-json-v1")));
    drop(memory);
}

#[test]
fn a_larger_capacity_does_not_automatically_fill_with_obsolete_file_versions() {
    let mut f = Fixture::new(true, true, HarnessRole::Writer).unwrap();
    let mut first = None;
    for revision in 0..24 {
        fs::write(
            f.workspace.join("file.txt"),
            format!("VERSION_{revision} {}", "old body ".repeat(8000)),
        )
        .unwrap();
        let (_, failed, id) = f
            .execute("workspace_read", &json!({"path":"file.txt","max_bytes":32768}), true)
            .unwrap();
        assert!(!failed);
        if first.is_none() {
            first = Some(id);
        }
    }
    let scope = crate::local_context::tools::hex(
        f.memory().unwrap().lock().unwrap().store.scope_digest().as_bytes(),
    );
    f.execute("context_sources", &json!({"scope":scope,"cursor":null,"max_bytes":1024}), true)
        .unwrap();
    f.reopen().unwrap();
    let mut memory = f.memory().unwrap().lock().unwrap();
    let messages = memory
        .prepare_view_with_governing(&profile(828_144, 256).unwrap(), &[], None, None, None, None)
        .unwrap();
    let estimated = peritus_agent::estimate_developer_request_tokens(&messages, &[]);
    assert!(
        estimated < 100_000,
        "a small live working set must not become an archive-sized request: {estimated}"
    );
    let rendered = messages
        .iter()
        .flat_map(Message::content)
        .filter_map(
            |b| if let ContentBlock::Text(t) = b { Some(t.expose_for_wire()) } else { None },
        )
        .collect::<String>();
    assert!(rendered.contains("VERSION_23"));
    assert!(
        rendered.contains(&"old body ".repeat(3000)),
        "current valid evidence must be able to exceed the optional history budget"
    );
    drop(memory);
    let (older, failed, _) = f
        .execute(
            "context_recover",
            &json!({"source":format!("obs:{:06}",first.unwrap()),"offset":0,"max_bytes":4096}),
            true,
        )
        .unwrap();
    assert!(!failed, "{older}");
    assert!(older["data"].as_str().unwrap().contains("VERSION_0"));
}
