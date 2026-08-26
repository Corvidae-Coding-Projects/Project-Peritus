//! Exhaustive small-graph reference-property tests.

use std::collections::BTreeSet;

use peritus_codec::sha256;
use peritus_harness::domain::{
    AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentDeclaration, ComponentId,
    ComponentIdentity, ComponentIntegrity, ComponentKind, ComponentLocation, ComponentOwnership,
    ComponentRequirements, DependencyRequirement, GraphEnvironment, HarnessLimits, MediaType,
    Owner, Provenance, SchemaInterval, SchemaVersion, SourcePath, TargetPath,
    authority_is_non_widening, component_ids_are_unique, dependencies_are_resolved,
    graph_digest_is_bound, topological_order_is_complete,
};

fn declaration(id: &str, dependency_ids: &[&str]) -> ComponentDeclaration {
    let schema = SchemaVersion::new(1).expect("schema");
    let mut dependencies = dependency_ids
        .iter()
        .map(|dependency| {
            DependencyRequirement::new(
                ComponentId::new(*dependency).expect("dependency ID"),
                ComponentKind::RolePrompt,
                SchemaInterval::new(schema, schema).expect("interval"),
                None,
            )
        })
        .collect::<Vec<_>>();
    dependencies.sort_by(|left, right| left.component_id().cmp(right.component_id()));
    let content = id.as_bytes();
    ComponentDeclaration::new(
        ComponentIdentity::new(
            ComponentId::new(id).expect("component ID"),
            ComponentKind::RolePrompt,
            schema,
        ),
        ComponentLocation::new(
            SourcePath::new(format!(".peritus-harness/components/{id}")).expect("source path"),
            TargetPath::new(format!("generated/{id}")).expect("target path"),
            MediaType::new("application/octet-stream").expect("media type"),
        ),
        ComponentIntegrity::new(
            u64::try_from(content.len()).expect("test content length"),
            sha256(content),
            None,
        ),
        ComponentOwnership::new(
            Owner::new("property-test").expect("owner"),
            Provenance::new("exhaustive small graph").expect("provenance"),
        ),
        ComponentRequirements::new(
            dependencies,
            CompatibilityContract::new(
                SchemaInterval::new(schema, schema).expect("interval"),
                Vec::new(),
                Vec::new(),
            )
            .expect("compatibility"),
            AuthoritySet::empty(),
            ComponentKind::RolePrompt.protection_class(),
        ),
        HarnessLimits::compiled(),
    )
    .expect("declaration")
}

fn reference_topological_order(declarations: &[ComponentDeclaration]) -> Vec<ComponentId> {
    let mut resolved = BTreeSet::new();
    let mut order = Vec::with_capacity(declarations.len());
    while order.len() < declarations.len() {
        let next = declarations
            .iter()
            .filter(|declaration| !resolved.contains(declaration.id()))
            .filter(|declaration| {
                declaration
                    .dependencies()
                    .iter()
                    .all(|dependency| resolved.contains(dependency.component_id()))
            })
            .map(ComponentDeclaration::id)
            .min()
            .expect("generated graph is acyclic")
            .clone();
        resolved.insert(next.clone());
        order.push(next);
    }
    order
}

#[test]
fn all_small_ranked_dags_match_the_reference_checker_and_round_trip() {
    // Every edge points from a later item in this rank to an earlier item, so all 64 masks are
    // acyclic while still exercising topological ties that differ from component-ID order.
    let rank = ["c", "a", "d", "b"];
    let candidate_edges = [(1, 0), (2, 0), (2, 1), (3, 0), (3, 1), (3, 2)];
    for mask in 0_u8..64 {
        let declarations = rank
            .iter()
            .enumerate()
            .map(|(consumer, id)| {
                let dependencies = candidate_edges
                    .iter()
                    .enumerate()
                    .filter(|(bit, (from, _))| *from == consumer && mask & (1_u8 << *bit) != 0)
                    .map(|(_, (_, dependency))| rank[*dependency])
                    .collect::<Vec<_>>();
                declaration(id, &dependencies)
            })
            .collect::<Vec<_>>();
        let expected = reference_topological_order(&declarations);
        let graph = CheckedHarnessGraph::check(
            declarations.into_iter().rev().collect(),
            &GraphEnvironment::new(Vec::new(), Vec::new()).expect("environment"),
            HarnessLimits::compiled(),
        )
        .expect("ranked DAG");

        assert_eq!(graph.topological_order(), expected);
        assert!(component_ids_are_unique(&graph));
        assert!(dependencies_are_resolved(&graph));
        assert!(topological_order_is_complete(&graph));
        assert!(authority_is_non_widening(&graph));
        assert!(graph_digest_is_bound(&graph));
        assert_eq!(
            CheckedHarnessGraph::decode_canonical(&graph.canonical_bytes())
                .expect("canonical graph"),
            graph
        );
    }
}
