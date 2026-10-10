//! Semantic validation of persisted behavior evidence and legacy compatibility.
use super::*;

pub(super) fn test_behavior_evidence(
    record: &RunRecord,
    operation: peritus_app_protocol::ControlOperationId,
) {
    let clean =
        serde_json::to_value(PersistedRecord::from_record(record).expect("persisted record"))
            .expect("json");
    let restore = |value: PersistedRecord| {
        restore_preview(
            value.preview_page,
            value.preview_operations,
            value.preview_outputs,
            record.request.run_id(),
            record.request.workspace_id(),
            &record.interaction,
        )
    };
    assert!(
        restore(serde_json::from_value(clean.clone()).unwrap()).is_ok(),
        "healthy evidence restores"
    );
    let index = clean["preview_operations"]
        .as_array()
        .unwrap()
        .iter()
        .position(|value| {
            value["operation"] == serde_json::to_value(operation.into_bytes()).unwrap()
        })
        .unwrap();
    for field in [
        "observed",
        "note",
        "launch",
        "matched_bytes_digest",
        "end_byte",
        "process_id",
        "artifact_digest",
        "binding_digest",
        "goal",
        "criterion_index",
        "user_revision",
        "required_input_generation",
    ] {
        let mut corrupt = clean.clone();
        let evidence = &mut corrupt["preview_operations"][index]["behavior_evidence"];
        match field {
            "observed" | "note" => evidence[field] = serde_json::json!("altered"),
            "launch" => evidence[field] = serde_json::to_value([255_u8; 16]).unwrap(),
            "matched_bytes_digest" => {
                evidence["observed_match"][field] = serde_json::to_value([0_u8; 32]).unwrap();
            }
            "end_byte" => evidence["observed_match"][field] = serde_json::json!(u64::MAX),
            "binding_digest" => evidence[field] = serde_json::to_value([0_u8; 32]).unwrap(),
            "process_id" | "artifact_digest" => {
                let source = evidence["observed_match"]["source"].as_object_mut().unwrap();
                let (kind, fields) = source.iter_mut().next().unwrap();
                let key = if field == "artifact_digest" && kind == "live_spool" {
                    "observed_prefix_digest"
                } else {
                    field
                };
                fields[key] = if field == "process_id" {
                    serde_json::to_value([255_u8; 16]).unwrap()
                } else {
                    serde_json::to_value([0_u8; 32]).unwrap()
                };
            }
            "goal" | "criterion_index" | "user_revision" | "required_input_generation" => {
                if evidence["goal"].is_null() {
                    continue;
                }
                evidence["goal"][field] = match field {
                    "goal" => serde_json::to_value([255_u8; 16]).unwrap(),
                    _ => serde_json::json!(evidence["goal"][field].as_u64().unwrap() + 1),
                };
            }
            _ => unreachable!(),
        }
        let decoded: PersistedRecord = serde_json::from_value(corrupt).expect("structural JSON");
        assert!(restore(decoded).is_err(), "reject corrupt {field}");
    }
    let mut legacy = clean;
    for operation in legacy["preview_operations"].as_array_mut().unwrap() {
        operation.as_object_mut().unwrap().remove("behavior_evidence");
    }
    let decoded: PersistedRecord = serde_json::from_value(legacy.clone()).expect("legacy JSON");
    assert_eq!(serde_json::to_value(&decoded).unwrap(), legacy, "legacy bytes gain no new fields");
    let restored = restore(decoded).expect("legacy restoration");
    assert!(restored.operations.values().all(|operation| operation.behavior_evidence.is_none()));
}
