//! Integration tests for fail-closed qualification verdicts.

use peritus_benchmarks::{
    DatasetLimits, MeasurementIngestor, QualificationDataset, QualificationEvaluator,
    QualificationVerdict, ResourceAccountant, StableId,
};

const PROFILE: &str =
    include_str!("../../../../../benchmarks/profiles/qualification-candidate-v1.json");
const WORKLOADS: &str = include_str!("../../../../../benchmarks/workloads/production-v1.json");

#[test]
fn missing_execution_and_baseline_can_never_be_ready() {
    let dataset =
        QualificationDataset::from_json(PROFILE, WORKLOADS, DatasetLimits::production_defaults())
            .expect("stable dataset");
    let sink = MeasurementIngestor::new(
        StableId::new("empty-run").expect("id"),
        dataset.profile().id().clone(),
        dataset.workloads().iter().map(|workload| workload.id().clone()),
        dataset.profile().max_measurements(),
    )
    .expect("sink");
    let evaluation = QualificationEvaluator::evaluate(
        dataset.profile(),
        dataset.workloads(),
        &sink.finish(),
        ResourceAccountant::new(dataset.profile().envelope()).summary(),
        &[],
        None,
    )
    .expect("evaluation");
    assert_eq!(evaluation.verdict(), QualificationVerdict::NotReady);
    assert!(!evaluation.not_ready_reasons().is_empty());
}
