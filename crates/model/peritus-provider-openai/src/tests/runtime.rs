//! Direct Codex runtime projection, decoder, process, and cancellation tests.

use std::collections::BTreeSet;

use peritus_model_protocol::{Capability, RequestedCapabilities, negotiate};
use peritus_provider_core::ProcessLimits;

use super::runtime_support::{
    codex_image_profile, codex_image_request, codex_profile, codex_tool_request,
};
use super::support::{fixture, model_limits, profile_minimal};
use crate::runtime::output::{DecodeFailure, decode};
use crate::runtime::request;
use crate::{CodexExecutable, CodexRuntimeConfig};

#[test]
fn profile_and_projection_are_exact_and_minimum_safe() {
    let profile = codex_profile("runtime-tool", true);
    let request = codex_tool_request(&profile, "runtime-projection");
    let encoded = request::encode(&request).expect("runtime projection");
    let prompt = core::str::from_utf8(&encoded.prompt).expect("prompt UTF-8");
    let schema: serde_json::Value = serde_json::from_slice(&encoded.schema).expect("schema JSON");
    assert!(prompt.contains("PERITUS_PROVIDER_REQUEST_JSON"));
    assert!(prompt.contains("host_tools"));
    assert!(prompt.contains("max_output_tokens_advisory"));
    assert_eq!(encoded.reasoning_effort(), "high");
    assert_eq!(
        schema.pointer("/properties/tool_calls/items/properties/name/enum/0"),
        Some(&serde_json::Value::String("lookup".to_owned()))
    );
    assert_eq!(
        schema.pointer("/properties/tool_calls/items/properties/arguments_json/type"),
        Some(&serde_json::Value::String("string".to_owned()))
    );
    assert!(!schema.to_string().contains("oneOf"));
    assert_eq!(encoded.prompt, without_final_newline(&fixture("runtime-golden-prompt.txt")));
    assert_eq!(encoded.schema, without_final_newline(&fixture("runtime-golden-schema.json")));
    assert!(CodexRuntimeConfig::new(
        missing_executable(),
        profile_minimal(),
        ProcessLimits::PRODUCTION,
    )
    .is_err());
}

#[test]
fn inline_image_is_staged_outside_the_prompt_with_a_digest_descriptor() {
    let profile = codex_image_profile("runtime-image");
    let request = codex_image_request(&profile, "runtime-image-projection");
    let encoded = request::encode(&request).expect("runtime image projection");
    let prompt = core::str::from_utf8(&encoded.prompt).expect("prompt UTF-8");

    assert!(prompt.contains("image_attachment"));
    assert!(prompt.contains("attachment_index"));
    assert!(prompt.contains("sha256"));
    assert!(!prompt.contains("bounded-image-bytes"));
    assert!(
        CodexRuntimeConfig::new(missing_executable(), profile, ProcessLimits::PRODUCTION,).is_ok()
    );
}

#[test]
fn decoder_accepts_current_lifecycle_and_exact_duplicates() {
    let allowed = BTreeSet::from(["lookup".to_owned()]);
    let success = decode(&fixture("runtime-success.jsonl"), &allowed, 2).expect("success");
    assert_eq!(success.content, "fixture response");
    assert_eq!(success.usage.input_tokens(), Some(12));
    let ordered = decode(&fixture("runtime-ordered-duplicate.jsonl"), &allowed, 2)
        .expect("ordered transcript");
    assert_eq!(ordered.raw_events, 5);
    assert_eq!(ordered.duplicates, 1);
    let tool = decode(&fixture("runtime-tool.jsonl"), &allowed, 2).expect("tool transcript");
    assert_eq!(tool.tool_calls.len(), 1);
    assert_eq!(tool.tool_calls[0].name, "lookup");
}

#[test]
fn decoder_rejects_corruption_incompletion_and_native_execution() {
    let allowed = BTreeSet::new();
    assert!(matches!(
        decode(&fixture("runtime-malformed.jsonl"), &allowed, 0),
        Err(DecodeFailure::Malformed)
    ));
    assert!(matches!(
        decode(&fixture("runtime-incomplete.jsonl"), &allowed, 0),
        Err(DecodeFailure::Incomplete)
    ));
    assert!(matches!(
        decode(&fixture("runtime-native-tool.jsonl"), &allowed, 0),
        Err(DecodeFailure::NativeTool)
    ));
    assert!(matches!(
        decode(&fixture("runtime-auth-error.jsonl"), &allowed, 0),
        Err(DecodeFailure::Authentication)
    ));
}

#[test]
fn unsupported_capability_is_rejected_without_a_process() {
    let profile = codex_profile("runtime-capability", false);
    let requested =
        RequestedCapabilities::new(&[Capability::StrictStructuredOutput], &[], model_limits())
            .expect("requested capabilities");
    assert!(negotiate(&profile, requested).is_err());
}

fn missing_executable() -> CodexExecutable {
    CodexExecutable::pin(std::env::current_exe().expect("current executable"))
        .expect("test executable")
}

fn without_final_newline(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}
