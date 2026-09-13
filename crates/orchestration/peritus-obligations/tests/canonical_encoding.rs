//! Compatibility regression for the public ledger digest preimage.

mod support;

use peritus_obligations::{
    AlternativeBranchId, AlternativeGroupId, BrowserRequirement, ConditionId, LifecycleRequirement,
    ObligationLimits, ObligationSpec, PathMention, PathRole, PerformanceExpectation,
    PerformanceRequirement, PerformanceStatistic, SchemaDirection, SchemaField, SchemaFieldId,
    SchemaRequirement,
};
use support::{digest, ledger, path_id};

fn schema_field(identity: u8, name: &[u8]) -> SchemaField {
    SchemaField::new(
        SchemaFieldId::new(digest(identity)),
        name.to_vec(),
        ObligationLimits::production().max_clause_bytes(),
    )
    .expect("bounded schema field")
}

fn path(identity: u8, exact: &[u8], role: PathRole) -> PathMention {
    PathMention::new(
        path_id(identity),
        exact.to_vec(),
        role,
        ObligationLimits::production().max_clause_bytes(),
    )
    .expect("bounded path")
}

#[test]
fn public_extraction_retains_the_existing_canonical_digest() {
    let limits = ObligationLimits::production();
    let group = AlternativeGroupId::new(digest(13));
    let request = SchemaRequirement::new(
        SchemaDirection::Request,
        vec![schema_field(21, b"alpha"), schema_field(22, &[0xff, b'b'])],
        limits,
    )
    .expect("request schema");
    let response =
        SchemaRequirement::new(SchemaDirection::Response, vec![schema_field(23, b"omega")], limits)
            .expect("response schema");
    let performance = PerformanceRequirement::new(
        digest(16),
        PerformanceStatistic::Percentile99,
        7,
        PerformanceExpectation::ImprovementAtLeast(123_456_789),
    )
    .expect("performance requirement");
    let ledger = ledger(vec![
        (
            1,
            b"hard\0binary",
            ObligationSpec::Hard,
            vec![
                path(1, b"out/result", PathRole::RequiredOutput),
                path(2, b"src/input", PathRole::RequiredModification),
                path(3, b"config/input", PathRole::RequiredInput),
                path(4, b"docs/reference", PathRole::Reference),
                path(5, b"examples/sample", PathRole::Example),
            ],
        ),
        (
            2,
            b"conditional",
            ObligationSpec::Conditional { condition_id: ConditionId::new(digest(12)) },
            Vec::new(),
        ),
        (
            3,
            b"alternative-a",
            ObligationSpec::Alternative {
                group_id: group,
                branch_id: AlternativeBranchId::new(digest(14)),
            },
            Vec::new(),
        ),
        (
            4,
            b"alternative-b",
            ObligationSpec::Alternative {
                group_id: group,
                branch_id: AlternativeBranchId::new(digest(15)),
            },
            Vec::new(),
        ),
        (5, b"example", ObligationSpec::Example, Vec::new()),
        (6, b"generated", ObligationSpec::GeneratedOutput, Vec::new()),
        (7, b"performance", ObligationSpec::Performance(performance), Vec::new()),
        (
            8,
            b"lifecycle",
            ObligationSpec::LifecycleIngress(LifecycleRequirement::new(
                digest(17),
                digest(18),
                digest(19),
                digest(20),
            )),
            Vec::new(),
        ),
        (9, b"request-schema", ObligationSpec::RequestSchema(request), Vec::new()),
        (10, b"response-schema", ObligationSpec::ResponseSchema(response), Vec::new()),
        (
            11,
            b"browser",
            ObligationSpec::BrowserSemantics(BrowserRequirement::new(digest(24))),
            Vec::new(),
        ),
        (
            12,
            b"external-effect",
            ObligationSpec::ExternalEffect { effect_identity: digest(25) },
            Vec::new(),
        ),
    ]);

    assert_eq!(
        ledger.digest().as_bytes(),
        &[
            0x73, 0xee, 0x3a, 0x1d, 0x6b, 0x4f, 0x33, 0x16, 0x6b, 0x98, 0xbd, 0xe4, 0x8c, 0xf3,
            0x7f, 0x7f, 0xaf, 0x90, 0xf3, 0xcf, 0xfa, 0xe7, 0x76, 0xe9, 0xed, 0x4f, 0x92, 0x9b,
            0xa2, 0x06, 0xb8, 0xa4,
        ]
    );
}
