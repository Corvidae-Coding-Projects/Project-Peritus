//! Recover a plan page only from exact committed proposal and child-receipt identities.

use super::{
    PlanRecord,
    contract::{Node, Parent, Request},
    error, records,
};
use crate::local_context::{memory::LocalMemory, tools};
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_model_protocol::CompletedToolCall;
use serde_json::Value;

pub(super) struct Page {
    pub(super) proposal: u64,
    pub(super) proposal_handle: String,
    pub(super) parent: Parent,
    pub(super) input_revision: u64,
    pub(super) nodes: Vec<Node>,
    pub(super) steps: Vec<u64>,
    pub(super) outputs: Vec<(String, Value, bool)>,
}

pub(super) fn prepare(
    memory: &mut LocalMemory,
    request: Request,
    call: &CompletedToolCall,
    role: Option<peritus_agent::DeveloperModelRole>,
) -> Result<Page, DeveloperLoopError> {
    let parent = Parent {
        context_invocation: memory.transcript.invocation,
        invocation: memory.transcript.request_prefix.clone(),
        call_id: call.id().expose_for_wire().to_owned(),
        arguments_digest: sha256(call.arguments().canonical_bytes()).into_bytes(),
        model_role: role.map(|role| {
            match role {
                peritus_agent::DeveloperModelRole::Writer => "writer",
                peritus_agent::DeveloperModelRole::Reviewer => "reviewer",
                peritus_agent::DeveloperModelRole::Fixer => "fixer",
            }
            .to_owned()
        }),
    };
    if let Some(nodes) = request.plan {
        let proposal = records::archive(
            memory,
            PlanRecord::Proposal {
                parent: parent.clone(),
                input_revision: request.input_revision,
                nodes: nodes.clone(),
            },
            &[],
        )?;
        return Ok(Page {
            proposal,
            proposal_handle: tools::source_handle(memory, proposal),
            parent,
            input_revision: request.input_revision,
            nodes,
            steps: Vec::new(),
            outputs: Vec::new(),
        });
    }
    let handle = request.cursor.as_deref().ok_or_else(|| error("plan or continuation required"))?;
    let cursor = tools::sequence(memory, handle)?;
    let PlanRecord::Continuation { proposal, steps, next } = records::load(memory, cursor)? else {
        return Err(error("cursor is not a plan continuation"));
    };
    if proposal >= cursor || steps.len() != next as usize || steps.len() > 64 {
        return Err(error("invalid plan continuation binding"));
    }
    let PlanRecord::Proposal { nodes, .. } = records::load(memory, proposal)? else {
        return Err(error("continuation proposal unavailable"));
    };
    super::validation::nodes(&nodes)?;
    if steps.len() > nodes.len() {
        return Err(error("continuation exceeds original plan"));
    }
    let mut outputs = Vec::new();
    for (index, id) in steps.iter().copied().enumerate() {
        if id >= cursor || id <= proposal {
            return Err(error("invalid plan child ordering"));
        }
        if !crate::local_context::source_policy::allowed(memory, id)? {
            return Err(error("continuation dependency is no longer readable"));
        }
        let PlanRecord::Step { proposal: owner, index: ordinal, output, is_error, .. } =
            records::load(memory, id)?
        else {
            return Err(error("continuation child unavailable"));
        };
        if owner != proposal || ordinal as usize != index {
            return Err(error("continuation child identity mismatch"));
        }
        let value = serde_json::from_slice(&memory.store.read(output)?)
            .map_err(|_| error("invalid exact child result"))?;
        outputs.push((nodes[index].id.clone(), value, is_error));
    }
    Ok(Page {
        proposal,
        proposal_handle: tools::source_handle(memory, proposal),
        parent,
        input_revision: request.input_revision,
        nodes,
        steps,
        outputs,
    })
}

pub(super) fn cursor(
    memory: &mut LocalMemory,
    page: &Page,
) -> Result<Option<String>, DeveloperLoopError> {
    if page.steps.len() == page.nodes.len() {
        return Ok(None);
    }
    let id = records::archive(
        memory,
        PlanRecord::Continuation {
            proposal: page.proposal,
            steps: page.steps.clone(),
            next: u32::try_from(page.steps.len()).map_err(|_| error("plan ordinal overflow"))?,
        },
        &[],
    )?;
    Ok(Some(tools::source_handle(memory, id)))
}
