//! Current checked file versions for all supplied reads, including undeclared model dependencies.

use crate::local_context::{error, memory::LocalMemory};
use peritus_agent::DeveloperLoopError;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub(super) struct Bindings {
    pub(super) files: BTreeMap<String, Value>,
    pub(super) identity: [u8; 32],
}

pub(super) fn capture(
    memory: &LocalMemory,
    evidence: &[super::index::Evidence],
) -> Result<Bindings, DeveloperLoopError> {
    let paths =
        evidence.iter().filter_map(|source| source.file.as_deref()).collect::<BTreeSet<_>>();
    let mut files = BTreeMap::new();
    for path in paths {
        let state = if super::super::evidence::may_reuse_read(memory, path) {
            resolve(memory, path)
                .and_then(|path| observe(&path))
                .unwrap_or_else(|_| json!({"state":"unavailable_requires_fresh_inspection"}))
        } else {
            json!({"state":"excluded_by_current_authority"})
        };
        files.insert(path.to_owned(), state);
    }
    let bytes =
        serde_json::to_vec(&files).map_err(|_| error("encode decision file-version closure"))?;
    Ok(Bindings { files, identity: peritus_codec::sha256(&bytes).into_bytes() })
}

fn resolve(memory: &LocalMemory, path: &str) -> Result<PathBuf, DeveloperLoopError> {
    let view = memory
        .conversation
        .as_ref()
        .ok_or_else(|| error("decision file has no authority owner"))?;
    if Path::new(path).is_absolute() {
        crate::developer_tools::authorized_reference_file(
            &memory.workspace,
            &view.reference_authority_context(),
            path,
        )
    } else {
        crate::developer_tools::checked_protected_file_for_developer(
            &memory.workspace,
            path,
            &view.reference_authority_context(),
            &memory.workspace_scope.protected,
        )
    }
}

fn observe(path: &Path) -> Result<Value, DeveloperLoopError> {
    let before =
        std::fs::symlink_metadata(path).map_err(|_| error("inspect decision file metadata"))?;
    let token = crate::file_observation::fingerprint(&before);
    let content = crate::file_observation::read(path, false)
        .map_err(|_| error("observe decision file content"))?;
    let after =
        std::fs::symlink_metadata(path).map_err(|_| error("recheck decision file metadata"))?;
    let after_token = crate::file_observation::fingerprint(&after);
    let stable = token.is_some() && token == after_token;
    Ok(
        json!({"state":if stable {"observed_file_version"} else {"unproven_change_token_requires_fresh_inspection"},
        "content_sha256":crate::local_context::tools::hex(&content.digest),"bytes":content.bytes,
        "change_token":after_token.as_ref().map(crate::file_observation::Fingerprint::binding),
        "readonly":after.permissions().readonly(),"regular_file":after.is_file(),
        "qualification":"not_a_current_candidate_or_grounding_grant"}),
    )
}
