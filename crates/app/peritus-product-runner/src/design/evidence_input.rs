//! Typed amendment evidence, with source identities resolved by the host.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    expansion::{EvidenceAuthority, EvidenceReference},
    expert_pass::failure,
};
use crate::ProductRunnerError;

/// The persisted format remains compatible with existing validated expert passes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    title: String,
    version: Option<String>,
    authority: EvidenceAuthority,
    reference: EvidenceReference,
}

impl Evidence {
    pub const fn is_source(&self) -> bool {
        matches!(self.reference, EvidenceReference::SourceRange { .. })
    }
    pub const fn is_external(&self) -> bool {
        matches!(self.authority, EvidenceAuthority::Primary | EvidenceAuthority::Official)
            && matches!(self.reference, EvidenceReference::Web { .. })
    }
    pub fn value(&self) -> Result<Value, ProductRunnerError> {
        serde_json::to_value(self).map_err(|e| failure(e.to_string()))
    }
}

pub(super) fn resolve(value: &Value) -> Result<Value, ProductRunnerError> {
    let mut evidence = value.clone();
    // This is an advertised input variant: the explicit authority and sole invariant
    // field identify a rationale. Its redundant persisted tag is host-owned.
    if evidence["authority"] == "engineering_rationale"
        && evidence["reference"]["kind"].is_null()
        && evidence["reference"]
            .as_object()
            .is_some_and(|r| r.len() == 1 && r.contains_key("invariant"))
    {
        evidence["reference"]["kind"] = json!("engineering_rationale");
    }
    if evidence["reference"]["kind"] == "web" {
        let reference = &evidence["reference"];
        let start =
            reference["start"].as_u64().ok_or_else(|| failure("web range start missing"))?;
        let end = reference["end"].as_u64().ok_or_else(|| failure("web range end missing"))?;
        if end <= start || end - start > 1_048_576 {
            return Err(failure("invalid web evidence range"));
        }
        let page = peritus_tools_web::Store::for_user()
            .and_then(|store| store.read(&json!({"source":reference["source"],"offset":start,"max_bytes":(end-start+4).min(1_048_576),"include_links":false})))
            .map_err(|e| failure(e.to_string()))?;
        evidence["reference"] = json!({"kind":"web","source":reference["source"],"requested_url":page["requested_url"],"final_url":page["url"],"fetched_at_unix":page["fetched_at_unix"],"text_sha256":page["text_sha256"],"start":start,"end":end});
    }
    // Typed construction happens before any amendment or assembled record is changed.
    let typed: Evidence = serde_json::from_value(evidence).map_err(|e| failure(e.to_string()))?;
    typed.value()
}

pub(super) fn schema() -> Value {
    fn object(properties: Value, required: &[&str]) -> Value {
        let mut result = json!({"type":"object","required":required,"additionalProperties":false});
        result["properties"] = properties;
        result
    }
    let short = json!({"type":"string","minLength":1,"maxLength":1024});
    let version = json!({"anyOf":[{"type":"null"},short]});
    let web = object(
        json!({"kind":{"const":"web"},"source":{"type":"string","minLength":64,"maxLength":64},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}}),
        &["kind", "source", "start", "end"],
    );
    let repository = object(
        json!({"kind":{"const":"repository"},"path":short,"sha256":{"type":"string","minLength":64,"maxLength":64}}),
        &["kind", "path", "sha256"],
    );
    let source = object(
        json!({"kind":{"const":"source_range"},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}}),
        &["kind", "start", "end"],
    );
    let rationale = object(
        json!({"kind":{"const":"engineering_rationale"},"invariant":{"type":"string","minLength":1,"maxLength":32768}}),
        &["invariant"],
    );
    let mut variants = Vec::new();
    for (authority, reference, applicability) in [
        (json!({"enum":["primary","official"]}), web, short.clone()),
        (json!({"const":"repository"}), repository, version.clone()),
        (json!({"const":"source_design"}), source, version.clone()),
        (json!({"const":"engineering_rationale"}), rationale, version),
    ] {
        variants.push(object(json!({"title":short,"version":applicability,"authority":authority,"reference":reference}), &["title","version","authority","reference"]));
    }
    json!({"anyOf":variants})
}
