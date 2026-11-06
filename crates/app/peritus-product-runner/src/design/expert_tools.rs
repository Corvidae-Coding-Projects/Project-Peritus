//! Small expert interfaces; identities, complete records, and publication stay host-owned.

use peritus_agent::{DeveloperLoopError, DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};
use serde_json::{Value, json};

use super::{
    SourceDesignSnapshot,
    expert_pass::{Amendment, Domain, ExpertPass, failure},
    research::ResearchSession,
};
use crate::{
    ProductRunInput, ProductRunnerError, control::PermissionCapability,
    developer_tools::WorkspaceDeveloperTools,
};

pub(super) const PLAN: &str = "design_research_plan";
pub(super) const AMEND: &str = "design_amendment";
pub(super) const PUBLISH: &str = "design_publish";
pub(super) const READ: &str = "design_pass_read";
pub(super) const SELECT: &str = "design_source_select";
pub(super) const RECOVER: &str = "design_recover";
pub(super) const PATCH: &str = "design_candidate_patch";

pub(super) const POLICY: &str = r"# Domain expert design pass
Produce an implementation-ready design by doing useful expert work: inspect the supplied design and relevant current code, identify concrete domain questions, research authoritative answers, and save actionable amendments. Preserve every source requirement, exclusion, value and current user instruction. The full domain-obligation policy above governs this pass.

The host supplies current source identity, draft_version, unresolved questions and saved amendment headings on every request. Do not request repeated state observations. If the user supplied or named a design document, ensure the actual document is selected with design_source_select; design_pass_read kind=source provides exact source pages. Read the complete selected source. Use one startup grounding check, then focused repository reads for concrete questions. Respect explicit deferred-testing instructions.

Start with design_research_plan: identify applicable domains and a small set of material questions, not an inventory of every possible topic. When outside knowledge affects correctness, use web_search to find primary/official guidance, open the relevant source with web_open, and read the needed ranges. A known authoritative URL can be opened directly. Search snippets are discovery, not evidence. Resolve routine choices from evidence and reversible supported defaults; ask only genuinely user-owned material questions.

As soon as a question has a justified answer, save design_amendment with the actual design change, target section, rationale, affected contracts, failure behavior, exact evidence and observable acceptance. The host assigns and cross-links all ledger IDs and captures web metadata. Do not reproduce a complete structured ledger. Save useful work before researching unrelated questions. Further research must address a named unresolved question or a newly discovered necessary obligation; append that question to the plan first. If evidence is unavailable, retain an unknown/conflict amendment and report its precise limitation through the research_limited/wait_for_user boundary; never fabricate successful research.

If retained candidate work exists, use design_recover once, inspect the relevant draft pages, reconcile current instructions and evidence, and update only affected questions/amendments. Prior candidate work is unapproved evidence. Every question must be resolved and source obligations preserved before design_publish. Call design_publish with the current draft_version and your substantive review, without resending unchanged Markdown or records. The host validates and publishes every required checkpoint. After successful publication, return a concise handoff; authorized coding proceeds automatically. No workspace edits, commands, commits, or permission expansion occur in this design stage.";

pub fn owns(name: &str) -> bool {
    matches!(name, PLAN | AMEND | PUBLISH | READ | SELECT | RECOVER | PATCH)
}

mod schema;
pub use schema::definitions;

pub fn context(
    input: &ProductRunInput,
    research: &ResearchSession,
) -> Result<String, ProductRunnerError> {
    let source = research.source.as_ref().ok_or_else(|| failure("exact source unavailable"))?;
    let pass = ExpertPass::load(input, source)?;
    let recovery = pass.amendments.is_empty()
        && pass.imported_records.is_none()
        && (input.trace_path.with_extension("design-proposal.json").is_file()
            || input.trace_path.with_extension("design-expert-prior.json").is_file());
    let published = research
        .expansion
        .as_ref()
        .is_some_and(|state| state.stage() == super::ExpansionStage::Ready);
    let mut projection = pass.observation();
    if recovery && !published {
        projection["next_action"] = json!(
            "Use design_recover before creating a new worklist. Reconcile the saved draft and retained evidence with current instructions, then change only affected amendments or candidate fields."
        );
    }
    Ok(format!(
        "\n# Current expert design pass\n{}\nPublished: {published}. {} Retained candidate work available: {recovery}. Use design_recover once when applicable, then reconcile it. The host owns IDs, evidence metadata, cross-links, record assembly and publication.{}\n",
        projection,
        if published {
            "Return the final handoff now."
        } else {
            "Research named questions and record useful amendments; repeated state observations are not work."
        },
        research.context()
    ))
}

pub fn execute(
    input: &ProductRunInput,
    research: &mut ResearchSession,
    workspace: &mut WorkspaceDeveloperTools,
    call: &CompletedToolCall,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    let result = execute_inner(input, research, workspace, call);
    let is_error = result.is_err();
    let value = result.unwrap_or_else(|error| json!({"error":error.detail()}));
    Ok(DeveloperToolObservation {
        output: CanonicalJson::parse(
            &value.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?,
        is_error,
    })
}

fn execute_inner(
    input: &ProductRunInput,
    research: &mut ResearchSession,
    workspace: &mut WorkspaceDeveloperTools,
    call: &CompletedToolCall,
) -> Result<Value, ProductRunnerError> {
    crate::execution::check_cancelled(input)?;
    if !input.conversation.effective_permissions().allows(PermissionCapability::Read) {
        return Err(failure("design work is disabled by current read permission"));
    }
    workspace.grounding().validate().map_err(failure)?;
    research.synchronize(input)?;
    let _lock = ExpertPass::lock(input)?;
    let source = research.source.as_ref().ok_or_else(|| failure("source unavailable"))?.clone();
    let mut pass = ExpertPass::load(input, &source)?;
    let args: Value = serde_json::from_slice(call.arguments().canonical_bytes())
        .map_err(|e| failure(e.to_string()))?;
    let digest = crate::encode_hex(
        peritus_codec::sha256(format!("{}:{}", call.name().as_str(), args).as_bytes()).as_bytes(),
    );
    if pass.last_call.as_ref() == Some(&digest) {
        return Ok(pass.observation());
    }
    if call.name().as_str() == READ {
        return read(input, &source, &pass, &args);
    }
    if call.name().as_str() == SELECT {
        return select_source(input, research, workspace, &source, &pass, &args);
    }
    check_predecessor(&pass, &args)?;
    if research
        .expansion
        .as_ref()
        .is_some_and(|state| state.stage() == super::ExpansionStage::Ready)
        && call.name().as_str() != PUBLISH
    {
        return Err(failure(
            "published expert passes are immutable; incorporate newer user input before revising the design",
        ));
    }
    match call.name().as_str() {
        PLAN => record_plan(&mut pass, &args)?,
        AMEND => record_amendment(input, &source, research, &mut pass, &args)?,
        RECOVER => recover_candidate(input, &source, research, &mut pass)?,
        PATCH => patch_candidate(input, &source, research, &mut pass, &args)?,
        PUBLISH => return publish_candidate(input, research, workspace, &source, &pass, &args),
        _ => return Err(failure("unknown expert design operation")),
    }
    pass.commit(input, digest)?;
    if call.name().as_str() == AMEND && pass.amendments.iter().any(|a| a.class == "unknown") {
        let mut records =
            serde_json::to_value(pass.records(&source)?).map_err(|e| failure(e.to_string()))?;
        records["limitations"] = json!(
            pass.amendments
                .iter()
                .filter(|a| a.class == "unknown")
                .map(|a| format!("{}: {}", a.heading, a.rationale))
                .collect::<Vec<_>>()
        );
        let version =
            research.expansion.as_ref().ok_or_else(|| failure("expansion unavailable"))?.version();
        let limited = tool_call(
            super::research::TOOL,
            &json!({"action":"research_limited","expected_version":version,"records":records,"markdown":null}),
        )?;
        let result = research
            .execute(input, &limited, &workspace.grounding().markdown())
            .map_err(|e| failure(e.to_string()))?;
        if result.is_error {
            return Err(failure(result.output.to_wire_string()));
        }
    }
    Ok(pass.observation())
}

fn select_source(
    input: &ProductRunInput,
    research: &mut ResearchSession,
    workspace: &mut WorkspaceDeveloperTools,
    source: &SourceDesignSnapshot,
    pass: &ExpertPass,
    args: &Value,
) -> Result<Value, ProductRunnerError> {
    let path = string(args, "path")?;
    let identity: Value = serde_json::from_str(source.metadata().source_identity_json())
        .map_err(|e| failure(e.to_string()))?;
    if identity["kind"] == "supplied_workspace" && identity["path"] == path {
        let sha: [u8; 32] = serde_json::from_value(identity["observed_sha256"].clone())
            .map_err(|e| failure(e.to_string()))?;
        if crate::encode_hex(&sha) == string(args, "sha256")? {
            return Ok(pass.observation());
        }
    }
    if research
        .expansion
        .as_ref()
        .is_some_and(|state| state.stage() == super::ExpansionStage::Ready)
    {
        return Err(failure("incorporate newer user input before replacing a published source"));
    }
    check_predecessor(pass, args)?;
    let read_call = tool_call("workspace_read", &json!({"path":path}))?;
    let observed = workspace.execute(&read_call).map_err(|e| failure(e.to_string()))?;
    if observed.is_error {
        return Err(failure(observed.output.to_wire_string()));
    }
    super::retention::archive(input)?;
    let selected =
        super::source::capture_supplied_workspace(input, source, path, string(args, "sha256")?)?;
    *research = ResearchSession::from_source(input, selected)?;
    Ok(ExpertPass::load(input, research.source.as_ref().expect("captured source"))?.observation())
}

fn check_predecessor(pass: &ExpertPass, args: &Value) -> Result<(), ProductRunnerError> {
    if args["expected_version"].as_u64() != Some(pass.version)
        || args["source_sha256"].as_str() != Some(pass.source.as_str())
    {
        return Err(failure(format!(
            "draft source or version changed; current draft_version={}, source_sha256={}. Reconcile the current host projection; do not restart research",
            pass.version, pass.source
        )));
    }
    Ok(())
}

fn patch_candidate(
    input: &ProductRunInput,
    source: &SourceDesignSnapshot,
    research: &ResearchSession,
    pass: &mut ExpertPass,
    args: &Value,
) -> Result<(), ProductRunnerError> {
    let markdown = pass
        .imported_markdown
        .as_mut()
        .ok_or_else(|| failure("recover a candidate before patching it"))?;
    let changes =
        args["markdown_changes"].as_array().ok_or_else(|| failure("missing Markdown changes"))?;
    let replacements =
        args["record_changes"].as_array().ok_or_else(|| failure("missing record changes"))?;
    if changes.is_empty() && replacements.is_empty() {
        return Err(failure("candidate patch has no changes"));
    }
    for change in changes {
        let before = string(change, "before")?;
        let after = string(change, "after")?;
        if before.is_empty() || before == after || markdown.matches(before).count() != 1 {
            return Err(failure("Markdown patch must replace one exact, changed span"));
        }
        *markdown = markdown.replacen(before, after, 1);
    }
    let records =
        pass.imported_records.as_mut().ok_or_else(|| failure("candidate records unavailable"))?;
    for change in replacements {
        let collection = string(change, "collection")?;
        let id = string(change, "id")?;
        if !["domains", "evidence", "obligations", "acceptance_mappings", "change_ledger"]
            .contains(&collection)
        {
            return Err(failure("unsupported record collection"));
        }
        let items = records[collection]
            .as_array_mut()
            .ok_or_else(|| failure("invalid candidate record collection"))?;
        let index = items
            .iter()
            .position(|record| record["id"] == id)
            .ok_or_else(|| failure("candidate record id is missing"))?;
        let replacement = match change["replacement_json"].as_str() {
            Some(text) => {
                Some(serde_json::from_str::<Value>(text).map_err(|e| failure(e.to_string()))?)
            }
            None if change["replacement_json"].is_null() => None,
            None => return Err(failure("invalid replacement JSON")),
        };
        let old = &items[index];
        if collection == "obligations"
            && matches!(
                old["class"].as_str(),
                Some("user" | "mandatory_inferred" | "supporting_change")
            )
            && replacement.as_ref().is_none_or(|new| {
                new["class"] != old["class"] || new["statement"] != old["statement"]
            })
        {
            return Err(failure("required candidate obligations cannot be removed or weakened"));
        }
        if let Some(replacement) = replacement {
            if replacement["id"] != id || replacement == items[index] {
                return Err(failure("record replacement needs the same id and changed content"));
            }
            items[index] = replacement;
        } else {
            items.remove(index);
        }
    }
    super::expansion::validate_pass_records(
        input,
        source,
        research.expansion.as_ref().ok_or_else(|| failure("expansion unavailable"))?,
        &pass.records(source)?,
    )
}

fn resolve_evidence(value: &Value) -> Result<Value, ProductRunnerError> {
    let mut evidence = value.clone();
    if evidence["reference"]["kind"] == "web" {
        let reference = &evidence["reference"];
        let start =
            reference["start"].as_u64().ok_or_else(|| failure("web range start missing"))?;
        let end = reference["end"].as_u64().ok_or_else(|| failure("web range end missing"))?;
        if end <= start || end - start > 1_048_576 {
            return Err(failure("invalid web evidence range"));
        }
        let page=peritus_tools_web::Store::for_user().and_then(|store|store.read(&json!({"source":reference["source"],"offset":start,"max_bytes":(end-start+4).min(1_048_576),"include_links":false}))).map_err(|e|failure(e.to_string()))?;
        evidence["reference"] = json!({"kind":"web","source":reference["source"],"requested_url":page["requested_url"],"final_url":page["url"],"fetched_at_unix":page["fetched_at_unix"],"text_sha256":page["text_sha256"],"start":start,"end":end});
    }
    Ok(evidence)
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, ProductRunnerError> {
    value[key].as_str().ok_or_else(|| failure(format!("missing string {key}")))
}

pub(super) fn tool_call(name: &str, args: &Value) -> Result<CompletedToolCall, ProductRunnerError> {
    let make = || -> Result<CompletedToolCall, DeveloperLoopError> {
        Ok(CompletedToolCall::new(
            ToolCallId::new("host-design-operation".to_owned())?,
            ToolName::new(name.to_owned())?,
            CanonicalJson::parse(&args.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION))?,
        )?)
    };
    make().map_err(|e| failure(e.to_string()))
}

fn read(
    input: &ProductRunInput,
    source: &SourceDesignSnapshot,
    pass: &ExpertPass,
    args: &Value,
) -> Result<Value, ProductRunnerError> {
    let kind = string(args, "kind")?;
    let text = match kind {
        "source" => source.text().to_owned(),
        "draft" => pass.markdown(source),
        "records" => {
            serde_json::to_string(&pass.records(source)?).map_err(|e| failure(e.to_string()))?
        }
        "proposal" => serde_json::to_string(
            &super::retention::observation(input)?
                .ok_or_else(|| failure("no proposal retained"))?,
        )
        .map_err(|e| failure(e.to_string()))?,
        "previous" => String::from_utf8(
            super::retention::read(&input.trace_path.with_extension("design-expert-prior.json"))?
                .ok_or_else(|| failure("no previous expert pass"))?,
        )
        .map_err(|e| failure(e.to_string()))?,
        _ => return Err(failure("unknown design page kind")),
    };
    let offset = args["offset"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| failure("invalid page offset"))?;
    let max = args["max_bytes"]
        .as_u64()
        .filter(|v| (4..=32768).contains(v))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| failure("invalid page size"))?;
    if offset >= text.len() || !text.is_char_boundary(offset) {
        return Err(failure("page offset is outside the exact UTF-8 text"));
    }
    let end = text.floor_char_boundary(offset.saturating_add(max).min(text.len()));
    Ok(
        json!({"kind":kind,"draft_version":pass.version,"source_sha256":pass.source,"offset":offset,"total_bytes":text.len(),"next_offset":(end<text.len()).then_some(end),"text":&text[offset..end],"sha256":crate::encode_hex(peritus_codec::sha256(text.as_bytes()).as_bytes()),"authority":"evidence_only"}),
    )
}

fn record_plan(pass: &mut ExpertPass, args: &Value) -> Result<(), ProductRunnerError> {
    let domains: Vec<Domain> =
        serde_json::from_value(args["domains"].clone()).map_err(|e| failure(e.to_string()))?;
    if domains.is_empty() || domains.len() + pass.domains.len() > 64 {
        return Err(failure("invalid domain worklist size"));
    }
    for domain in domains {
        if domain.discipline.trim().is_empty()
            || domain.applicability.trim().is_empty()
            || domain.questions.is_empty()
            || domain.questions.len() > 32
            || domain.questions.iter().any(|q| q.question.trim().is_empty())
        {
            return Err(failure("each domain needs applicability and concrete research questions"));
        }
        if pass.domains.iter().any(|d| {
            d.discipline == domain.discipline
                && d.questions
                    .iter()
                    .map(|q| &q.question)
                    .eq(domain.questions.iter().map(|q| &q.question))
        }) {
            return Err(failure("this domain/question worklist is already retained"));
        }
        pass.domains.push(domain);
    }
    Ok(())
}

fn record_amendment(
    input: &ProductRunInput,
    source: &SourceDesignSnapshot,
    research: &ResearchSession,
    pass: &mut ExpertPass,
    args: &Value,
) -> Result<(), ProductRunnerError> {
    let mut amendment = args.clone();
    amendment
        .as_object_mut()
        .ok_or_else(|| failure("amendment is not an object"))?
        .remove("expected_version");
    amendment.as_object_mut().expect("checked object").remove("source_sha256");
    let mut amendment: Amendment =
        serde_json::from_value(amendment).map_err(|e| failure(e.to_string()))?;
    let question = pass
        .questions()
        .into_iter()
        .find(|(id, _)| id == &amendment.question_id)
        .map(|(_, q)| q.clone())
        .ok_or_else(|| failure("unknown research question"))?;
    if amendment.markdown.trim().is_empty()
        || amendment.markdown.len() > 32768
        || amendment.acceptance.is_empty()
        || amendment.evidence.is_empty()
        || pass.amendments.len() >= 256
    {
        return Err(failure(
            "amendment needs bounded Markdown, evidence and observable acceptance",
        ));
    }
    amendment.evidence =
        amendment.evidence.iter().map(resolve_evidence).collect::<Result<Vec<_>, _>>()?;
    if question.external_authority_required
        && !matches!(amendment.class.as_str(), "unknown" | "conflict")
        && !amendment.evidence.iter().any(|e| {
            e["reference"]["kind"] == "web"
                && matches!(e["authority"].as_str(), Some("primary" | "official"))
        })
    {
        return Err(failure("this research question requires opened primary/official evidence"));
    }
    if let Some(index) = pass.amendments.iter().position(|a| a.question_id == amendment.question_id)
    {
        pass.amendments[index] = amendment;
    } else {
        pass.amendments.push(amendment);
    }
    let records = pass.records(source)?;
    super::expansion::validate_pass_records(
        input,
        source,
        research.expansion.as_ref().ok_or_else(|| failure("expansion unavailable"))?,
        &records,
    )?;
    Ok(())
}

fn recover_candidate(
    input: &ProductRunInput,
    source: &SourceDesignSnapshot,
    research: &ResearchSession,
    pass: &mut ExpertPass,
) -> Result<(), ProductRunnerError> {
    if pass.imported_records.is_some() {
        return Err(failure("candidate work is already imported; review or amend it"));
    }
    for extension in ["design-expert.json", "design-expert-prior.json"] {
        if let Some(bytes) = super::retention::read(&input.trace_path.with_extension(extension))? {
            let previous: ExpertPass =
                serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))?;
            if previous.source == pass.source {
                continue;
            }
            let prefix = source
                .text()
                .as_bytes()
                .get(
                    ..usize::try_from(previous.source_bytes)
                        .map_err(|_| failure("previous source size exceeds platform"))?,
                )
                .ok_or_else(|| failure("previous source is not preserved"))?;
            if previous.source_bytes == 0
                || crate::encode_hex(peritus_codec::sha256(prefix).as_bytes()) != previous.source
            {
                return Err(failure(
                    "previous source changed; read the retained previous pass and reconcile its amendments explicitly",
                ));
            }
            if !pass.domains.is_empty() || !pass.amendments.is_empty() {
                return Err(failure("recover prior work before adding new questions"));
            }
            pass.domains = previous.domains;
            pass.amendments = previous.amendments;
            pass.imported_records = previous.imported_records;
            pass.imported_markdown = previous.imported_markdown;
            super::expansion::validate_pass_records(
                input,
                source,
                research.expansion.as_ref().ok_or_else(|| failure("expansion unavailable"))?,
                &pass.records(source)?,
            )?;
            return Ok(());
        }
    }
    let proposal = super::retention::observation(input)?
        .ok_or_else(|| failure("no retained candidate is available"))?;
    let records = proposal["arguments"]["records"].clone();
    let _parsed = super::ExpansionRecords::from_json(
        &serde_json::to_vec(&records).map_err(|e| failure(e.to_string()))?,
    )?;
    // Import is an unapproved recovery operation, not a claim that the old proposal
    // satisfies today's evidence. Keep it repairable through bounded candidate patches;
    // publication still performs the complete current evidence and Ready checks.
    pass.imported_markdown = Some(string(&proposal["arguments"], "markdown")?.to_owned());
    pass.imported_records = Some(records);
    Ok(())
}

fn publish_candidate(
    input: &ProductRunInput,
    research: &mut ResearchSession,
    workspace: &WorkspaceDeveloperTools,
    source: &SourceDesignSnapshot,
    pass: &ExpertPass,
    args: &Value,
) -> Result<Value, ProductRunnerError> {
    if research
        .expansion
        .as_ref()
        .is_some_and(|state| state.stage() == super::ExpansionStage::Ready)
    {
        super::read_design_expansion(input, source)?;
        return Ok(json!({"published":true,"draft_version":pass.version,"replayed":true}));
    }
    if string(args, "review")?.trim().is_empty()
        || !pass.unresolved().is_empty()
        || pass.domains.is_empty() && pass.imported_records.is_none()
    {
        return Err(failure(
            "review the complete source and resolve every research question before publication",
        ));
    }
    let records =
        serde_json::to_value(pass.records(source)?).map_err(|e| failure(e.to_string()))?;
    let version =
        research.expansion.as_ref().ok_or_else(|| failure("expansion unavailable"))?.version();
    let publication = tool_call(
        super::research::TOOL,
        &json!({"action":"publish_ready","expected_version":version,"records":records,"markdown":pass.markdown(source)}),
    )?;
    let result = research
        .execute(input, &publication, &workspace.grounding().markdown())
        .map_err(|e| failure(e.to_string()))?;
    if result.is_error {
        return Err(failure(result.output.to_wire_string()));
    }
    Ok(
        json!({"published":true,"draft_version":pass.version,"expansion_version":research.expansion.as_ref().expect("published expansion").version(),"next_action":"Return a concise handoff; the host now continues the authorized coding pipeline."}),
    )
}
