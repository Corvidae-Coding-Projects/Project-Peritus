//! Stateful replay of a new input arriving after design source capture.

use super::*;
use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, ToolCallId,
    WireDialect,
};
use peritus_provider_core::{CancellationToken, ProcessLimits};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

struct Inputs {
    received: AtomicU64,
    incorporated: AtomicU64,
}
impl crate::ConversationView for Inputs {
    fn revision(&self) -> u64 {
        self.received.load(Ordering::Acquire)
    }
    fn incorporated_revision(&self) -> u64 {
        self.incorporated.load(Ordering::Acquire)
    }
    fn render(&self) -> String {
        "Publish atomic state; retain the previous generation. Preserve this requirement after steering.".into()
    }
    fn source_design_user_text(&self, revision: u64) -> Result<Option<String>, String> {
        if revision != self.incorporated_revision() {
            return Err("input is not incorporated".into());
        }
        Ok(Some(if revision == 1 { self.render() } else { "Also preserve compatibility.".into() }))
    }
}

fn input(root: &std::path::Path, conversation: Arc<Inputs>) -> ProductRunInput {
    let run_id = RunId::new([0x11; 16]).unwrap();
    let profile = ProviderProfile::new(
        ProviderProfileId::new([0x22; 16]).unwrap(),
        1,
        ProviderName::new("openai".into()).unwrap(),
        ModelName::new("gpt-6.1-sol".into()).unwrap(),
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).unwrap(),
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 128, 8, 1).unwrap(),
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .unwrap();
    // This state replay never performs inference. A pinned real executable is required only
    // to construct the otherwise-unused production role adapter in ProductRunInput.
    let provider = Arc::new(CodexRuntimeProvider::new(
        CodexRuntimeConfig::new(
            CodexExecutable::pin(std::env::current_exe().unwrap()).unwrap(),
            profile,
            ProcessLimits::new(1024 * 1024, 1024 * 1024, 65536, Duration::from_secs(30)).unwrap(),
        )
        .unwrap(),
    ));
    let workspace = root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let processes =
        peritus_process::ProcessStore::open(root.join("processes"), &workspace).unwrap();
    ProductRunInput {
        workspace_kind: crate::ProductWorkspaceKind::Managed,
        run_id,
        workspace_id: WorkspaceId::new([0x33; 16]).unwrap(),
        workspace_root: workspace.clone(),
        source_root: workspace.clone(),
        trace_path: root.join("run.trace"),
        command_runtime: crate::CommandRuntime::open(
            root.join("router"),
            &workspace,
            run_id,
            processes,
        )
        .unwrap(),
        module_jobs: None,
        finding_state: String::new(),
        task: "Publish atomic state".into(),
        max_elapsed: None,
        delivery_scope: crate::ProductDeliveryScope::WorkspaceChanges,
        conversation,
        providers: crate::RoleProviders {
            writer: provider.clone(),
            reviewer: provider.clone(),
            fixer: provider,
            fallbacks: vec![],
        },
        cancelled: Arc::new(AtomicBool::new(false)),
        provider_cancellation: CancellationToken::new(),
        resume: None,
    }
}

fn call(value: &serde_json::Value) -> CompletedToolCall {
    named_call(TOOL, value)
}

fn named_call(name: &str, value: &serde_json::Value) -> CompletedToolCall {
    let limits = ProtocolLimits::PRODUCTION;
    CompletedToolCall::new(
        ToolCallId::new("design-replay".into()).unwrap(),
        ToolName::new(name.into()).unwrap(),
        CanonicalJson::parse(&value.to_string(), JsonBounds::value(limits)).unwrap(),
    )
    .unwrap()
}

// Model-facing mutations bind both the observed source and predecessor version.
fn expert_call(
    input: &ProductRunInput,
    name: &str,
    value: &serde_json::Value,
) -> CompletedToolCall {
    let mut value = value.clone();
    let source =
        SourceDesignSnapshot::open(&input.trace_path, input.run_id, input.workspace_id).unwrap();
    value["source_sha256"] =
        serde_json::json!(crate::encode_hex(source.metadata().sha256().as_bytes()));
    if name == crate::design::expert_tools::RECOVER && value.get("expected_version").is_none() {
        value["expected_version"] = serde_json::json!(1);
    }
    named_call(name, &value)
}

fn proposal(version: u64) -> serde_json::Value {
    serde_json::json!({"action":"publish_ready","expected_version":version,
    "markdown":"# Atomic-state design\n\nPreserve the previous generation on failure. Implement atomic publication and validate restoration through actual filesystem operations.",
    "records":{
      "domains":[{"id":"d","discipline":"durable state","applicability":"atomic publication","expected_sources":["user request"],"risks":["partial write"],"research_required":true,"external_authority_required":false,"internal_mechanical":null,"research_skip_rationale":null}],
      "evidence":[{"id":"e","domain_ids":["d"],"obligation_ids":["o"],"title":"User durability requirement","version":null,"authority":"source_design","reference":{"kind":"source_range","start":0,"end":20}}],
      "obligations":[{"id":"o","class":"user","statement":"Publish atomic state","origin":"user source","rationale":"preserve previous state","applicability":"every publication","affected_contracts":["state"],"failure_behavior":"retain prior state","evidence_ids":["e"],"acceptance_ids":["a"],"status":"supported"}],
      "acceptance_mappings":[{"id":"a","criterion":"A failed publication retains prior bytes","obligation_ids":["o"]}],
      "material_question":null,"expanded_markdown":null,"partial_markdown":null,"limitations":[],
      "change_ledger":[{"id":"c","kind":"preserved","source_evidence_ids":["e"],"obligation_ids":["o"],"summary":"Retain the atomic-state requirement"}]
    }})
}

#[test]
fn steering_refreshes_stale_design_and_preserves_proposal_through_restart() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation.clone());
    let mut session = ResearchSession::automatic(&input).unwrap();
    let old_source = std::fs::read(input.trace_path.with_extension("design-source.json")).unwrap();
    let old_expansion =
        std::fs::read(input.trace_path.with_extension("design-expansion.json")).unwrap();
    conversation.received.store(2, Ordering::Release);
    assert!(!session.synchronize(&input).unwrap(), "pending input must not be captured");
    conversation.incorporated.store(2, Ordering::Release);
    let submitted = proposal(1);
    let stale = session.execute(&input, &call(&submitted), "grounding").unwrap();
    assert!(stale.is_error);
    assert_eq!(session.expansion.as_ref().unwrap().input_revision(), 2);
    assert_eq!(session.expansion.as_ref().unwrap().version(), 2);
    let updated_source = session.source.as_ref().unwrap();
    assert!(
        updated_source.text().starts_with(&crate::ConversationView::render(conversation.as_ref()))
    );
    assert!(updated_source.text().ends_with("Also preserve compatibility."));
    for (name, bytes) in
        [("design-source.json", old_source), ("design-expansion.json", old_expansion)]
    {
        let digest = crate::encode_hex(peritus_codec::sha256(&bytes).as_bytes());
        assert_eq!(
            std::fs::read(input.trace_path.with_extension(format!("{name}.{digest}.history")))
                .unwrap(),
            bytes
        );
    }
    drop(session);
    let mut restarted = ResearchSession::automatic(&input).unwrap();
    let observed = restarted.execute(&input, &call(&serde_json::json!({"action":"observe","expected_version":null,"records":null,"markdown":null})), "grounding").unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(observed.output.canonical_bytes()).unwrap();
    assert_eq!(value["retained_proposal"]["arguments"], submitted);
    assert_eq!(value["source"]["source_design"]["input_revision"], 2);
    let mut exact_source = String::new();
    let mut offset = 0;
    loop {
        let page = restarted
            .read_source(
                &input,
                &named_call(SOURCE_TOOL, &serde_json::json!({"offset":offset,"max_bytes":17})),
            )
            .unwrap();
        assert!(!page.is_error);
        let page: serde_json::Value =
            serde_json::from_slice(page.output.canonical_bytes()).unwrap();
        assert_eq!(page["source_design"]["authority"], "evidence_only");
        exact_source.push_str(page["page"]["text"].as_str().unwrap());
        match page["page"]["next_offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(exact_source, restarted.source.as_ref().unwrap().text());
    assert!(
        restarted
            .read_source(
                &input,
                &named_call(SOURCE_TOOL, &serde_json::json!({"offset":0,"max_bytes":32769}))
            )
            .unwrap()
            .is_error
    );
    let ready = restarted.execute(&input, &call(&proposal(2)), "grounding").unwrap();
    assert!(!ready.is_error, "{}", ready.output.to_wire_string());
    assert_eq!(restarted.expansion.as_ref().unwrap().stage(), ExpansionStage::Ready);
    assert!(input.trace_path.with_extension("design.md").is_file());
}

#[test]
fn repeated_observation_cannot_satisfy_a_required_design_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    let grounded = tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"", "depth":1})))
        .unwrap();
    assert!(!grounded.is_error);
    tools.checkpoint_required = true;
    assert_eq!(tools.required_tool_name(), Some(crate::design::expert_tools::PLAN));
    for _ in 0..2 {
        tools.prepare_model_request().unwrap();
        tools.model_request_admitted();
        let before = tools.research.expansion.as_ref().unwrap().version();
        tools.execute(&call(&serde_json::json!({"action":"observe","expected_version":null,"records":null,"markdown":null}))).unwrap();
        assert_eq!(tools.research.expansion.as_ref().unwrap().version(), before);
        tools.model_response_completed();
    }
    assert!(tools.continuation_blocker().is_some());
}

#[test]
fn repeated_file_reads_require_a_checkpoint_but_fresh_evidence_resets_the_streak() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    std::fs::write(input.workspace_root.join("first.rs"), "pub fn first() {}\n").unwrap();
    std::fs::write(input.workspace_root.join("second.rs"), "pub fn second() {}\n").unwrap();
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"", "depth":1})))
        .unwrap();
    let _ = tools.take_progress_feedback();
    let first = named_call("workspace_read", &serde_json::json!({"path":"first.rs"}));
    let second = named_call("workspace_read", &serde_json::json!({"path":"second.rs"}));
    let cached = tools.execute(&first).unwrap();
    let _ = tools.take_progress_feedback();
    for _ in 0..2 {
        assert!(!tools.execute(&first).unwrap().is_error);
        assert!(tools.take_progress_feedback().is_some());
        assert!(!tools.checkpoint_required);
    }
    assert!(!tools.execute(&second).unwrap().is_error);
    assert!(tools.take_progress_feedback().is_none());
    assert_eq!(tools.repeated_research_batches, 0);
    for _ in 0..3 {
        assert!(!tools.execute(&second).unwrap().is_error);
        assert!(tools.take_progress_feedback().is_some());
    }
    assert_eq!(tools.required_tool_name(), Some(crate::design::expert_tools::PLAN));
    assert!(tools.execute(&second).unwrap().is_error);
    assert!(tools.reuse_workspace_read(&first, &cached).unwrap().unwrap().is_error);
}

#[test]
fn required_checkpoint_does_not_block_real_progress_or_allow_further_reads() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"", "depth":1})))
        .unwrap();
    for _ in 0..24 {
        tools.prepare_model_request().unwrap();
        tools.model_request_admitted();
        tools.model_response_completed();
    }
    tools.prepare_model_request().unwrap();
    assert!(tools.checkpoint_required, "context-only loops also need durable progress");
    let denied = tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"", "depth":1})))
        .unwrap();
    assert!(denied.is_error);
    let ready = tools.execute(&call(&proposal(1))).unwrap();
    assert!(!ready.is_error, "{}", ready.output.to_wire_string());
    assert!(!tools.checkpoint_required);
    assert_eq!(tools.requests_since_checkpoint, 0);
    assert!(tools.continuation_blocker().is_none());
}

fn plan(version: u64, external: bool) -> serde_json::Value {
    serde_json::json!({"expected_version":version,"domains":[{"discipline":"Atomic state","applicability":"Publication must retain the old complete state on failure","risks":["partial publication"],"questions":[{"question":"Which publication and recovery changes preserve the prior generation?","external_authority_required":external}]}]})
}

fn amendment(version: u64, evidence: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({"expected_version":version,"question_id":"q-1-1","heading":"Atomic publication and recovery","target_section":"State lifecycle","markdown":"Publish the complete new generation atomically. Keep the previous complete generation on failure and reopen only committed state.","class":"mandatory_inferred","rationale":"Avoid partial state after interruption","affected_contracts":["state store"],"failure_behavior":"Retain the previous generation","acceptance":["Interrupt publication and independently verify the old complete bytes remain readable"],"evidence":[evidence]})
}

fn source_evidence() -> serde_json::Value {
    serde_json::json!({"title":"Exact source durability requirement","version":null,"authority":"source_design","reference":{"kind":"source_range","start":0,"end":20}})
}

#[test]
fn imported_host_source_rows_refresh_without_losing_file_evidence_links() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let mut input = input(directory.path(), conversation);
    input.task = "Read the current design.md".into();
    std::fs::write(
        input.workspace_root.join("design.md"),
        "# Supplied design\n\nPreserve all existing behavior.\n",
    )
    .unwrap();
    let research = ResearchSession::automatic(&input).unwrap();
    let source = research.source.as_ref().unwrap();
    let mut pass = crate::design::expert_pass::ExpertPass::load(&input, source).unwrap();
    let mut imported = serde_json::to_value(pass.records(source).unwrap()).unwrap();
    imported["evidence"][0]["reference"]["end"] = serde_json::json!(1);
    pass.imported_records = Some(imported);
    let records = pass.records(source).unwrap();
    let value = serde_json::to_value(&records).unwrap();
    let source_e = value["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "expert-source-e")
        .unwrap();
    assert_eq!(source_e["reference"]["end"], source.metadata().bytes());
    let source_o = value["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == "expert-source-o")
        .unwrap();
    assert!(
        source_o["evidence_ids"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("expert-source-file-e"))
    );
    crate::design::expansion::validate_pass_records(
        &input,
        source,
        research.expansion.as_ref().unwrap(),
        &records,
    )
    .unwrap();
}

#[test]
fn denied_read_permission_cannot_publish_or_modify_a_saved_expert_pass() {
    struct Blocked;
    impl crate::ConversationView for Blocked {
        fn revision(&self) -> u64 {
            1
        }
        fn render(&self) -> String {
            "Publish atomic state".into()
        }
        fn effective_permissions(&self) -> crate::control::HostPermissions {
            crate::control::HostPermissions::none()
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let mut input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    tools
        .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan(1, false)))
        .unwrap();
    drop(tools);
    let before = std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap();
    input.conversation = Arc::new(Blocked);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    let denied = tools
        .execute(&expert_call(
            &input,
            crate::design::expert_tools::PUBLISH,
            &serde_json::json!({"expected_version":2,"review":"This must be denied"}),
        ))
        .unwrap();
    assert!(denied.is_error);
    assert!(denied.output.to_wire_string().contains("permission"));
    assert_eq!(
        before,
        std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap()
    );
    assert!(!input.trace_path.with_extension("design.md").exists());
}

#[cfg(unix)]
#[test]
fn automatic_source_selection_rejects_symlinks_without_reading_the_target() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let mut input = input(directory.path(), conversation);
    input.task = "Read the current design.md".into();
    let target = directory.path().join("outside.md");
    std::fs::write(&target, "Private original input").unwrap();
    std::os::unix::fs::symlink(&target, input.workspace_root.join("design.md")).unwrap();
    assert!(
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).is_err()
    );
    assert_eq!(std::fs::read_to_string(target).unwrap(), "Private original input");
    assert!(!input.trace_path.with_extension("design-expert.json").exists());
}

#[test]
fn a_matching_draft_version_cannot_apply_work_from_a_previous_source() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation.clone());
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    let stale = expert_call(&input, crate::design::expert_tools::PLAN, &plan(1, false));
    conversation.received.store(2, Ordering::Release);
    conversation.incorporated.store(2, Ordering::Release);
    tools.prepare_model_request().unwrap();
    assert!(tools.execute(&stale).unwrap().is_error);
    assert!(!input.trace_path.with_extension("design-expert.json").exists());
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan(1, false)))
            .unwrap()
            .is_error
    );
}

#[test]
fn expert_question_contexts_have_distinct_durable_trace_identities() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    for args in [
        vec!["init", "--quiet"],
        vec![
            "-c",
            "user.name=Qualification",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "--quiet",
            "-m",
            "Baseline",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&input.workspace_root)
                .status()
                .unwrap()
                .success()
        );
    }
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    let scope = || {
        crate::local_context::LocalContextHandle::for_run(&input, "designer")
            .unwrap()
            .unwrap()
            .scope_digest()
            .unwrap()
    };
    let intake = scope();
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan(1, false)))
            .unwrap()
            .is_error
    );
    let question = scope();
    assert_ne!(intake, question, "a new directory alone cannot isolate trace replay");
    assert_eq!(question, scope(), "restart preserves the exact active question lineage");
    assert!(
        !tools
            .execute(&expert_call(
                &input,
                crate::design::expert_tools::AMEND,
                &amendment(2, &source_evidence())
            ))
            .unwrap()
            .is_error
    );
    let review = scope();
    assert_ne!(review, question);
    assert_ne!(review, intake);
    assert_eq!(review, scope());
}

#[test]
fn malformed_retained_evidence_can_be_repaired_without_resending_the_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    let mut candidate = proposal(1);
    let correct = candidate["records"]["evidence"][0].clone();
    candidate["records"]["evidence"][0]["reference"]["end"] = serde_json::json!(999_999);
    crate::design::retention::proposal(&input, &candidate).unwrap();
    assert!(
        !tools
            .execute(&expert_call(
                &input,
                crate::design::expert_tools::RECOVER,
                &serde_json::json!({})
            ))
            .unwrap()
            .is_error
    );
    let publish =
        serde_json::json!({"expected_version":2,"review":"Review against current exact source"});
    assert!(
        tools
            .execute(&expert_call(&input, crate::design::expert_tools::PUBLISH, &publish))
            .unwrap()
            .is_error,
        "unapproved import must never bypass current evidence validation"
    );
    let patch = serde_json::json!({"expected_version":2,"markdown_changes":[],"record_changes":[{"collection":"evidence","id":"e","replacement_json":correct.to_string()}]});
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PATCH, &patch))
            .unwrap()
            .is_error
    );
    let published = tools.execute(&expert_call(&input, crate::design::expert_tools::PUBLISH, &serde_json::json!({"expected_version":3,"review":"Repaired the exact source range; all original requirements and remaining candidate content are preserved."}))).unwrap();
    assert!(!published.is_error, "{}", published.output.to_wire_string());
}

#[test]
fn expert_pass_saves_amendments_restarts_and_publishes_without_full_resubmission() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    let plan = plan(1, false);
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan))
            .unwrap()
            .is_error
    );
    let committed = std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap();
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan))
            .unwrap()
            .is_error
    );
    assert_eq!(
        std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap(),
        committed,
        "same request must be idempotent"
    );
    let change = amendment(2, &source_evidence());
    let result =
        tools.execute(&expert_call(&input, crate::design::expert_tools::AMEND, &change)).unwrap();
    assert!(!result.is_error, "{}", result.output.to_wire_string());
    drop(tools);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    let context = tools.context();
    assert!(context.contains("Atomic publication and recovery"));
    assert!(
        !context.contains("Interrupt publication"),
        "acceptance bodies remain explicitly pageable"
    );
    let stale = tools
        .execute(&expert_call(
            &input,
            crate::design::expert_tools::AMEND,
            &amendment(1, &source_evidence()),
        ))
        .unwrap();
    assert!(stale.is_error);
    let publish = serde_json::json!({"expected_version":3,"review":"The complete source, recovery amendment, failure behavior and independent acceptance preserve every user requirement."});
    let published = tools
        .execute(&expert_call(&input, crate::design::expert_tools::PUBLISH, &publish))
        .unwrap();
    assert!(!published.is_error, "{}", published.output.to_wire_string());
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PUBLISH, &publish))
            .unwrap()
            .is_error
    );
    let markdown = std::fs::read_to_string(input.trace_path.with_extension("design.md")).unwrap();
    assert!(markdown.contains("Preserve this requirement after steering."));
    assert!(markdown.contains("Interrupt publication and independently verify"));
    assert_eq!(tools.research.expansion.as_ref().unwrap().stage(), ExpansionStage::Ready);
}

#[test]
fn expert_pass_requires_primary_evidence_and_retains_useful_blocked_work() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan(1, true)))
            .unwrap()
            .is_error
    );
    let before = std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap();
    let unsupported = tools
        .execute(&expert_call(
            &input,
            crate::design::expert_tools::AMEND,
            &amendment(2, &source_evidence()),
        ))
        .unwrap();
    assert!(unsupported.is_error);
    assert_eq!(
        std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap(),
        before
    );
    let mut blocked = amendment(2, &source_evidence());
    blocked["class"] = serde_json::json!("unknown");
    blocked["rationale"] = serde_json::json!(
        "Required primary evidence is unavailable; retain the recovery design pending that source."
    );
    let retained =
        tools.execute(&expert_call(&input, crate::design::expert_tools::AMEND, &blocked)).unwrap();
    assert!(!retained.is_error, "{}", retained.output.to_wire_string());
    assert_eq!(tools.research.expansion.as_ref().unwrap().stage(), ExpansionStage::ResearchLimited);
    let publication = tools
        .execute(&expert_call(
            &input,
            crate::design::expert_tools::PUBLISH,
            &serde_json::json!({"expected_version":3,"review":"Source unavailable"}),
        ))
        .unwrap();
    assert!(publication.is_error);
    assert!(tools.finish("Required source is unavailable.").is_err());
    let partial =
        std::fs::read_to_string(input.trace_path.with_extension("design.partial.md")).unwrap();
    assert!(partial.contains("Atomic publication and recovery"));
    assert!(!input.trace_path.with_extension("design.md").exists());
}

#[test]
fn expert_source_selection_preserves_file_bytes_and_detects_later_source_changes() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let mut input = input(directory.path(), conversation);
    input.task = "Read the current design and improve its recovery behavior".into();
    let original = b"# Existing design\r\n\r\nKeep all original behavior.\r\n";
    std::fs::write(input.workspace_root.join("design.md"), original).unwrap();
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    let source = tools.research.source.as_ref().unwrap();
    assert_eq!(source.metadata().origin(), SourceDesignOrigin::SuppliedWorkspaceDesign);
    assert!(source.text().contains(std::str::from_utf8(original).unwrap()));
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    assert!(
        !tools
            .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan(1, false)))
            .unwrap()
            .is_error
    );
    assert!(
        !tools
            .execute(&expert_call(
                &input,
                crate::design::expert_tools::AMEND,
                &amendment(2, &source_evidence())
            ))
            .unwrap()
            .is_error
    );
    assert_eq!(std::fs::read(input.workspace_root.join("design.md")).unwrap(), original);
    std::fs::write(input.workspace_root.join("design.md"), b"# New source\n").unwrap();
    let published = tools
        .execute(&expert_call(
            &input,
            crate::design::expert_tools::PUBLISH,
            &serde_json::json!({"expected_version":3,"review":"Review source and amendments"}),
        ))
        .unwrap();
    assert!(published.is_error, "changed source must invalidate publication");
    assert!(!input.trace_path.with_extension("design.md").exists());
}

#[test]
fn checkpoint_guard_counts_informed_responses_not_parallel_calls_or_view_retries() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    for _ in 0..30 {
        tools.observe_model_context(&[]).unwrap();
    }
    assert_eq!(tools.requests_since_checkpoint, 0);
    tools.checkpoint_required = true;
    for _ in 0..4 {
        assert!(
            tools
                .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
                .unwrap()
                .is_error
        );
    }
    tools.model_response_completed();
    assert!(tools.continuation_blocker().is_none(), "an old response has not seen the requirement");
    for attempt in 1..=2 {
        tools.prepare_model_request().unwrap();
        assert_eq!(tools.required_tool_name(), Some(crate::design::expert_tools::PLAN));
        tools.model_request_admitted();
        for _ in 0..4 {
            tools.execute(&call(&serde_json::json!({"action":"observe","expected_version":null,"records":null,"markdown":null}))).unwrap();
        }
        tools.model_response_completed();
        assert_eq!(tools.unchanged_checkpoints, attempt);
    }
    assert!(tools.continuation_blocker().is_some());
}

#[test]
fn recovered_large_candidate_accepts_small_atomic_patches_and_preserves_all_other_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation);
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    let mut candidate = proposal(1);
    let large = format!(
        "# Retained design\n\n{}\nUnique correction: OLD-END.\n",
        "Preserve the original behavior and explicit recovery requirements.\n".repeat(1600)
    );
    candidate["markdown"] = serde_json::json!(large);
    crate::design::retention::proposal(&input, &candidate).unwrap();
    let recovered = tools
        .execute(&expert_call(&input, crate::design::expert_tools::RECOVER, &serde_json::json!({})))
        .unwrap();
    assert!(!recovered.is_error, "{}", recovered.output.to_wire_string());
    assert!(
        recovered.output.canonical_bytes().len() < 4096,
        "recovery returns a receipt rather than the entire candidate"
    );
    let patch = serde_json::json!({"expected_version":2,"markdown_changes":[{"before":"Unique correction: OLD-END.","after":"Unique correction: NEW-END."}],"record_changes":[]});
    assert!(patch.to_string().len() < 512);
    let patched =
        tools.execute(&expert_call(&input, crate::design::expert_tools::PATCH, &patch)).unwrap();
    assert!(!patched.is_error, "{}", patched.output.to_wire_string());
    let saved = std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap();
    let mut invalid = patch;
    invalid["expected_version"] = serde_json::json!(3);
    assert!(
        tools
            .execute(&expert_call(&input, crate::design::expert_tools::PATCH, &invalid))
            .unwrap()
            .is_error
    );
    assert_eq!(
        std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap(),
        saved
    );
    let result=tools.execute(&expert_call(&input, crate::design::expert_tools::PUBLISH,&serde_json::json!({"expected_version":3,"review":"Reviewed the complete preserved source and candidate. The exact correction preserves every other requirement and current evidence."}))).unwrap();
    assert!(!result.is_error, "{}", result.output.to_wire_string());
    let markdown = std::fs::read_to_string(input.trace_path.with_extension("design.md")).unwrap();
    assert!(markdown.contains(&large.replace("OLD-END", "NEW-END")));
    assert!(!markdown.contains("Unique correction: OLD-END."));
}

#[test]
fn steering_recovers_saved_expert_amendments_without_research_restart() {
    let directory = tempfile::tempdir().unwrap();
    let conversation =
        Arc::new(Inputs { received: AtomicU64::new(1), incorporated: AtomicU64::new(1) });
    let input = input(directory.path(), conversation.clone());
    let mut tools =
        DesignTools::new(&input, input.read_only_workspace_tools().for_design_expansion()).unwrap();
    tools
        .execute(&named_call("workspace_list", &serde_json::json!({"path":"","depth":1})))
        .unwrap();
    tools
        .execute(&expert_call(&input, crate::design::expert_tools::PLAN, &plan(1, false)))
        .unwrap();
    let recorded = tools
        .execute(&expert_call(
            &input,
            crate::design::expert_tools::AMEND,
            &amendment(2, &source_evidence()),
        ))
        .unwrap();
    assert!(!recorded.is_error);
    conversation.received.store(2, Ordering::Release);
    conversation.incorporated.store(2, Ordering::Release);
    tools.prepare_model_request().unwrap();
    let recovered = tools
        .execute(&expert_call(&input, crate::design::expert_tools::RECOVER, &serde_json::json!({})))
        .unwrap();
    assert!(!recovered.is_error, "{}", recovered.output.to_wire_string());
    let pass: serde_json::Value = serde_json::from_slice(
        &std::fs::read(input.trace_path.with_extension("design-expert.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(pass["input_revision"], 2);
    assert_eq!(pass["amendments"].as_array().unwrap().len(), 1);
    assert_eq!(pass["amendments"][0]["heading"], "Atomic publication and recovery");
    let published=tools.execute(&expert_call(&input, crate::design::expert_tools::PUBLISH,&serde_json::json!({"expected_version":2,"review":"The retained amendment still applies, and the complete new source preserves compatibility and original durability behavior."}))).unwrap();
    assert!(!published.is_error, "{}", published.output.to_wire_string());
}
