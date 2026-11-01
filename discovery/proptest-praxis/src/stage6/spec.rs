//! Independent small graph model for the immutable acceptance gate schedule.

use crate::Metrics;
use peritus_spec::{
    ContentReference, GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph,
    GateSuccessRule, SpecError,
};
use peritus_types::{EnvironmentId, GateId, Sha256Digest};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Graph { dependencies: Vec<u16> },
    Invalid { kind: u8 },
}

fn id(index: usize) -> GateId {
    let mut bytes = [0; 16];
    bytes[15] = u8::try_from(index + 1).expect("bounded gate index");
    GateId::new(bytes).expect("nonzero gate id")
}

fn plan(timeout: u64) -> Result<GateExecutionPlan, SpecError> {
    let content = ContentReference::new(Sha256Digest::new([7; 32]));
    let mut environment = [0; 16];
    environment[0] = 1;
    GateExecutionPlan::new(
        content,
        EnvironmentId::new(environment).expect("nonzero environment"),
        content,
        content,
        GateSuccessRule::ExitCodeZero,
        timeout,
        content,
        GateFreshnessScope::ExactRevisionTuple,
    )
}

fn definitions(masks: &[u16]) -> Result<Vec<GateDefinition>, SpecError> {
    let plan = plan(1)?;
    masks
        .iter()
        .enumerate()
        .map(|(index, mask)| {
            let dependencies = (0..masks.len())
                .filter(|dependency| *mask & (1 << dependency) != 0)
                .map(id)
                .collect();
            GateDefinition::new(id(index), plan, dependencies, Vec::new())
        })
        .collect()
}

/// A bitset interpreter chooses the lexicographically first valid topological order.
fn reference_order(masks: &[u16]) -> Option<(Vec<usize>, u64)> {
    let mut seen = 0_u16;
    let mut order = Vec::with_capacity(masks.len());
    let mut multiple_ready = 0;
    while order.len() < masks.len() {
        let ready = (0..masks.len())
            .filter(|index| seen & (1 << index) == 0 && masks[*index] & !seen == 0)
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return None;
        }
        if ready.len() > 1 {
            multiple_ready += 1;
        }
        let selected = ready[0];
        seen |= 1 << selected;
        order.push(selected);
    }
    Some((order, multiple_ready))
}

/// Enumerate every valid schedule for small graphs, including those the greedy model skips.
fn exhaustive_order(masks: &[u16]) -> Option<Vec<usize>> {
    fn visit(masks: &[u16], used: u16, prefix: &mut Vec<usize>, best: &mut Option<Vec<usize>>) {
        if prefix.len() == masks.len() {
            if best.as_ref().is_none_or(|old| prefix.as_slice() < old.as_slice()) {
                *best = Some(prefix.clone());
            }
            return;
        }
        for index in 0..masks.len() {
            if used & (1 << index) == 0 && masks[index] & !used == 0 {
                prefix.push(index);
                visit(masks, used | (1 << index), prefix, best);
                prefix.pop();
            }
        }
    }
    let mut best = None;
    visit(masks, 0, &mut Vec::new(), &mut best);
    best
}

fn bump(metrics: &mut Metrics, name: &str) {
    *metrics.entry(name.into()).or_default() += 1;
}

fn graph(masks: &[u16], metrics: &mut Metrics) -> Result<(), String> {
    if masks.is_empty() || masks.len() > 8 {
        return Err("setup-incomplete: gate graph must have 1..=8 nodes".into());
    }
    let valid_bits = (1_u16 << masks.len()) - 1;
    if masks.iter().any(|mask| mask & !valid_bits != 0) {
        return Err("setup-incomplete: gate dependency outside declared bitset".into());
    }
    let first_self = (0..masks.len()).find(|index| masks[*index] & (1 << index) != 0);
    let constructed = definitions(masks);
    if let Some(index) = first_self {
        if constructed != Err(SpecError::SelfDependency(id(index))) {
            return Err(format!("self dependency should reject gate {index}: {constructed:?}"));
        }
        bump(metrics, "self_dependency_rejections");
        return Ok(());
    }
    let actual = GateGraph::new(
        constructed.map_err(|error| format!("unexpected gate definition error: {error:?}"))?,
    );
    let expected = reference_order(masks);
    match (actual, expected) {
        (Err(SpecError::GateCycle), None) => bump(metrics, "cyclic_graph_rejections"),
        (Ok(graph), Some((indices, multiple))) => {
            if masks.len() <= 6 {
                let enumerated = exhaustive_order(masks);
                if enumerated.as_deref() != Some(indices.as_slice()) {
                    return Err(format!(
                        "greedy and exhaustive gate oracles differ: {indices:?} versus {enumerated:?}"
                    ));
                }
                bump(metrics, "exhaustive_small_graphs");
            }
            let expected_ids = indices.iter().copied().map(id).collect::<Vec<_>>();
            if graph.execution_order() != expected_ids {
                return Err(format!(
                    "gate schedule differs: expected {expected_ids:?}, got {:?}",
                    graph.execution_order()
                ));
            }
            for (index, mask) in masks.iter().enumerate() {
                let definition = graph.get(id(index)).ok_or("declared gate missing")?;
                let expected_dependencies = (0..masks.len())
                    .filter(|dependency| *mask & (1 << dependency) != 0)
                    .map(id)
                    .collect::<Vec<_>>();
                if definition.dependencies() != expected_dependencies {
                    return Err(format!("gate {index} dependencies changed"));
                }
                for dependency in 0..masks.len() {
                    let left = indices
                        .iter()
                        .position(|seen| *seen == dependency)
                        .expect("complete order");
                    let right =
                        indices.iter().position(|seen| *seen == index).expect("complete order");
                    if graph.dependency_precedes(id(dependency), id(index)) != (left < right) {
                        return Err(format!("gate order query differs for {dependency}->{index}"));
                    }
                }
            }
            if graph.get(id(masks.len())).is_some()
                || graph.dependency_precedes(id(masks.len()), id(0))
            {
                return Err("unknown gate was treated as declared".into());
            }
            bump(metrics, "acyclic_graphs");
            *metrics.entry("independent_ready_choices".into()).or_default() += multiple;
            *metrics.entry("declared_dependency_edges".into()).or_default() +=
                masks.iter().map(|mask| u64::from(mask.count_ones())).sum::<u64>();
        }
        (actual, expected) => {
            return Err(format!(
                "graph acceptance differs: actual={actual:?}, expected={expected:?}"
            ));
        }
    }
    Ok(())
}

fn invalid(kind: u8, metrics: &mut Metrics) -> Result<(), String> {
    let result = match kind % 7 {
        0 => GateDefinition::new(id(0), plan(1).expect("plan"), vec![id(1), id(1)], Vec::new())
            .map(|_| ()),
        1 => GateDefinition::new(id(0), plan(1).expect("plan"), vec![id(2), id(1)], Vec::new())
            .map(|_| ()),
        2 => {
            let mut values = definitions(&[0, 0, 0]).expect("setup");
            values[0] = GateDefinition::new(id(0), plan(1).expect("plan"), vec![id(3)], Vec::new())
                .expect("setup");
            GateGraph::new(values).map(|_| ())
        }
        3 => {
            let first = definitions(&[0]).expect("setup").remove(0);
            let second = definitions(&[0]).expect("setup").remove(0);
            GateGraph::new(vec![first, second]).map(|_| ())
        }
        4 => {
            let mut values = definitions(&[0, 0, 0]).expect("setup");
            values.reverse();
            GateGraph::new(values).map(|_| ())
        }
        5 => plan(0).map(|_| ()),
        _ => GateGraph::new(Vec::new()).map(|_| ()),
    };
    if result.is_ok() {
        return Err(format!("invalid gate case {kind} was accepted"));
    }
    bump(metrics, "invalid_gate_rejections");
    bump(metrics, &format!("invalid_gate_kind_{}", kind % 7));
    Ok(())
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut metrics = Metrics::new();
    match case {
        Case::Graph { dependencies } => graph(dependencies, &mut metrics)?,
        Case::Invalid { kind } => invalid(*kind, &mut metrics)?,
    }
    Ok(metrics)
}

pub fn calibration() -> Case {
    Case::Graph { dependencies: vec![1 << 2, 0, 0, (1 << 0) | (1 << 1)] }
}

pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        calibration(),
        Case::Graph { dependencies: vec![0] },
        Case::Graph { dependencies: vec![1 << 0] },
        Case::Graph { dependencies: vec![1 << 1, 1 << 0] },
        Case::Graph { dependencies: vec![0, 1 << 0, 1 << 0, (1 << 1) | (1 << 2)] },
    ];
    cases.extend((0..7).map(|kind| Case::Invalid { kind }));
    cases
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let nodes = max_steps.clamp(1, 8);
    let graph = (1_usize..=nodes).prop_flat_map(|n| {
        (
            proptest::collection::vec(any::<u16>(), n),
            proptest::collection::vec(any::<u8>(), n),
            any::<bool>(),
        )
            .prop_map(move |(mut masks, ranks, acyclic)| {
                let valid = (1_u16 << n) - 1;
                for index in 0..n {
                    masks[index] &= valid;
                    if acyclic {
                        for dependency in 0..n {
                            let earlier = (ranks[dependency], dependency) < (ranks[index], index);
                            if !earlier {
                                masks[index] &= !(1 << dependency);
                            }
                        }
                    }
                }
                Case::Graph { dependencies: masks }
            })
    });
    prop_oneof![5 => graph, 1 => (0_u8..7).prop_map(|kind| Case::Invalid { kind })].boxed()
}
