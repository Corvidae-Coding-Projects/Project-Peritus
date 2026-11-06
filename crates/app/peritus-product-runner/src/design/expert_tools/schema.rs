//! Strict expert tool schemas, separate from state transactions.

use super::{AMEND, PATCH, PLAN, PUBLISH, READ, RECOVER, SELECT};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{
    BoundedText, JsonBounds, JsonSchema, ProtocolLimits, SchemaDialect, ToolDefinition, ToolName,
};
use serde_json::{Value, json};

pub(super) fn definition(
    name: &str,
    description: &str,
    schema: &Value,
) -> Result<ToolDefinition, DeveloperLoopError> {
    let limits = ProtocolLimits::PRODUCTION;
    Ok(ToolDefinition::new(
        ToolName::new(name.to_owned())?,
        Some(BoundedText::new(description.to_owned(), limits)?),
        JsonSchema::parse(
            &schema.to_string(),
            SchemaDialect::Draft202012,
            JsonBounds::schema(limits),
        )?,
        true,
    ))
}

fn object(properties: Value) -> Value {
    let required =
        properties.as_object().expect("schema properties").keys().cloned().collect::<Vec<_>>();
    {
        let mut schema = json!({"type":"object","required":required,"additionalProperties":false});
        schema["properties"] = properties;
        schema
    }
}

fn mutation(mut properties: Value) -> Value {
    properties["source_sha256"] = json!({"type":"string","minLength":64,"maxLength":64});
    properties["expected_version"] = json!({"type":"integer","minimum":1});
    object(properties)
}

pub fn definitions() -> Result<Vec<ToolDefinition>, DeveloperLoopError> {
    let text = json!({"type":"string","minLength":1,"maxLength":32768});
    let short = json!({"type":"string","minLength":1,"maxLength":1024});
    let texts = json!({"type":"array","minItems":1,"maxItems":64,"items":short});
    let version = json!({"type":"integer","minimum":1});
    let nullable_text = json!({"anyOf":[{"type":"null"},short]});
    let reference = json!({"anyOf":[
        object(json!({"kind":{"type":"string","enum":["repository"]},"path":short,"sha256":{"type":"string","minLength":64,"maxLength":64}})),
        object(json!({"kind":{"type":"string","enum":["web"]},"source":{"type":"string","minLength":64,"maxLength":64},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}})),
        object(json!({"kind":{"type":"string","enum":["source_range"]},"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":1}})),
        object(json!({"kind":{"type":"string","enum":["engineering_rationale"]},"invariant":text}))
    ]});
    let question =
        object(json!({"question":short,"external_authority_required":{"type":"boolean"}}));
    let domain = object(
        json!({"discipline":short,"applicability":short,"risks":texts,"questions":{"type":"array","minItems":1,"maxItems":32,"items":question}}),
    );
    let evidence = object(
        json!({"title":short,"version":nullable_text,"authority":{"type":"string","enum":["source_design","repository","primary","official","engineering_rationale"]},"reference":reference}),
    );
    Ok(vec![
        definition(
            PLAN,
            "Identify the applicable domains and concrete questions that can change this design. Append new domains only when research discovers a new obligation. External-authority questions require opened primary/official evidence. This saves a worklist, not a completed design; question IDs are returned by the host.",
            &mutation(
                json!({"expected_version":version,"domains":{"type":"array","minItems":1,"maxItems":32,"items":domain}}),
            ),
        )?,
        definition(
            AMEND,
            "Save or replace one useful amendment resolving a named research question. Supply new expert judgment, not an entire ledger. Include exact evidence and observable acceptance. Web references use source/start/end from retained pages; the host supplies URLs, digests, IDs and links. Cite repository path/sha256 from a current observation. Source/user obligations need source_range evidence. An unchanged retry is idempotent.",
            &mutation(
                json!({"expected_version":version,"question_id":short,"heading":short,"target_section":short,"markdown":text,
            "class":{"type":"string","enum":["user","mandatory_inferred","supporting_change","material_recommendation","optional","conflict","unknown"]},
            "rationale":text,"affected_contracts":texts,"failure_behavior":text,"acceptance":texts,"evidence":{"type":"array","minItems":1,"maxItems":64,"items":evidence}}),
            ),
        )?,
        definition(
            PUBLISH,
            "After reviewing the complete retained source and amendments, publish the current draft by version. Every planned question must have an evidenced amendment; critical conflicts/unknowns block publication. The host validates current source, evidence, full records and artifact identities, then preserves every required checkpoint. No full-document resubmission is needed.",
            &mutation(json!({"expected_version":version,"review":text})),
        )?,
        definition(
            READ,
            "Read a bounded UTF-8 page of the current source, assembled draft, records, recovered proposal, or previous expert pass. Use this only when those bytes are needed; current versions and unresolved questions are already supplied in host guidance. Follow next_offset for continuation.",
            &object(
                json!({"kind":{"type":"string","enum":["source","draft","records","proposal","previous"]},"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":4,"maximum":32768}}),
            ),
        )?,
        definition(
            SELECT,
            "Select an observed workspace design by exact relative path and SHA-256. The host validates scope and bytes, preserves the original instructions, and captures the complete document as source evidence. This resets the source binding; do this before research when a supplied document is not already captured.",
            &mutation(
                json!({"path":short,"sha256":{"type":"string","minLength":64,"maxLength":64}}),
            ),
        )?,
        definition(
            RECOVER,
            "Import a retained proposal or prior expert pass once as unapproved candidate work. Import does not approve old evidence; corrections and publication validate current evidence and source ranges. Inspect the recovered draft with design_pass_read, reconcile current instructions, and review before publishing. This never approves a design.",
            &mutation(json!({})),
        )?,
        definition(
            PATCH,
            "Update only changed parts of an imported candidate. Markdown before text must match exactly once. Each record replacement_json is one complete JSON record with the same id; null removes a nonrequired record. All links and current evidence are validated atomically. Required user/inferred/supporting obligations cannot be deleted or weakened.",
            &mutation(json!({"expected_version":version,
            "markdown_changes":{"type":"array","maxItems":64,"items":object(json!({"before":text,"after":{"type":"string","maxLength":32768}}))},
            "record_changes":{"type":"array","maxItems":128,"items":object(json!({"collection":{"type":"string","enum":["domains","evidence","obligations","acceptance_mappings","change_ledger"]},"id":short,"replacement_json":{"anyOf":[{"type":"null"},text]}}))}})),
        )?,
    ])
}
