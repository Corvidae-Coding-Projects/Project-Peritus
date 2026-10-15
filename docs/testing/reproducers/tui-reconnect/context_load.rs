//! Real public-API context consumer. No daemon, provider, mocks, or test harness.
//! Build with rustc --edition=2024 -O -C panic=abort -L dependency=target/release/deps and
//! --extern peritus_context=... --extern peritus_role=... --extern peritus_codec=...
//! --extern peritus_types=... .
//! --shape none --nodes 512 --repeats 1 generates bounded selection load.
//! --shape scenarios exercises independent closure/order/budget expectations.
//! --snapshot PATH --run RUN_HEX --workspace WORKSPACE_HEX renders a retained
//! Writer working-state snapshot without printing its private contents.

use peritus_codec::sha256;
use peritus_context::{
    AuthorityClass, ContentKind, ContextErrorKind, ContextGraph, ContextLimits, ContextNode,
    ContextNodeId, ContextNodeMetadata, ContextPlan, ContextPlanId, OmissionReason, Provenance,
    RequirementMode, RoleVisibility, SelectionPolicy, SelectionReason, TokenBudget, TrustClass,
    bind_context_content, select_context,
};
use peritus_role::{ContextClass, HarnessRole, RoleProfile};
use peritus_types::{RunId, WorkspaceId};
use std::{env, time::Instant};

fn bytes16(text: &str) -> [u8; 16] {
    assert_eq!(text.len(), 32);
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap();
    }
    bytes
}

fn snapshot(path: &str, run: &str, workspace: &str, preferred: u64, available: u64) {
    use peritus_context::working::{
        WorkingBinding, WorkingEntryStatus, WorkingLimits, decode_working_state,
        render_working_state_with_headroom,
    };
    let bytes = std::fs::read(path).unwrap();
    let run = bytes16(run);
    let binding = WorkingBinding::new(
        RunId::new(run).unwrap(),
        WorkspaceId::new(bytes16(workspace)).unwrap(),
        ContextNodeId::new(run).unwrap(),
        HarnessRole::Writer,
        0,
    );
    let begin = Instant::now();
    let state = decode_working_state(&bytes, binding, WorkingLimits::standard()).unwrap();
    let decode_ms = begin.elapsed().as_secs_f64() * 1000.0;
    let entries = state.entries(state.binding()).unwrap();
    let stale = entries.iter().filter(|entry| entry.status() == WorkingEntryStatus::Stale).count();
    let edges: usize = entries.iter().map(|entry| entry.links().depends_on().len()).sum();
    let begin = Instant::now();
    let view =
        render_working_state_with_headroom(&state, state.binding(), preferred, available).unwrap();
    let render_ms = begin.elapsed().as_secs_f64() * 1000.0;
    // Hash the complete typed result, including exact source content, ordered IDs,
    // metadata, accounting and omissions. Private strings never reach stdout.
    let exact = format!("{:?}|{:?}", view.plan(), view.omitted());
    let digest = sha256(exact.as_bytes())
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let selected = view.plan().map_or(0, |plan| plan.segments().len());
    let tokens = view.plan().map_or(0, |plan| plan.accounting().used_input());
    println!(
        "{{\"shape\":\"brownfield-snapshot\",\"entries\":{},\"stale\":{stale},\"edges\":{edges},\"state_revision\":{},\"preferred\":{preferred},\"available\":{available},\"selected\":{selected},\"omitted\":{},\"tokens\":{tokens},\"decode_ms\":{decode_ms:.3},\"render_ms\":{render_ms:.3},\"render_digest\":\"{digest}\"}}",
        entries.len(),
        state.revision(),
        view.omitted().len()
    );
}

fn id(number: usize) -> ContextNodeId {
    ContextNodeId::new((number as u128).to_be_bytes()).unwrap()
}

fn fixture(shape: &str, count: usize) -> ContextGraph {
    let limits = ContextLimits::new(count, 128, 8, 2).unwrap();
    let nodes = (1..=count)
        .map(|number| {
            let (requirement, dependencies, hidden) = match shape {
                "chain" | "chain-forward" | "chain-optional" => (
                    if shape == "chain-optional" {
                        RequirementMode::Optional
                    } else if number == (if shape == "chain-forward" { 1 } else { count }) {
                        RequirementMode::Required
                    } else {
                        RequirementMode::DependencyRequired
                    },
                    if shape == "chain-forward" {
                        if number < count { vec![id(number + 1)] } else { vec![] }
                    } else if number > 1 {
                        vec![id(number - 1)]
                    } else {
                        vec![]
                    },
                    false,
                ),
                "diamond" => {
                    let base = ((number - 1) / 4) * 4;
                    let within = number - base;
                    let dependencies = match within {
                        2 | 3 => vec![id(base + 1)],
                        4 => vec![id(base + 2), id(base + 3)],
                        _ => vec![],
                    };
                    (
                        if within == 4 {
                            RequirementMode::Optional
                        } else {
                            RequirementMode::DependencyRequired
                        },
                        dependencies,
                        false,
                    )
                }
                "hidden" | "hidden-required" => (
                    if number == 3 {
                        if shape == "hidden-required" {
                            RequirementMode::Required
                        } else {
                            RequirementMode::Optional
                        }
                    } else {
                        RequirementMode::DependencyRequired
                    },
                    if number == 3 { vec![id(1), id(2)] } else { vec![] },
                    number == 1,
                ),
                _ => (RequirementMode::Optional, vec![], false),
            };
            let visibility = RoleVisibility::new(
                vec![if hidden {
                    HarnessRole::Reviewer.actor_role()
                } else {
                    HarnessRole::Writer.actor_role()
                }],
                limits,
            )
            .unwrap();
            let bytes = format!("source-{number}").into_bytes();
            let metadata = ContextNodeMetadata::new(
                id(number),
                Provenance::Memory,
                AuthorityClass::NonAuthoritative,
                TrustClass::Untrusted,
                ContextClass::MemoryEvidence,
                ContentKind::MemoryEvidence,
                1,
                1,
                requirement,
                0,
                visibility,
                dependencies,
                limits,
            )
            .unwrap();
            ContextNode::new(
                metadata,
                bind_context_content(bytes.clone(), sha256(&bytes), limits).unwrap(),
            )
        })
        .collect();
    ContextGraph::new(nodes, limits).unwrap()
}

fn check_plan(plan: &ContextPlan, selected: &[usize], reasons: &[SelectionReason], budget: u64) {
    let actual: Vec<_> = plan.selected().iter().map(|entry| entry.node_id()).collect();
    let expected: Vec<_> = selected.iter().map(|number| id(*number)).collect();
    assert_eq!(actual, expected, "canonical render order or selected closure changed");
    assert_eq!(plan.selected().iter().map(|entry| entry.reason()).collect::<Vec<_>>(), reasons);
    assert_eq!(plan.accounting().used_input(), selected.len() as u64);
    assert_eq!(plan.accounting().remaining_input(), budget - selected.len() as u64);
    assert_eq!(
        plan.selected_bytes(),
        selected.iter().map(|number| format!("source-{number}").len()).sum::<usize>()
    );
}

fn run(shape: &str, count: usize, repeats: usize, budget: u64) {
    assert!(count > 0 && repeats > 0);
    assert!(shape != "diamond" || count % 4 == 0);
    let begin = Instant::now();
    let graph = fixture(shape, count);
    let construction = begin.elapsed();
    let policy = SelectionPolicy::new(
        RoleProfile::for_harness_role(HarnessRole::Writer),
        TokenBudget::new(budget, 0, 0).unwrap(),
        count,
        count * 128,
    )
    .unwrap();
    let begin = Instant::now();
    let mut outcome = String::new();
    for _ in 0..repeats {
        let result =
            select_context(&graph, &policy, ContextPlanId::new(sha256(b"context-consumer-v1")));
        if shape == "hidden-required" {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), ContextErrorKind::HiddenRequiredDependency);
            assert_eq!(error.node_id(), Some(id(3)));
            assert_eq!(error.related_id(), Some(id(1)));
            outcome = "hidden-required-rejected".into();
            continue;
        }
        if matches!(shape, "chain" | "chain-forward") && budget < count as u64 {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), ContextErrorKind::RequiredTokenBudgetExceeded);
            assert_eq!(error.node_id(), Some(id(if shape == "chain-forward" { 1 } else { count })));
            assert_eq!(error.expected(), Some(budget));
            assert_eq!(error.actual(), Some(count as u64));
            outcome = "required-budget-rejected".into();
            continue;
        }
        let plan = result.unwrap();
        let selected_count = if shape == "hidden" {
            0
        } else if shape == "diamond" {
            count.min((budget as usize / 4) * 4)
        } else {
            count.min(budget as usize)
        };
        let selected: Vec<_> = (1..=selected_count).collect();
        let reasons: Vec<_> = selected
            .iter()
            .map(|number| match shape {
                "chain" if *number == count => SelectionReason::RequiredRoot,
                "chain-forward" if *number == 1 => SelectionReason::RequiredRoot,
                "chain" | "chain-forward" => SelectionReason::RequiredDependency,
                "diamond" if number % 4 != 0 => SelectionReason::OptionalDependency,
                _ => SelectionReason::OptionalRoot,
            })
            .collect();
        check_plan(&plan, &selected, &reasons, budget);
        if shape == "hidden" {
            assert_eq!(plan.omitted().len(), 1);
            let omitted = plan.omitted()[0];
            assert_eq!(omitted.node_id(), id(3));
            assert_eq!(omitted.reason(), OmissionReason::HiddenDependency);
            assert_eq!(omitted.blocking_dependency(), Some(id(1)));
        } else {
            let omitted: Vec<_> = if shape == "diamond" {
                ((selected_count + 4)..=count).step_by(4).collect()
            } else {
                ((selected_count + 1)..=count).collect()
            };
            assert_eq!(
                plan.omitted().iter().map(|entry| entry.node_id()).collect::<Vec<_>>(),
                omitted.iter().map(|number| id(*number)).collect::<Vec<_>>()
            );
            assert!(
                plan.omitted().iter().all(|entry| entry.reason() == OmissionReason::TokenBudget)
            );
        }
        outcome = format!("selected-{selected_count}");
    }
    println!(
        "{{\"shape\":\"{shape}\",\"nodes\":{count},\"repeats\":{repeats},\"budget\":{budget},\"graph_ms\":{:.3},\"selection_ms\":{:.3},\"outcome\":\"{outcome}\",\"passed\":true}}",
        construction.as_secs_f64() * 1000.0,
        begin.elapsed().as_secs_f64() * 1000.0
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let arg =
        |name: &str| args.windows(2).find(|pair| pair[0] == name).map(|pair| pair[1].as_str());
    if let Some(path) = arg("--snapshot") {
        snapshot(
            path,
            arg("--run").expect("--run required"),
            arg("--workspace").expect("--workspace required"),
            arg("--preferred").unwrap_or("4096").parse().unwrap(),
            arg("--available").unwrap_or("200000").parse().unwrap(),
        );
        return;
    }
    let shape = arg("--shape").unwrap_or("scenarios");
    assert!(
        matches!(
            shape,
            "scenarios"
                | "none"
                | "chain"
                | "chain-forward"
                | "chain-optional"
                | "diamond"
                | "hidden"
                | "hidden-required"
        ),
        "unknown shape"
    );
    let count = arg("--nodes").unwrap_or("512").parse().unwrap();
    let repeats = arg("--repeats").unwrap_or("1").parse().unwrap();
    let budget = arg("--budget").map(|value| value.parse().unwrap()).unwrap_or(count as u64);
    if shape == "scenarios" {
        for n in [4, 8, 12] {
            for budget in [1, n as u64 / 2, n as u64, n as u64 + 1] {
                run("none", n, repeats, budget);
                run("chain", n, repeats, budget);
                run("chain-forward", n, repeats, budget);
                // Prefix roots have fully contained backward dependencies, so
                // the same exact budget keeps a canonical prefix selected.
                run("chain-optional", n, repeats, budget);
                run("diamond", n, repeats, budget);
            }
        }
        run("hidden", 3, repeats, 8);
        run("hidden-required", 3, repeats, 8);
    } else {
        run(shape, count, repeats, budget);
    }
}
