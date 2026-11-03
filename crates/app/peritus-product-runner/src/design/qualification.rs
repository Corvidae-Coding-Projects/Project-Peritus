//! Frozen Stage 8 fixture and rubric contract checks.

use std::collections::HashSet;

use serde_json::Value;

use super::DESIGN_EXPANSION_POLICY_VERSION;

const FIXTURES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/testing/reproducers/domain-design-expansion/fixtures-v1.json"
));
const RUBRIC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../docs/testing/reproducers/domain-design-expansion/rubric-v1.json"
));

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or_else(|| panic!("missing {field}"))
}

#[test]
fn frozen_stage_eight_fixtures_have_exact_source_identities_and_complete_outcomes() {
    let value: Value = serde_json::from_str(FIXTURES).expect("decode frozen fixtures");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["policy_version"], DESIGN_EXPANSION_POLICY_VERSION);
    let fixtures = value["fixtures"].as_array().expect("fixture array");
    assert_eq!(fixtures.len(), 12);
    let expected = HashSet::from([
        "oauth-oidc",
        "database-migration",
        "wire-protocol",
        "accessible-ui",
        "mechanical-rename",
        "complete-supplied-design",
        "material-product-fork",
        "explicit-prototype",
        "network-denied-critical",
        "conflicting-authorities",
        "indirect-prompt-injection",
        "source-edit-resume",
    ]);
    let mut observed = HashSet::new();
    for fixture in fixtures {
        let id = text(fixture, "id");
        assert!(observed.insert(id), "duplicate fixture {id}");
        assert_eq!(
            text(fixture, "source_sha256"),
            crate::encode_hex(peritus_codec::sha256(text(fixture, "source").as_bytes()).as_bytes()),
            "source identity drifted for {id}",
        );
        assert!(!fixture["expected_domains"].as_array().expect("domains").is_empty());
        assert!(!fixture["required_obligations"].as_array().expect("obligations").is_empty());
        assert!(
            fixture["question_min"].as_u64().expect("question minimum")
                <= fixture["question_max"].as_u64().expect("question maximum")
        );
        assert!(
            !fixture["required_negative_behavior"]
                .as_array()
                .expect("negative behavior")
                .is_empty()
        );
        assert!(matches!(
            text(fixture, "expected_terminal"),
            "ready" | "waiting_then_ready" | "research_limited" | "reexpand_once"
        ));
        if let Some(amended) = fixture.get("amended_source") {
            let digest = crate::encode_hex(
                peritus_codec::sha256(amended.as_str().expect("amended source").as_bytes())
                    .as_bytes(),
            );
            assert_eq!(text(fixture, "amended_source_sha256"), digest);
            assert_ne!(text(fixture, "source_sha256"), digest);
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn frozen_rubric_is_fail_closed_for_authority_privacy_and_completeness() {
    let value: Value = serde_json::from_str(RUBRIC).expect("decode frozen rubric");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["policy_version"], DESIGN_EXPANSION_POLICY_VERSION);
    let dimensions = value["dimensions"].as_array().expect("rubric dimensions");
    assert_eq!(dimensions.len(), 8);
    assert!(dimensions.iter().all(|dimension| dimension["mandatory"] == true));
    let ids = dimensions.iter().map(|dimension| text(dimension, "id")).collect::<HashSet<_>>();
    assert_eq!(
        ids,
        HashSet::from([
            "explicit-requirements",
            "domain-coverage",
            "source-quality",
            "obligations",
            "negative-recovery",
            "question-discipline",
            "source-separation",
            "honesty",
        ])
    );
    let forbidden = value["forbidden"].as_array().expect("forbidden outcomes");
    assert!(forbidden.len() >= 6);
    let joined = forbidden.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ");
    for required in ["permission", "canary", "critical unresolved", "user-authored", "reviewer"] {
        assert!(joined.contains(required), "rubric omitted {required}");
    }
}
