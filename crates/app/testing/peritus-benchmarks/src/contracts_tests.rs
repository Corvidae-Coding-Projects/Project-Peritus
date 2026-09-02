//! Focused tests for subject and runner integration contracts.

use crate::{RunContext, StableId};

#[test]
fn run_context_carries_an_explicit_workload_binding() {
    let context = RunContext::for_workload(id("run"), id("profile"), id("plan"), id("workload"));
    assert_eq!(context.plan_id().as_str(), "plan");
    assert_eq!(context.workload_id().as_str(), "workload");
}

#[test]
fn legacy_run_context_uses_plan_as_workload() {
    let context = RunContext::new(id("run"), id("profile"), id("plan"));
    assert_eq!(context.workload_id(), context.plan_id());
}

fn id(value: &str) -> StableId {
    StableId::new(value).expect("stable id")
}
