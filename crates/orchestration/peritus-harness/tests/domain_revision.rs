//! Immutable revision history and rollback-selection tests.

use peritus_codec::sha256;
use peritus_harness::domain::{
    AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentContents,
    ComponentDeclaration, ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind,
    ComponentLocation, ComponentOwnership, ComponentRequirements, GraphEnvironment,
    HarnessDomainErrorKind, HarnessHistory, HarnessLimits, HarnessRevision, LineageSeed,
    ManifestDigest, MediaType, Owner, Provenance, SchemaInterval, SchemaVersion, SourcePath,
    TargetPath, VerifiedComponentContent, history_is_append_only, protected_assets_are_invariant,
    rollback_is_ancestor,
};

fn declaration(id: &str, kind: ComponentKind, content: &[u8]) -> ComponentDeclaration {
    let schema = SchemaVersion::new(1).expect("schema");
    ComponentDeclaration::new(
        ComponentIdentity::new(ComponentId::new(id).expect("ID"), kind, schema),
        ComponentLocation::new(
            SourcePath::new(format!(".peritus-harness/components/{id}")).expect("source"),
            TargetPath::new(format!("harness/{id}")).expect("target"),
            MediaType::new("application/octet-stream").expect("media"),
        ),
        ComponentIntegrity::new(
            u64::try_from(content.len()).expect("length"),
            sha256(content),
            None,
        ),
        ComponentOwnership::new(
            Owner::new("harness-team").expect("owner"),
            Provenance::new("repository").expect("provenance"),
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
    .expect("declaration")
}

fn graph_and_contents(
    id: &str,
    kind: ComponentKind,
    content: &[u8],
) -> (CheckedHarnessGraph, ComponentContents) {
    let declaration = declaration(id, kind, content);
    let verified = VerifiedComponentContent::new(&declaration, content.to_vec()).expect("content");
    let graph = CheckedHarnessGraph::check(
        vec![declaration],
        &GraphEnvironment::new(Vec::new(), Vec::new()).expect("environment"),
        HarnessLimits::compiled(),
    )
    .expect("graph");
    let contents = ComponentContents::new(&graph, vec![verified]).expect("complete contents");
    (graph, contents)
}

fn assembled(
    entries: Vec<(ComponentDeclaration, Vec<u8>)>,
) -> (CheckedHarnessGraph, ComponentContents) {
    let verified = entries
        .iter()
        .map(|(declaration, bytes)| {
            VerifiedComponentContent::new(declaration, bytes.clone()).expect("content")
        })
        .collect();
    let graph = CheckedHarnessGraph::check(
        entries.into_iter().map(|(declaration, _)| declaration).collect(),
        &GraphEnvironment::new(Vec::new(), Vec::new()).expect("environment"),
        HarnessLimits::compiled(),
    )
    .expect("graph");
    let contents = ComponentContents::new(&graph, verified).expect("contents");
    (graph, contents)
}

fn genesis(kind: ComponentKind, content: &[u8]) -> HarnessRevision {
    let (graph, contents) = graph_and_contents("component", kind, content);
    HarnessRevision::genesis(
        LineageSeed::new(sha256(b"lineage")),
        ManifestDigest::new(sha256(b"manifest-one")),
        graph,
        &contents,
    )
    .expect("genesis")
}

fn successor(
    predecessor: &HarnessRevision,
    kind: ComponentKind,
    content: &[u8],
    manifest: &[u8],
) -> HarnessRevision {
    let (graph, contents) = graph_and_contents("component", kind, content);
    HarnessRevision::successor(predecessor, ManifestDigest::new(sha256(manifest)), graph, &contents)
        .expect("successor")
}

#[test]
fn exact_component_bytes_are_checked_before_revision_construction() {
    let declaration = declaration("component", ComponentKind::RolePrompt, b"expected");
    assert_eq!(
        VerifiedComponentContent::new(&declaration, b"short".to_vec())
            .expect_err("length mismatch")
            .kind(),
        HarnessDomainErrorKind::ContentLengthMismatch
    );
    let same_length = b"mismatch";
    assert_eq!(same_length.len(), b"expected".len());
    assert_eq!(
        VerifiedComponentContent::new(&declaration, same_length.to_vec())
            .expect_err("digest mismatch")
            .kind(),
        HarnessDomainErrorKind::ContentDigestMismatch
    );
}

#[test]
fn genesis_and_successor_are_deterministic_content_addressed_values() {
    let first = genesis(ComponentKind::RolePrompt, b"one");
    let second = genesis(ComponentKind::RolePrompt, b"one");
    assert_eq!(first, second);
    assert_eq!(first.number(), peritus_types::RevisionNumber::first());
    assert!(first.predecessor().is_none());
    let next = successor(&first, ComponentKind::RolePrompt, b"two", b"manifest-two");
    assert_eq!(next.predecessor(), Some(first.digest()));
    assert_eq!(next.harness_id(), first.harness_id());
    assert_ne!(next.digest(), first.digest());
    assert_eq!(
        HarnessRevision::decode_canonical(&first.canonical_bytes(), None).expect("genesis decode"),
        first
    );
    assert_eq!(
        HarnessRevision::decode_canonical(&next.canonical_bytes(), Some(&first))
            .expect("successor decode"),
        next
    );
}

#[test]
fn every_structural_change_to_a_protected_asset_rejects() {
    let first = genesis(ComponentKind::MetricDefinition, b"metric-one");
    let (changed_graph, changed_contents) =
        graph_and_contents("component", ComponentKind::MetricDefinition, b"metric-two");
    assert_eq!(
        HarnessRevision::successor(
            &first,
            ManifestDigest::new(sha256(b"changed")),
            changed_graph,
            &changed_contents,
        )
        .expect_err("protected content drift")
        .kind(),
        HarnessDomainErrorKind::ProtectedAssetDrift
    );

    let (same_graph, same_contents) =
        graph_and_contents("component", ComponentKind::MetricDefinition, b"metric-one");
    let successor = HarnessRevision::successor(
        &first,
        ManifestDigest::new(sha256(b"new-manifest-only")),
        same_graph,
        &same_contents,
    )
    .expect("unchanged protected asset");
    assert!(protected_assets_are_invariant(&first, &successor));
}

#[test]
fn protected_asset_addition_removal_and_reordering_reject() {
    let role = declaration("role", ComponentKind::RolePrompt, b"role");
    let metric = declaration("metric", ComponentKind::MetricDefinition, b"metric");
    let metric_for_addition = metric.clone();
    let (initial_graph, initial_contents) =
        assembled(vec![(metric.clone(), b"metric".to_vec()), (role.clone(), b"role".to_vec())]);
    let initial = HarnessRevision::genesis(
        LineageSeed::new(sha256(b"protected-lineage")),
        ManifestDigest::new(sha256(b"protected-manifest")),
        initial_graph,
        &initial_contents,
    )
    .expect("genesis");
    let (reordered_graph, reordered_contents) =
        assembled(vec![(role.clone(), b"role".to_vec()), (metric, b"metric".to_vec())]);
    assert_eq!(
        HarnessRevision::successor(
            &initial,
            ManifestDigest::new(sha256(b"reordered")),
            reordered_graph,
            &reordered_contents,
        )
        .expect_err("protected position drift")
        .kind(),
        HarnessDomainErrorKind::ProtectedAssetDrift
    );
    let (evolvable_graph, evolvable_contents) = assembled(vec![(
        declaration("role", ComponentKind::RolePrompt, b"role"),
        b"role".to_vec(),
    )]);
    let evolvable = HarnessRevision::genesis(
        LineageSeed::new(sha256(b"evolvable-lineage")),
        ManifestDigest::new(sha256(b"evolvable-manifest")),
        evolvable_graph,
        &evolvable_contents,
    )
    .expect("evolvable genesis");
    let (added_graph, added_contents) = assembled(vec![
        (metric_for_addition, b"metric".to_vec()),
        (declaration("role", ComponentKind::RolePrompt, b"role"), b"role".to_vec()),
    ]);
    assert_eq!(
        HarnessRevision::successor(
            &evolvable,
            ManifestDigest::new(sha256(b"added")),
            added_graph,
            &added_contents,
        )
        .expect_err("protected addition")
        .kind(),
        HarnessDomainErrorKind::ProtectedAssetDrift
    );
    let (removed_graph, removed_contents) = assembled(vec![(role, b"role".to_vec())]);
    assert_eq!(
        HarnessRevision::successor(
            &initial,
            ManifestDigest::new(sha256(b"removed")),
            removed_graph,
            &removed_contents,
        )
        .expect_err("protected removal")
        .kind(),
        HarnessDomainErrorKind::ProtectedAssetDrift
    );
}

#[test]
fn history_retains_branches_and_allows_only_strict_ancestor_rollback() {
    let root = genesis(ComponentKind::RolePrompt, b"root");
    let left = successor(&root, ComponentKind::RolePrompt, b"left", b"left-manifest");
    let right = successor(&root, ComponentKind::RolePrompt, b"right", b"right-manifest");
    let left_tip = successor(&left, ComponentKind::RolePrompt, b"left-tip", b"tip-manifest");
    let before = HarnessHistory::new(root.clone(), HarnessLimits::compiled()).expect("history");
    let mut history = before.clone();
    history.append(left).expect("left branch");
    history.append(right.clone()).expect("right branch");
    history.append(left_tip.clone()).expect("left tip");
    assert!(history_is_append_only(&before, &history));
    assert_eq!(history.children(root.digest()).len(), 2);
    assert_eq!(history.branch_tips().len(), 2);
    assert!(history.is_ancestor(root.digest(), left_tip.digest()));
    assert!(rollback_is_ancestor(&history, left_tip.digest(), root.digest()));
    assert!(!rollback_is_ancestor(&history, left_tip.digest(), right.digest()));
    assert_eq!(
        history
            .validate_rollback(left_tip.digest(), root.digest())
            .expect("ancestor rollback")
            .target()
            .digest(),
        root.digest()
    );
    assert_eq!(
        history
            .validate_rollback(left_tip.digest(), right.digest())
            .expect_err("sibling is not ancestor")
            .kind(),
        HarnessDomainErrorKind::RollbackNotAncestor
    );
    assert_eq!(
        history
            .validate_rollback(left_tip.digest(), left_tip.digest())
            .expect_err("rollback is strict")
            .kind(),
        HarnessDomainErrorKind::RollbackNotAncestor
    );
}

#[test]
fn history_snapshot_is_deterministic_and_fully_rechecked() {
    let root = genesis(ComponentKind::RolePrompt, b"root");
    let left = successor(&root, ComponentKind::RolePrompt, b"left", b"left-manifest");
    let right = successor(&root, ComponentKind::RolePrompt, b"right", b"right-manifest");
    let mut first = HarnessHistory::new(root.clone(), HarnessLimits::compiled()).expect("first");
    first.append(left.clone()).expect("left");
    first.append(right.clone()).expect("right");
    let mut second = HarnessHistory::new(root, HarnessLimits::compiled()).expect("second");
    second.append(right).expect("right");
    second.append(left).expect("left");
    assert_eq!(first.canonical_snapshot(), second.canonical_snapshot());
    let decoded = HarnessHistory::decode_canonical_snapshot(
        &first.canonical_snapshot(),
        HarnessLimits::compiled(),
    )
    .expect("snapshot decode");
    assert_eq!(decoded.canonical_snapshot(), first.canonical_snapshot());

    let mut corrupted = first.canonical_snapshot();
    let final_byte = corrupted.last_mut().expect("nonempty snapshot");
    *final_byte ^= 1;
    assert!(
        HarnessHistory::decode_canonical_snapshot(&corrupted, HarnessLimits::compiled(),).is_err()
    );
}
