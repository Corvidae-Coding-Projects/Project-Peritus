//! Deterministic graph validation and canonical binding tests.

use peritus_codec::sha256;
use peritus_harness::domain::{
    Authority, AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentDeclaration,
    ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind, ComponentLocation,
    ComponentOwnership, ComponentRequirements, DependencyRequirement, FeatureTag, GraphEnvironment,
    HarnessDomainErrorKind, HarnessLimits, MediaType, Owner, Provenance, SchemaInterval,
    SchemaVersion, SourcePath, TargetPath, authority_is_non_widening, component_ids_are_unique,
    dependencies_are_resolved, graph_digest_is_bound, topological_order_is_complete,
};
use peritus_types::Sha256Digest;

#[derive(Clone)]
struct Dependency<'a> {
    id: &'a str,
    kind: ComponentKind,
    minimum: u32,
    maximum: u32,
    digest: Option<Sha256Digest>,
}

fn declaration(
    id: &str,
    kind: ComponentKind,
    schema: u32,
    content: &[u8],
    dependencies: &[Dependency<'_>],
    provider_features: &[&str],
    authority: &[Authority],
) -> ComponentDeclaration {
    let version = SchemaVersion::new(schema).expect("schema");
    let requirements = dependencies
        .iter()
        .map(|dependency| {
            DependencyRequirement::new(
                ComponentId::new(dependency.id).expect("dependency ID"),
                dependency.kind,
                SchemaInterval::new(
                    SchemaVersion::new(dependency.minimum).expect("minimum"),
                    SchemaVersion::new(dependency.maximum).expect("maximum"),
                )
                .expect("interval"),
                dependency.digest,
            )
        })
        .collect();
    let compatibility = CompatibilityContract::new(
        SchemaInterval::new(version, version).expect("own interval"),
        provider_features.iter().map(|tag| FeatureTag::new(*tag).expect("feature")).collect(),
        Vec::new(),
    )
    .expect("compatibility");
    ComponentDeclaration::new(
        ComponentIdentity::new(ComponentId::new(id).expect("ID"), kind, version),
        ComponentLocation::new(
            SourcePath::new(format!(".peritus-harness/components/{id}")).expect("source"),
            TargetPath::new(format!("harness/{id}")).expect("target"),
            MediaType::new("application/octet-stream").expect("media"),
        ),
        ComponentIntegrity::new(
            u64::try_from(content.len()).expect("test content length"),
            sha256(content),
            None,
        ),
        ComponentOwnership::new(
            Owner::new("harness-team").expect("owner"),
            Provenance::new("repository").expect("provenance"),
        ),
        ComponentRequirements::new(
            requirements,
            compatibility,
            AuthoritySet::new(authority.to_vec()).expect("authority"),
            kind.protection_class(),
        ),
        HarnessLimits::compiled(),
    )
    .expect("declaration")
}

fn empty_environment() -> GraphEnvironment {
    GraphEnvironment::new(Vec::new(), Vec::new()).expect("environment")
}

#[test]
fn graph_resolves_deterministically_and_round_trips_canonical_bytes() {
    let base = declaration(
        "base",
        ComponentKind::BaseInstructionFragment,
        1,
        b"base",
        &[],
        &[],
        &[Authority::ContextRead],
    );
    let role = declaration(
        "role",
        ComponentKind::RolePrompt,
        1,
        b"role",
        &[Dependency {
            id: "base",
            kind: ComponentKind::BaseInstructionFragment,
            minimum: 1,
            maximum: 1,
            digest: Some(sha256(b"base")),
        }],
        &[],
        &[Authority::ContextRead],
    );
    let graph = CheckedHarnessGraph::check(
        vec![role, base],
        &empty_environment(),
        HarnessLimits::compiled(),
    )
    .expect("checked graph");
    assert_eq!(
        graph.topological_order().iter().map(ComponentId::as_str).collect::<Vec<_>>(),
        vec!["base", "role"]
    );
    assert!(component_ids_are_unique(&graph));
    assert!(dependencies_are_resolved(&graph));
    assert!(topological_order_is_complete(&graph));
    assert!(authority_is_non_widening(&graph));
    assert!(graph_digest_is_bound(&graph));
    assert_eq!(
        CheckedHarnessGraph::decode_canonical(&graph.canonical_bytes()).expect("round trip"),
        graph
    );
}

#[test]
fn duplicate_missing_and_cycle_are_rejected() {
    let first = declaration("same", ComponentKind::RolePrompt, 1, b"a", &[], &[], &[]);
    let second = declaration("same", ComponentKind::RolePrompt, 1, b"a", &[], &[], &[]);
    assert_eq!(
        CheckedHarnessGraph::check(
            vec![first, second],
            &empty_environment(),
            HarnessLimits::compiled(),
        )
        .expect_err("duplicate")
        .kind(),
        HarnessDomainErrorKind::DuplicateComponent
    );

    let missing = declaration(
        "consumer",
        ComponentKind::RolePrompt,
        1,
        b"consumer",
        &[Dependency {
            id: "absent",
            kind: ComponentKind::RolePrompt,
            minimum: 1,
            maximum: 1,
            digest: None,
        }],
        &[],
        &[],
    );
    assert_eq!(
        CheckedHarnessGraph::check(vec![missing], &empty_environment(), HarnessLimits::compiled(),)
            .expect_err("missing")
            .kind(),
        HarnessDomainErrorKind::MissingDependency
    );

    let a = declaration(
        "a",
        ComponentKind::RolePrompt,
        1,
        b"a",
        &[Dependency {
            id: "b",
            kind: ComponentKind::RolePrompt,
            minimum: 1,
            maximum: 1,
            digest: None,
        }],
        &[],
        &[],
    );
    let b = declaration(
        "b",
        ComponentKind::RolePrompt,
        1,
        b"b",
        &[Dependency {
            id: "a",
            kind: ComponentKind::RolePrompt,
            minimum: 1,
            maximum: 1,
            digest: None,
        }],
        &[],
        &[],
    );
    assert_eq!(
        CheckedHarnessGraph::check(vec![a, b], &empty_environment(), HarnessLimits::compiled())
            .expect_err("cycle")
            .kind(),
        HarnessDomainErrorKind::DependencyCycle
    );
}

#[test]
fn dependency_kind_version_and_digest_are_independently_checked() {
    let target = declaration("target", ComponentKind::ReferenceBundle, 1, b"target", &[], &[], &[]);
    for (dependency, expected) in [
        (
            Dependency {
                id: "target",
                kind: ComponentKind::RolePrompt,
                minimum: 1,
                maximum: 1,
                digest: None,
            },
            HarnessDomainErrorKind::IncompatibleDependencyKind,
        ),
        (
            Dependency {
                id: "target",
                kind: ComponentKind::ReferenceBundle,
                minimum: 2,
                maximum: 2,
                digest: None,
            },
            HarnessDomainErrorKind::IncompatibleDependencyVersion,
        ),
        (
            Dependency {
                id: "target",
                kind: ComponentKind::ReferenceBundle,
                minimum: 1,
                maximum: 1,
                digest: Some(sha256(b"wrong")),
            },
            HarnessDomainErrorKind::DependencyDigestMismatch,
        ),
    ] {
        let consumer = declaration(
            "consumer",
            ComponentKind::RoleDefinition,
            1,
            b"consumer",
            &[dependency],
            &[],
            &[],
        );
        assert_eq!(
            CheckedHarnessGraph::check(
                vec![consumer, target.clone()],
                &empty_environment(),
                HarnessLimits::compiled(),
            )
            .expect_err("incompatible dependency")
            .kind(),
            expected
        );
    }
}

#[test]
fn unknown_features_remain_inert_and_unsatisfied() {
    let component = declaration(
        "provider",
        ComponentKind::ProviderProfile,
        1,
        b"provider",
        &[],
        &["model.responses-v2"],
        &[],
    );
    assert_eq!(
        CheckedHarnessGraph::check(
            vec![component.clone()],
            &empty_environment(),
            HarnessLimits::compiled(),
        )
        .expect_err("unsupported")
        .kind(),
        HarnessDomainErrorKind::UnsatisfiedProviderFeature
    );
    let environment = GraphEnvironment::new(
        vec![FeatureTag::new("model.responses-v2").expect("feature")],
        Vec::new(),
    )
    .expect("environment");
    assert!(
        CheckedHarnessGraph::check(vec![component], &environment, HarnessLimits::compiled(),)
            .is_ok()
    );
}

#[test]
fn transitive_authority_and_incompatible_protected_dependencies_reject() {
    let workspace_reader = declaration(
        "reader",
        ComponentKind::ReferenceBundle,
        1,
        b"reader",
        &[],
        &[],
        &[Authority::WorkspaceRead],
    );
    let prompt = declaration(
        "prompt",
        ComponentKind::RolePrompt,
        1,
        b"prompt",
        &[Dependency {
            id: "reader",
            kind: ComponentKind::ReferenceBundle,
            minimum: 1,
            maximum: 1,
            digest: None,
        }],
        &[],
        &[],
    );
    assert_eq!(
        CheckedHarnessGraph::check(
            vec![prompt, workspace_reader],
            &empty_environment(),
            HarnessLimits::compiled(),
        )
        .expect_err("closure widening")
        .kind(),
        HarnessDomainErrorKind::DependencyAuthorityExceeded
    );

    let metric =
        declaration("metric", ComponentKind::MetricDefinition, 1, b"metric", &[], &[], &[]);
    let prompt = declaration(
        "prompt",
        ComponentKind::RolePrompt,
        1,
        b"prompt",
        &[Dependency {
            id: "metric",
            kind: ComponentKind::MetricDefinition,
            minimum: 1,
            maximum: 1,
            digest: None,
        }],
        &[],
        &[],
    );
    assert_eq!(
        CheckedHarnessGraph::check(
            vec![prompt, metric],
            &empty_environment(),
            HarnessLimits::compiled(),
        )
        .expect_err("protected delegation")
        .kind(),
        HarnessDomainErrorKind::ProtectedDependency
    );
}

#[test]
fn declaration_order_is_identity_but_topological_ties_use_component_id() {
    let a = declaration("a", ComponentKind::RolePrompt, 1, b"a", &[], &[], &[]);
    let b = declaration("b", ComponentKind::RolePrompt, 1, b"b", &[], &[], &[]);
    let first = CheckedHarnessGraph::check(
        vec![b.clone(), a.clone()],
        &empty_environment(),
        HarnessLimits::compiled(),
    )
    .expect("first");
    let second =
        CheckedHarnessGraph::check(vec![a, b], &empty_environment(), HarnessLimits::compiled())
            .expect("second");
    assert_eq!(first.topological_order(), second.topological_order());
    assert_ne!(first.graph_digest(), second.graph_digest());
}
