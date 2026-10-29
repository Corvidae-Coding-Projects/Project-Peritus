//! Closed inert plan grammar and versioned host receipt data.

use crate::local_context::storage::StoredArtifact;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(in crate::local_context) const MAX_NODES: usize = 64;
pub(in crate::local_context) const MAX_PLAN_BYTES: usize = 65_536;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::local_context) struct Request {
    pub(in crate::local_context) version: u16,
    pub(in crate::local_context) scope: String,
    pub(in crate::local_context) input_revision: u64,
    #[serde(deserialize_with = "required_option")]
    pub(in crate::local_context) plan: Option<Vec<Node>>,
    #[serde(deserialize_with = "required_option")]
    pub(in crate::local_context) cursor: Option<String>,
    pub(in crate::local_context) max_result_bytes: usize,
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::local_context) struct Node {
    pub(in crate::local_context) id: String,
    pub(in crate::local_context) tool: ReadTool,
    pub(in crate::local_context) arguments: Value,
    pub(in crate::local_context) depends_on: Vec<String>,
    pub(in crate::local_context) result_refs: Vec<ResultReference>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::local_context) enum ReadTool {
    #[serde(rename = "workspace_read")]
    Read,
    #[serde(rename = "workspace_list")]
    List,
    #[serde(rename = "workspace_search")]
    Search,
}

impl ReadTool {
    pub(in crate::local_context) const fn name(self) -> &'static str {
        match self {
            Self::Read => "workspace_read",
            Self::List => "workspace_list",
            Self::Search => "workspace_search",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::local_context) struct ResultReference {
    pub(in crate::local_context) argument: String,
    pub(in crate::local_context) node: String,
    pub(in crate::local_context) pointer: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::local_context) struct Parent {
    pub(in crate::local_context) context_invocation: u64,
    pub(in crate::local_context) invocation: String,
    pub(in crate::local_context) call_id: String,
    pub(in crate::local_context) arguments_digest: [u8; 32],
    pub(in crate::local_context) model_role: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(in crate::local_context) struct RecordEnvelope {
    pub(in crate::local_context) version: u16,
    pub(in crate::local_context) scope: String,
    pub(in crate::local_context) record: PlanRecord,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::local_context) enum PlanRecord {
    Proposal {
        parent: Parent,
        input_revision: u64,
        nodes: Vec<Node>,
    },
    Step {
        proposal: u64,
        parent: Parent,
        input_revision: u64,
        index: u32,
        arguments: Value,
        output: StoredArtifact,
        is_error: bool,
        dependencies: Vec<u64>,
    },
    Continuation {
        proposal: u64,
        steps: Vec<u64>,
        next: u32,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::local_context) struct ReadRequest {
    pub(in crate::local_context) step: String,
    pub(in crate::local_context) offset: u64,
    pub(in crate::local_context) max_bytes: usize,
}
