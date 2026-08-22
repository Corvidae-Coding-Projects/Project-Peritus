use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActorsDocument {
    pub(super) schema: String,
    pub(super) schema_version: u64,
    pub(super) baseline: String,
    pub(super) entries: Vec<ActorEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActorEntry {
    pub(super) id: String,
    pub(super) kind: ActorKind,
    pub(super) principal: String,
    pub(super) display_name: String,
    pub(super) roles: Vec<ActorRole>,
    pub(super) provenance: ActorProvenanceRef,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActorProvenanceRef {
    pub(super) record_path: String,
    pub(super) record_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActorProvenanceDocument {
    pub(super) schema: String,
    pub(super) schema_version: u64,
    pub(super) baseline: String,
    pub(super) entries: Vec<ActorProvenanceEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActorProvenanceEntry {
    pub(super) actor_id: String,
    pub(super) kind: ActorKind,
    pub(super) principal: String,
    pub(super) repository: String,
    pub(super) issue: u64,
    pub(super) issue_created_at: String,
    pub(super) session: u64,
    pub(super) task: String,
    pub(super) mode: ActorMode,
    pub(super) model: Option<String>,
    pub(super) reasoning_effort: Option<ActorReasoningEffort>,
    pub(super) public_key: Option<String>,
    pub(super) allowed_signer: Option<String>,
    pub(super) record_locators: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ActorMode {
    Implementation,
    ReadOnlyReview,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ActorReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ActorKind {
    CrosslinkAgent,
    CodexSubagent,
}

impl ActorKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::CrosslinkAgent => "crosslink-agent",
            Self::CodexSubagent => "codex-subagent",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ActorRole {
    Owner,
    Reviewer,
}

impl ActorRole {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Reviewer => "reviewer",
        }
    }
}
