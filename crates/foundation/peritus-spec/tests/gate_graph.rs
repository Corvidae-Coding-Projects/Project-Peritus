//! Adversarial validation of deterministic gate dependency graphs.

mod support;

use peritus_spec::{CanonicalCollection, GateDefinition, GateGraph, SpecError};

#[test]
fn graph_returns_a_deterministic_dependency_order() {
    let graph = GateGraph::new(vec![
        support::gate(1, vec![], vec![]),
        support::gate(2, vec![], vec![]),
        support::gate(3, vec![support::gate_id(1), support::gate_id(2)], vec![]),
    ])
    .expect("DAG");

    assert_eq!(
        graph.execution_order(),
        &[support::gate_id(1), support::gate_id(2), support::gate_id(3)]
    );
    assert_eq!(graph.get(support::gate_id(2)).map(GateDefinition::id), Some(support::gate_id(2)));
    assert!(graph.get(support::gate_id(9)).is_none());
    assert!(graph.dependency_precedes(support::gate_id(1), support::gate_id(3)));
    assert!(graph.dependency_precedes(support::gate_id(2), support::gate_id(3)));
    assert!(!graph.dependency_precedes(support::gate_id(3), support::gate_id(1)));
}

#[test]
fn graph_rejects_unknown_dependencies_and_cycles() {
    assert_eq!(
        GateGraph::new(vec![support::gate(1, vec![support::gate_id(2)], vec![])]),
        Err(SpecError::UnknownGateDependency {
            gate: support::gate_id(1),
            dependency: support::gate_id(2),
        })
    );

    let cycle = GateGraph::new(vec![
        support::gate(1, vec![support::gate_id(2)], vec![]),
        support::gate(2, vec![support::gate_id(1)], vec![]),
    ]);
    assert_eq!(cycle, Err(SpecError::GateCycle));
}

#[test]
fn gate_and_graph_constructors_reject_ambiguous_ordering() {
    assert_eq!(
        GateDefinition::new(
            support::gate_id(2),
            support::plan(2),
            vec![support::gate_id(1), support::gate_id(1)],
            vec![],
        ),
        Err(SpecError::DuplicateCanonicalValue(CanonicalCollection::GateDependencies))
    );
    assert_eq!(
        GateDefinition::new(
            support::gate_id(2),
            support::plan(2),
            vec![support::gate_id(2)],
            vec![],
        ),
        Err(SpecError::SelfDependency(support::gate_id(2)))
    );
    assert_eq!(
        GateGraph::new(vec![support::gate(2, vec![], vec![]), support::gate(1, vec![], vec![])]),
        Err(SpecError::NonCanonicalOrder(CanonicalCollection::Gates))
    );
}
