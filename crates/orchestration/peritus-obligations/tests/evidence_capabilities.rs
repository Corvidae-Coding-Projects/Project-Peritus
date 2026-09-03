//! Generic capability fixtures for typed performance, lifecycle, schema, and browser evidence.

mod support;

use peritus_obligations::{
    BrowserEvidence, BrowserImplementation, BrowserRequirement, ExternalEffectEvidence,
    LifecycleEvidence, LifecycleObservationKind, LifecycleRequirement, ObligationSpec,
    PerformanceEvidence, PerformanceExpectation, PerformanceRequirement, PerformanceStatistic,
    RequirementEvidence, SchemaDirection, SchemaEvidence, SchemaField, SchemaFieldId,
    SchemaRequirement, qualify,
};

use support::{binding, candidate, digest, ledger};

#[test]
fn performance_requires_current_same_workload_measurement() {
    let candidate = candidate(11, 7, 2);
    let requirement = PerformanceRequirement::new(
        digest(31),
        PerformanceStatistic::Median,
        5,
        PerformanceExpectation::RegressionAtMost(3),
    )
    .expect("performance requirement");
    let ledger = ledger(vec![(
        1,
        b"The candidate must not regress by more than three units.",
        ObligationSpec::Performance(requirement),
        Vec::new(),
    )]);

    let missing = qualify(&ledger, &candidate, &[], &[]).expect("missing report");
    assert!(!missing.qualified());
    assert_eq!(missing.missing_count(), 1);

    let current = RequirementEvidence::Performance(
        PerformanceEvidence::new(
            binding(&ledger, candidate, 1, Vec::new(), 41),
            digest(31),
            100,
            102,
            7,
            PerformanceStatistic::Median,
            1,
            PerformanceExpectation::RegressionAtMost(3),
        )
        .expect("performance evidence"),
    );
    assert!(qualify(&ledger, &candidate, &[], &[current]).expect("current report").qualified());

    let old_candidate = support::candidate(10, 7, 1);
    let stale = RequirementEvidence::Performance(
        PerformanceEvidence::new(
            binding(&ledger, old_candidate, 1, Vec::new(), 42),
            digest(31),
            100,
            102,
            7,
            PerformanceStatistic::Median,
            1,
            PerformanceExpectation::RegressionAtMost(3),
        )
        .expect("stale evidence"),
    );
    let stale_report = qualify(&ledger, &candidate, &[], &[stale]).expect("stale report");
    assert!(!stale_report.qualified());
    assert_eq!(stale_report.stale_count(), 1);
}

#[test]
fn lifecycle_simulation_cannot_replace_public_ingress() {
    let candidate = candidate(11, 7, 2);
    let requirement = LifecycleRequirement::new(digest(51), digest(52), digest(53), digest(54));
    let ledger = ledger(vec![(
        1,
        b"A public restart must produce the named service transition and ready state.",
        ObligationSpec::LifecycleIngress(requirement),
        Vec::new(),
    )]);
    let internal = RequirementEvidence::Lifecycle(LifecycleEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 55),
        digest(51),
        digest(52),
        digest(53),
        digest(54),
        LifecycleObservationKind::InternalSimulation,
    ));
    let internal_report = qualify(&ledger, &candidate, &[], &[internal]).expect("internal report");
    assert!(!internal_report.qualified());
    assert_eq!(internal_report.invalid_count(), 1);

    let public = RequirementEvidence::Lifecycle(LifecycleEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 56),
        digest(51),
        digest(52),
        digest(53),
        digest(54),
        LifecycleObservationKind::PublicIngress,
    ));
    assert!(qualify(&ledger, &candidate, &[], &[public]).expect("public report").qualified());
}

#[test]
fn request_and_response_schema_fields_are_directional() {
    let limits = peritus_obligations::ObligationLimits::production();
    let value = SchemaField::new(SchemaFieldId::new(digest(61)), b"value".to_vec(), 64)
        .expect("request field");
    let val = SchemaField::new(SchemaFieldId::new(digest(62)), b"val".to_vec(), 64)
        .expect("response field");
    let request = SchemaRequirement::new(SchemaDirection::Request, vec![value], limits)
        .expect("request schema");
    let response = SchemaRequirement::new(SchemaDirection::Response, vec![val], limits)
        .expect("response schema");
    let candidate = candidate(11, 7, 2);
    let ledger = ledger(vec![
        (
            1,
            b"Requests require the field value.",
            ObligationSpec::RequestSchema(request),
            Vec::new(),
        ),
        (
            2,
            b"Responses require the field val.",
            ObligationSpec::ResponseSchema(response),
            Vec::new(),
        ),
    ]);

    let confused = vec![
        RequirementEvidence::Schema(
            SchemaEvidence::new(
                binding(&ledger, candidate, 1, Vec::new(), 63),
                SchemaDirection::Request,
                vec![SchemaFieldId::new(digest(62))],
                limits,
            )
            .expect("confused request"),
        ),
        RequirementEvidence::Schema(
            SchemaEvidence::new(
                binding(&ledger, candidate, 2, Vec::new(), 64),
                SchemaDirection::Response,
                vec![SchemaFieldId::new(digest(61))],
                limits,
            )
            .expect("confused response"),
        ),
    ];
    let confused_report = qualify(&ledger, &candidate, &[], &confused).expect("confused report");
    assert!(!confused_report.qualified());
    assert_eq!(confused_report.invalid_count(), 2);

    let exact = vec![
        RequirementEvidence::Schema(
            SchemaEvidence::new(
                binding(&ledger, candidate, 1, Vec::new(), 65),
                SchemaDirection::Request,
                vec![SchemaFieldId::new(digest(61))],
                limits,
            )
            .expect("request evidence"),
        ),
        RequirementEvidence::Schema(
            SchemaEvidence::new(
                binding(&ledger, candidate, 2, Vec::new(), 66),
                SchemaDirection::Response,
                vec![SchemaFieldId::new(digest(62))],
                limits,
            )
            .expect("response evidence"),
        ),
    ];
    assert!(qualify(&ledger, &candidate, &[], &exact).expect("exact report").qualified());
}

#[test]
fn malformed_html_claim_needs_a_real_browser_oracle() {
    let candidate = candidate(11, 7, 2);
    let requirement = BrowserRequirement::new(digest(71));
    let ledger = ledger(vec![(
        1,
        b"Malformed HTML must follow browser parsing and rendering semantics.",
        ObligationSpec::BrowserSemantics(requirement),
        Vec::new(),
    )]);
    let parser = RequirementEvidence::Browser(BrowserEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 72),
        BrowserImplementation::ParserOnly,
        None,
        true,
    ));
    assert!(!qualify(&ledger, &candidate, &[], &[parser]).expect("parser report").qualified());

    let browser = RequirementEvidence::Browser(BrowserEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 73),
        BrowserImplementation::StandardsCompliant,
        Some(digest(71)),
        true,
    ));
    assert!(qualify(&ledger, &candidate, &[], &[browser]).expect("browser report").qualified());
}

#[test]
fn external_effect_requires_public_boundary_completion() {
    let candidate = candidate(11, 7, 2);
    let effect_identity = digest(81);
    let ledger = ledger(vec![(
        1,
        b"Publish the requested result through the named external service.",
        ObligationSpec::ExternalEffect { effect_identity },
        Vec::new(),
    )]);
    let internal = RequirementEvidence::ExternalEffect(ExternalEffectEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 82),
        effect_identity,
        false,
        true,
    ));
    assert!(!qualify(&ledger, &candidate, &[], &[internal]).expect("internal report").qualified());

    let public = RequirementEvidence::ExternalEffect(ExternalEffectEvidence::new(
        binding(&ledger, candidate, 1, Vec::new(), 83),
        effect_identity,
        true,
        true,
    ));
    assert!(qualify(&ledger, &candidate, &[], &[public]).expect("public report").qualified());
}
