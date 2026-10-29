//! Sequential host execution; every child is independently admitted, executed and committed.

use super::{
    PlanRecord,
    admission::Admission,
    contract::{MAX_PLAN_BYTES, Request},
    error, records,
    state::{self, Page},
    validation,
};
use crate::local_context::{LocalContextHandle, tools};
use peritus_agent::{
    DeveloperControlFlow, DeveloperLoopError, DeveloperModelRole, DeveloperToolEffect,
    DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};
use serde_json::{Value, json};

pub(super) fn execute(
    memory: &LocalContextHandle,
    base: &mut dyn DeveloperToolExecutor,
    role: Option<DeveloperModelRole>,
    call: &CompletedToolCall,
    host_calls: &mut u32,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    let Ok(request) = serde_json::from_slice::<Request>(call.arguments().canonical_bytes()) else {
        return super::rejected("invalid inspection plan schema");
    };
    if request.version != 1
        || !(2048..=65_536).contains(&request.max_result_bytes)
        || call.arguments().canonical_bytes().len() > MAX_PLAN_BYTES
        || request.plan.is_some() == request.cursor.is_some()
    {
        return super::rejected("unsupported plan version, page bounds, or ambiguous plan/cursor");
    }
    if let Some(nodes) = &request.plan
        && let Err(reason) = validation::nodes(nodes)
    {
        return super::rejected(&reason.to_string());
    }
    if base.required_tool_name().is_some() {
        return super::rejected("complete the ordinary required host tool before starting a plan");
    }
    let maximum = request.max_result_bytes;
    let (page, admission) = {
        let mut owner = memory.lock()?;
        if !owner.config.read_only_plans.is_enabled() {
            return super::rejected("inspection plan execution is disabled");
        }
        if request.scope != tools::hex(owner.store.scope_digest().as_bytes())
            || super::admission::revision(&owner)? != request.input_revision
        {
            return super::rejected("plan scope or current input binding changed");
        }
        if let Some(cursor) = &request.cursor {
            let Ok(id) = tools::sequence(&owner, cursor) else {
                return super::rejected("invalid or foreign plan continuation");
            };
            if !matches!(records::maybe_load(&owner, id)?, Some(PlanRecord::Continuation { .. })) {
                return super::rejected("handle does not identify a plan continuation");
            }
            if !crate::local_context::source_policy::allowed(&owner, id)? {
                return super::rejected(
                    "plan continuation depends on evidence no longer readable under current permissions",
                );
            }
        }
        let identity = super::super::memory::call_identity(call);
        if !owner.transcript.pending.iter().any(|pending| {
            pending.invocation == owner.transcript.invocation && pending.call == identity
        }) {
            return super::rejected("plan has no matching durable provider proposal");
        }
        let admission =
            Admission::new(&owner, role, call.id().expose_for_wire(), request.input_revision)?;
        if admission.current()? != DeveloperControlFlow::Continue {
            return super::rejected("current read permission or input no longer admits the plan");
        }
        super::ensure_reader(&mut owner)?;
        let page = state::prepare(&mut owner, request, call, role)?;
        drop(owner);
        (page, admission)
    };
    run_page(memory, base, page, &admission, maximum, host_calls)
}

fn run_page(
    memory: &LocalContextHandle,
    base: &mut dyn DeveloperToolExecutor,
    mut page: Page,
    admission: &Admission,
    maximum: usize,
    host_calls: &mut u32,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    let mut rows = Vec::new();
    let mut status = "completed";
    while page.steps.len() < page.nodes.len() {
        let index = page.steps.len();
        let node = &page.nodes[index];
        if !rows.is_empty() && !next_row_fits(&page, &rows, maximum)? {
            status = "page_complete";
            break;
        }
        let Ok(arguments) = validation::resolve(node, &page.outputs) else {
            status = "needs_judgment";
            break;
        };
        let sequence = u32::try_from(index + 1).map_err(|_| error("plan ordinal overflow"))?;
        if admission.admit(sequence)? != DeveloperControlFlow::Continue {
            status = "yielded";
            break;
        }
        let child = child_call(page.proposal, index, node.tool.name(), &arguments)?;
        if base.effect(&child) != DeveloperToolEffect::ReadOnly {
            return Err(error("inspection child executor did not prove read-only behavior"));
        }
        admission.started(&child)?;
        let output = match base.execute(&child) {
            Ok(output) => output,
            Err(reason) => super::observation(&json!({"error":reason.to_string()}), true)?,
        };
        *host_calls =
            host_calls.checked_add(1).ok_or_else(|| error("host plan accounting overflow"))?;
        let value: Value = serde_json::from_slice(output.output.canonical_bytes())
            .map_err(|_| error("invalid canonical child output"))?;
        let row = commit_step(memory, &mut page, arguments, &output, &value)?;
        rows.push(row);
        admission.finished(&child, &output)?;
        let allowed = admission.complete(sequence)? == DeveloperControlFlow::Continue;
        if !allowed {
            status = "yielded";
            break;
        }
        if output.is_error {
            status = "needs_judgment";
            break;
        }
        let last = rows.last_mut().ok_or_else(|| error("missing committed plan step"))?;
        last["output"] = value;
        if encoded_size(&render(
            &page,
            &rows,
            "page_complete",
            Some(&"x".repeat(100)),
            page.steps.len(),
        ))? > maximum
        {
            last_remove_output(&mut rows);
            status =
                if page.steps.len() < page.nodes.len() { "page_complete" } else { "completed" };
            break;
        }
    }
    let mut owner = memory.lock()?;
    let cursor = state::cursor(&mut owner, &page)?;
    drop(owner);
    if status == "yielded" {
        for row in &mut rows {
            if let Some(row) = row.as_object_mut() {
                row.remove("output");
            }
        }
    }
    let result = render(&page, &rows, status, cursor.as_deref(), page.steps.len());
    if encoded_size(&result)? > maximum {
        return Err(error("plan page exceeded its declared allocation"));
    }
    super::observation(&result, status == "needs_judgment" || status == "yielded")
}

fn render(
    page: &Page,
    rows: &[Value],
    status: &str,
    cursor: Option<&str>,
    completed: usize,
) -> Value {
    let sources = rows
        .iter()
        .filter_map(|row| row["step"].as_str())
        .map(|handle| json!({"handle":handle}))
        .collect::<Vec<_>>();
    json!({"version":1,"status":status,"proposal":page.proposal_handle,"completed_nodes":completed,"total_nodes":page.nodes.len(),
        "steps":rows,"continuation":cursor,"sources":sources,"authority":"none"})
}

fn encoded_size(value: &Value) -> Result<usize, DeveloperLoopError> {
    Ok(serde_json::to_vec(value).map_err(|_| error("encode plan page"))?.len())
}

fn next_row_fits(page: &Page, rows: &[Value], maximum: usize) -> Result<bool, DeveloperLoopError> {
    let node = &page.nodes[page.steps.len()];
    let mut proposed = rows.to_vec();
    proposed.push(json!({"id":node.id,"tool":node.tool.name(),"step":"x".repeat(100),
        "is_error":false,"source_sha256":"f".repeat(64),"source_bytes":u64::MAX}));
    Ok(encoded_size(&render(
        page,
        &proposed,
        "page_complete",
        Some(&"x".repeat(100)),
        page.steps.len() + 1,
    ))? <= maximum)
}

fn last_remove_output(rows: &mut [Value]) {
    if let Some(Value::Object(row)) = rows.last_mut() {
        row.remove("output");
    }
}

fn child_call(
    proposal: u64,
    index: usize,
    name: &str,
    arguments: &Value,
) -> Result<CompletedToolCall, DeveloperLoopError> {
    Ok(CompletedToolCall::new(
        ToolCallId::new(format!("host_plan_{proposal}_{index}"))?,
        ToolName::new(name.to_owned())?,
        CanonicalJson::parse(
            &arguments.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?,
    )?)
}

fn commit_step(
    memory: &LocalContextHandle,
    page: &mut Page,
    arguments: Value,
    observation: &DeveloperToolObservation,
    value: &Value,
) -> Result<Value, DeveloperLoopError> {
    let mut owner = memory.lock()?;
    let output = owner.store.store(observation.output.canonical_bytes())?;
    let index = page.steps.len();
    let node = &page.nodes[index];
    let dependencies = node
        .depends_on
        .iter()
        .filter_map(|dependency| {
            page.nodes
                .iter()
                .position(|node| &node.id == dependency)
                .and_then(|index| page.steps.get(index).copied())
        })
        .collect::<Vec<_>>();
    let id = records::archive(
        &mut owner,
        PlanRecord::Step {
            proposal: page.proposal,
            parent: page.parent.clone(),
            input_revision: page.input_revision,
            index: u32::try_from(index).map_err(|_| error("plan ordinal overflow"))?,
            arguments,
            output,
            is_error: observation.is_error,
            dependencies,
        },
        &[output.digest],
    )?;
    let handle = tools::source_handle(&owner, id);
    drop(owner);
    page.steps.push(id);
    page.outputs.push((node.id.clone(), value.clone(), observation.is_error));
    Ok(json!({"id":node.id,"tool":node.tool.name(),"step":handle,"is_error":observation.is_error,
        "source_sha256":tools::hex(output.digest.as_bytes()),"source_bytes":output.bytes}))
}
