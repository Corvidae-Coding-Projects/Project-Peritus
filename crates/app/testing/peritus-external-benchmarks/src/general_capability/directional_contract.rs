use peritus_obligations::{
    ObligationLimits, SchemaDirection, SchemaEvidence, SchemaField, SchemaFieldId,
    SchemaRequirement,
};
use serde::Deserialize;

use super::fixture::{Expected, FixtureSet, binding, digest};

const CASES: &str =
    include_str!("../../tests/fixtures/general-capability/directional-contract/cases.json");

#[derive(Deserialize)]
struct Case {
    name: String,
    request_fields: Vec<Field>,
    response_fields: Vec<Field>,
    expected: Expected,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Field {
    Value,
    Val,
}

#[test]
fn request_and_response_field_names_are_directionally_exact() {
    let limits = ObligationLimits::production();
    let request =
        SchemaRequirement::new(SchemaDirection::Request, vec![field(Field::Value)], limits)
            .expect("request requirement");
    let response =
        SchemaRequirement::new(SchemaDirection::Response, vec![field(Field::Val)], limits)
            .expect("response requirement");
    let fixtures: FixtureSet<Case> = serde_json::from_str(CASES).expect("schema fixtures");

    for fixture in fixtures.cases {
        let request_evidence = SchemaEvidence::new(
            binding(41, 42),
            SchemaDirection::Request,
            ids(&fixture.request_fields),
            limits,
        )
        .expect("request evidence");
        let response_evidence = SchemaEvidence::new(
            binding(43, 44),
            SchemaDirection::Response,
            ids(&fixture.response_fields),
            limits,
        )
        .expect("response evidence");
        let qualified = request_evidence.covers(&request) && response_evidence.covers(&response);
        assert_eq!(qualified, fixture.expected == Expected::Success, "{}", fixture.name);
        if fixture.expected == Expected::Partial {
            assert!(request_evidence.covers(&request));
            assert!(!response_evidence.covers(&response));
        }
    }
}

fn field(field: Field) -> SchemaField {
    let (id, name) = match field {
        Field::Value => (SchemaFieldId::new(digest(1)), b"value".to_vec()),
        Field::Val => (SchemaFieldId::new(digest(2)), b"val".to_vec()),
    };
    SchemaField::new(id, name, 32).expect("schema field")
}

fn ids(fields: &[Field]) -> Vec<SchemaFieldId> {
    let mut ids = fields.iter().copied().map(|value| field(value).id()).collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}
