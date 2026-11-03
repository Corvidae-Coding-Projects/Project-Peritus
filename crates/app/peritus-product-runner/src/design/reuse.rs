//! Reuse a host-published task design without another discovery/provider pass.

use super::{
    DesignDocument, ExpansionReceiptIdentity, ExpansionStage, ProductRunInput, ProductRunnerError,
    SourceDesignSnapshot, publish, read_design_expansion,
};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    workspace: [u8; 16],
    request: String,
    design: String,
    #[serde(default)]
    guided: Option<String>,
    #[serde(default)]
    revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expansion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence: Option<String>,
}

fn digest(bytes: &[u8]) -> String {
    encode_digest(peritus_codec::sha256(bytes).as_bytes())
}

fn encode_digest(bytes: &[u8; 32]) -> String {
    format!("{bytes:02x?}")
}

fn request(input: &ProductRunInput) -> String {
    digest(format!("{}\0{}", input.workspace_root.display(), input.task).as_bytes())
}

pub(super) fn load(input: &ProductRunInput) -> Option<DesignDocument> {
    if let Ok(bytes) = fs::read(input.trace_path.with_extension("design-receipt.json"))
        && let Ok(saved) = serde_json::from_slice::<Receipt>(&bytes)
        && saved.schema == 3
        && saved.workspace == *input.workspace_id.as_bytes()
        && saved.request == request(input)
        && saved.guided == guided_digest(input)
        && let Some((document, identity)) = open_ready_expansion(input)
        && saved.design == digest(document.markdown().as_bytes())
        && matches_identity(&saved, &identity)
    {
        crate::efficiency::event(
            &input.trace_path,
            serde_json::json!({"event":"design-expansion-reused","input_revision":identity.input_revision}),
        );
        return Some(document);
    }
    None
}

/// Restores an exact ready expansion even when its derived reuse receipt has not yet been minted.
///
/// Guided Design publishes the source, expansion manifest, and expanded Markdown before Build is
/// authorized. Those host-bound identities are sufficient to recreate the cache receipt locally;
/// no model call is needed merely to repeat an already completed design.
pub(super) fn load_ready_expansion(input: &ProductRunInput) -> Option<DesignDocument> {
    let (document, identity) = open_ready_expansion(input)?;
    if let Err(error) = save(input, &document) {
        eprintln!("peritus: expanded design reuse receipt unavailable: {error}");
    }
    crate::efficiency::event(
        &input.trace_path,
        serde_json::json!({
            "event":"design-expansion-reused",
            "input_revision":identity.input_revision,
            "receipt_recreated":true,
        }),
    );
    Some(document)
}

fn open_ready_expansion(
    input: &ProductRunInput,
) -> Option<(DesignDocument, ExpansionReceiptIdentity)> {
    let path = input.trace_path.with_extension("design.md");
    let source =
        SourceDesignSnapshot::open(&input.trace_path, input.run_id, input.workspace_id).ok()?;
    let expansion = read_design_expansion(input, &source).ok()?;
    if expansion.stage() != ExpansionStage::Ready {
        return None;
    }
    let markdown = fs::read_to_string(&path).ok()?;
    let identity = ExpansionReceiptIdentity::ready(input, &source, &expansion, &markdown).ok()?;
    let document = DesignDocument::restored_expanded(
        path,
        markdown,
        identity.input_revision,
        identity.clone(),
    );
    Some((document, identity))
}

fn matches_identity(saved: &Receipt, identity: &ExpansionReceiptIdentity) -> bool {
    identity.is_complete()
        && saved.source.as_deref() == Some(encode_digest(&identity.source_sha256).as_str())
        && saved.expansion.as_deref() == Some(encode_digest(&identity.manifest_sha256).as_str())
        && saved.design == encode_digest(&identity.expanded_sha256)
        && saved.task.as_deref() == Some(encode_digest(&identity.task_sha256).as_str())
        && saved.evidence.as_deref() == Some(encode_digest(&identity.evidence_sha256).as_str())
        && saved.policy.as_deref() == Some(identity.policy_version.as_str())
        && saved.revision == identity.input_revision
}

pub(super) fn save(
    input: &ProductRunInput,
    design: &DesignDocument,
) -> Result<(), ProductRunnerError> {
    let expansion = design.expansion_identity();
    let Some(expansion) = expansion else {
        return Err(super::filesystem(
            "only a ready domain expansion can publish a design reuse receipt",
        ));
    };
    let receipt = Receipt {
        schema: 3,
        workspace: *input.workspace_id.as_bytes(),
        request: request(input),
        design: digest(design.markdown().as_bytes()),
        guided: guided_digest(input),
        revision: design.conversation_revision(),
        source: Some(encode_digest(&expansion.source_sha256)),
        expansion: Some(encode_digest(&expansion.manifest_sha256)),
        policy: Some(expansion.policy_version.clone()),
        task: Some(encode_digest(&expansion.task_sha256)),
        evidence: Some(encode_digest(&expansion.evidence_sha256)),
    };
    let bytes =
        serde_json::to_vec(&receipt).map_err(|error| super::filesystem(error.to_string()))?;
    publish(&input.trace_path.with_extension("design-receipt.json"), &bytes)
}

fn guided_digest(input: &ProductRunInput) -> Option<String> {
    fs::read(crate::guided_design::path(input)).ok().map(|bytes| digest(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_one_and_two_receipts_remain_readable_without_expansion_fields() {
        for schema in [1, 2] {
            let json = format!(
                r#"{{"schema":{schema},"workspace":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"request":"request","design":"design","revision":7}}"#,
            );
            let receipt: Receipt = serde_json::from_str(&json).expect("legacy receipt");
            assert_eq!(receipt.schema, schema);
            assert!(receipt.source.is_none());
            assert!(receipt.expansion.is_none());
            assert!(receipt.policy.is_none());
            assert!(receipt.task.is_none());
            assert!(receipt.evidence.is_none());
        }
    }

    #[test]
    fn schema_three_round_trip_carries_exact_source_manifest_and_policy_identity() {
        let source = [0x12; 32];
        let expansion = [0x34; 32];
        let receipt = Receipt {
            schema: 3,
            workspace: [0x56; 16],
            request: "request".to_owned(),
            design: "design".to_owned(),
            guided: None,
            revision: 9,
            source: Some(encode_digest(&source)),
            expansion: Some(encode_digest(&expansion)),
            policy: Some(crate::design::DESIGN_EXPANSION_POLICY_VERSION.to_owned()),
            task: Some(encode_digest(&[0x78; 32])),
            evidence: Some(encode_digest(&[0x9a; 32])),
        };
        let bytes = serde_json::to_vec(&receipt).expect("encode schema-3 receipt");
        let restored: Receipt = serde_json::from_slice(&bytes).expect("decode schema-3 receipt");
        assert_eq!(restored.source.as_deref(), Some(encode_digest(&source).as_str()));
        assert_eq!(restored.expansion.as_deref(), Some(encode_digest(&expansion).as_str()));
        assert_eq!(
            restored.policy.as_deref(),
            Some(crate::design::DESIGN_EXPANSION_POLICY_VERSION)
        );
        assert_eq!(restored.task.as_deref(), Some(encode_digest(&[0x78; 32]).as_str()));
        assert_eq!(restored.evidence.as_deref(), Some(encode_digest(&[0x9a; 32]).as_str()));
        assert!(serde_json::from_slice::<Receipt>(
            br#"{"schema":3,"workspace":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"request":"r","design":"d","unexpected":true}"#,
        )
        .is_err());
    }

    #[test]
    fn every_governing_identity_change_invalidates_schema_three_reuse() {
        let identity = ExpansionReceiptIdentity {
            source_sha256: [0x11; 32],
            manifest_sha256: [0x22; 32],
            expanded_sha256: [0x33; 32],
            task_sha256: [0x44; 32],
            evidence_sha256: [0x55; 32],
            input_revision: 7,
            policy_version: crate::design::DESIGN_EXPANSION_POLICY_VERSION.to_owned(),
        };
        let receipt = Receipt {
            schema: 3,
            workspace: [0x66; 16],
            request: "request".to_owned(),
            design: encode_digest(&identity.expanded_sha256),
            guided: None,
            revision: identity.input_revision,
            source: Some(encode_digest(&identity.source_sha256)),
            expansion: Some(encode_digest(&identity.manifest_sha256)),
            policy: Some(identity.policy_version.clone()),
            task: Some(encode_digest(&identity.task_sha256)),
            evidence: Some(encode_digest(&identity.evidence_sha256)),
        };
        assert!(matches_identity(&receipt, &identity));

        let mut mutations = Vec::new();
        let mut changed = identity.clone();
        changed.source_sha256[0] ^= 1;
        mutations.push(changed);
        let mut changed = identity.clone();
        changed.manifest_sha256[0] ^= 1;
        mutations.push(changed);
        let mut changed = identity.clone();
        changed.expanded_sha256[0] ^= 1;
        mutations.push(changed);
        let mut changed = identity.clone();
        changed.task_sha256[0] ^= 1;
        mutations.push(changed);
        let mut changed = identity.clone();
        changed.evidence_sha256[0] ^= 1;
        mutations.push(changed);
        let mut changed = identity.clone();
        changed.input_revision += 1;
        mutations.push(changed);
        let mut changed = identity;
        changed.policy_version.push_str("-changed");
        mutations.push(changed);

        for changed in mutations {
            assert!(!matches_identity(&receipt, &changed));
        }
        assert_ne!(receipt.schema, 1);
        assert_ne!(receipt.schema, 2);
    }
}
