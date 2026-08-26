//! Complete E1 component catalog and protected-class tests.

use peritus_codec::sha256;
use peritus_harness::domain::{
    Authority, AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentDeclaration,
    ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind, ComponentLocation,
    ComponentOwnership, ComponentRequirements, FeatureTag, GraphEnvironment,
    HarnessDomainErrorKind, HarnessLimitKind, HarnessLimits, MediaType, Owner, ProtectionClass,
    Provenance, SchemaInterval, SchemaVersion, SourcePath, TargetPath,
};

#[test]
fn complete_catalog_has_stable_unique_tags_and_every_protection_class() {
    assert_eq!(ComponentKind::ALL.len(), 30);
    for (index, kind) in ComponentKind::ALL.into_iter().enumerate() {
        assert_eq!(usize::from(kind.tag()), index + 1);
        assert_eq!(kind.protection_class().tag(), kind.protection_class() as u8);
    }
    for class in ProtectionClass::ALL {
        assert!(ComponentKind::ALL.into_iter().any(|kind| kind.protection_class() == class));
    }
    for kind in ComponentKind::ALL {
        let content = [kind.tag()];
        let schema = SchemaVersion::new(1).expect("schema");
        let id = format!("kind-{}", kind.tag());
        let declaration = ComponentDeclaration::new(
            ComponentIdentity::new(ComponentId::new(&id).expect("ID"), kind, schema),
            ComponentLocation::new(
                SourcePath::new(format!(".peritus-harness/components/{id}")).expect("source"),
                TargetPath::new(format!("catalog/{id}")).expect("target"),
                MediaType::new("application/octet-stream").expect("media"),
            ),
            ComponentIntegrity::new(1, sha256(&content), None),
            ComponentOwnership::new(
                Owner::new("catalog-test").expect("owner"),
                Provenance::new("compiled catalog").expect("provenance"),
            ),
            ComponentRequirements::new(
                Vec::new(),
                CompatibilityContract::new(
                    SchemaInterval::new(schema, schema).expect("interval"),
                    Vec::new(),
                    Vec::new(),
                )
                .expect("compatibility"),
                AuthoritySet::empty(),
                kind.protection_class(),
            ),
            HarnessLimits::compiled(),
        )
        .expect("catalog declaration");
        let graph = CheckedHarnessGraph::check(
            vec![declaration],
            &GraphEnvironment::new(Vec::new(), Vec::new()).expect("environment"),
            HarnessLimits::compiled(),
        )
        .expect("catalog graph");
        assert_eq!(
            CheckedHarnessGraph::decode_canonical(&graph.canonical_bytes())
                .expect("catalog round trip"),
            graph
        );
    }
}

#[test]
fn nominal_values_reject_noncanonical_or_protected_inputs() {
    assert_eq!(
        ComponentId::new("").expect_err("empty ID").kind(),
        HarnessDomainErrorKind::EmptyValue
    );
    assert_eq!(
        ComponentId::new("not/a/component").expect_err("portable ID").kind(),
        HarnessDomainErrorKind::InvalidValue
    );
    assert_eq!(
        SourcePath::new("components/a").expect_err("source root").kind(),
        HarnessDomainErrorKind::InvalidPath
    );
    assert_eq!(
        TargetPath::new(".git/hooks/pre-commit").expect_err("protected target").kind(),
        HarnessDomainErrorKind::ProtectedPath
    );
    assert_eq!(
        TargetPath::new("a/../b").expect_err("normalized target").kind(),
        HarnessDomainErrorKind::InvalidPath
    );
    assert_eq!(
        TargetPath::new("nested/.GIT/config").expect_err("nested protected target").kind(),
        HarnessDomainErrorKind::ProtectedPath
    );
    assert_eq!(
        TargetPath::new("NUL").expect_err("portable device alias").kind(),
        HarnessDomainErrorKind::InvalidPath
    );
    assert_eq!(
        FeatureTag::new("CUDA").expect_err("lowercase feature").kind(),
        HarnessDomainErrorKind::InvalidValue
    );
}

#[test]
fn intervals_feature_sets_and_authority_sets_are_strict() {
    let one = SchemaVersion::new(1).expect("schema one");
    let two = SchemaVersion::new(2).expect("schema two");
    assert_eq!(
        SchemaInterval::new(two, one).expect_err("inverted interval").kind(),
        HarnessDomainErrorKind::InvalidSchemaInterval
    );
    let feature = FeatureTag::new("linux").expect("feature");
    assert_eq!(
        CompatibilityContract::new(
            SchemaInterval::new(one, two).expect("interval"),
            vec![feature.clone(), feature],
            Vec::new(),
        )
        .expect_err("duplicate feature")
        .kind(),
        HarnessDomainErrorKind::NonCanonicalOrder
    );
    assert_eq!(
        AuthoritySet::new(vec![Authority::WorkspaceRead, Authority::ContextRead])
            .expect_err("authority order")
            .kind(),
        HarnessDomainErrorKind::NonCanonicalOrder
    );
}

#[test]
fn manifest_limits_can_only_tighten_compiled_ceilings() {
    assert_eq!(
        HarnessLimits::compiled().max_components(),
        peritus_patch::MAX_PATCH_OPERATIONS as u64,
    );
    assert_eq!(
        HarnessLimits::compiled().max_component_bytes(),
        peritus_patch::MAX_FILE_BYTES as u64,
    );
    assert_eq!(
        HarnessLimits::compiled().max_total_materialized_bytes(),
        peritus_patch::MAX_PATCH_BYTES as u64,
    );
    assert_eq!(HarnessLimits::compiled().max_state_bytes(), 16 * 1_024 * 1_024);
    let limits =
        HarnessLimits::compiled().tighten(HarnessLimitKind::Components, 8).expect("tightened");
    assert_eq!(limits.max_components(), 8);
    assert_eq!(
        limits.tighten(HarnessLimitKind::Components, 0).expect_err("zero").kind(),
        HarnessDomainErrorKind::InvalidLimit
    );
    assert_eq!(
        limits
            .tighten(HarnessLimitKind::Components, 9)
            .expect_err("a tightened value cannot be widened")
            .kind(),
        HarnessDomainErrorKind::LimitWidening
    );
    assert_eq!(
        limits
            .tighten(HarnessLimitKind::Components, HarnessLimits::compiled().max_components() + 1)
            .expect_err("widening")
            .kind(),
        HarnessDomainErrorKind::LimitWidening
    );
}

#[test]
fn declaration_enforces_compiled_protection_and_authority() {
    let limits = HarnessLimits::compiled();
    let content = b"policy";
    let identity = ComponentIdentity::new(
        ComponentId::new("policy").expect("ID"),
        ComponentKind::RolePrompt,
        SchemaVersion::new(1).expect("schema"),
    );
    let location = ComponentLocation::new(
        SourcePath::new(".peritus-harness/components/policy").expect("source"),
        TargetPath::new("harness/policy").expect("target"),
        MediaType::new("text/plain").expect("media"),
    );
    let integrity = ComponentIntegrity::new(
        u64::try_from(content.len()).expect("test content length"),
        sha256(content),
        None,
    );
    let ownership = ComponentOwnership::new(
        Owner::new("security").expect("owner"),
        Provenance::new("repository").expect("provenance"),
    );
    let compatibility = CompatibilityContract::new(
        SchemaInterval::new(
            SchemaVersion::new(1).expect("minimum"),
            SchemaVersion::new(1).expect("maximum"),
        )
        .expect("interval"),
        Vec::new(),
        Vec::new(),
    )
    .expect("compatibility");
    let wrong_protection = ComponentRequirements::new(
        Vec::new(),
        compatibility.clone(),
        AuthoritySet::empty(),
        ProtectionClass::SecurityRoot,
    );
    assert_eq!(
        ComponentDeclaration::new(
            identity.clone(),
            location.clone(),
            integrity,
            ownership.clone(),
            wrong_protection,
            limits,
        )
        .expect_err("manifest cannot promote or downgrade protection")
        .kind(),
        HarnessDomainErrorKind::ProtectionMismatch
    );
    let excessive_authority = ComponentRequirements::new(
        Vec::new(),
        compatibility,
        AuthoritySet::new(vec![Authority::NetworkAccess]).expect("canonical authority"),
        ProtectionClass::Evolvable,
    );
    assert_eq!(
        ComponentDeclaration::new(
            identity,
            location,
            integrity,
            ownership,
            excessive_authority,
            limits,
        )
        .expect_err("role prompts cannot request network")
        .kind(),
        HarnessDomainErrorKind::AuthorityExceeded
    );
}
