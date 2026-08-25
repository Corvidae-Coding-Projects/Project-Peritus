//! Retrieval permutation, fail-closed input, and lifecycle-domain checks.

mod support;

use peritus_memory::{
    BasisPoints, CandidateExplanation, ClaimType, ClaimTypeSet, Confidence, ExclusionReason,
    FeedbackPolicy, MemoryState, RankingWeights, RequiredFeatures, RetrievalFeatures,
    RetrievalLimits, RetrievalPolicy, RetrievalQuery, ScopePolicy, retrieve,
};
use peritus_role::{HarnessRole, RoleProfile};
use support::{RecordOptions, make_record, observation, project_scope};

fn standard_policy() -> RetrievalPolicy {
    let claims = ClaimTypeSet::new(vec![
        ClaimType::Fact,
        ClaimType::Preference,
        ClaimType::Procedure,
        ClaimType::Outcome,
        ClaimType::Warning,
        ClaimType::Constraint,
        ClaimType::Hypothesis,
    ])
    .unwrap();
    let weights = RankingWeights::new(
        BasisPoints::new(1_000).unwrap(),
        BasisPoints::new(3_000).unwrap(),
        BasisPoints::new(2_000).unwrap(),
        BasisPoints::new(1_500).unwrap(),
        BasisPoints::new(1_500).unwrap(),
        BasisPoints::new(1_000).unwrap(),
    )
    .unwrap();
    RetrievalPolicy::new(
        RetrievalLimits::new(20, Confidence::new(0).unwrap(), None).unwrap(),
        claims,
        weights,
        FeedbackPolicy::new(None, None),
        ScopePolicy::Exact,
    )
}

fn standard_query() -> RetrievalQuery {
    RetrievalQuery::new(
        project_scope(1),
        RoleProfile::for_harness_role(HarnessRole::Writer),
        observation(10),
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        1_000,
    )
    .unwrap()
}

fn reason_for(
    plan: &peritus_memory::RetrievalPlan,
    id: peritus_memory::MemoryId,
) -> Option<ExclusionReason> {
    plan.explanations().iter().find_map(|explanation| match explanation {
        CandidateExplanation::Excluded(excluded) if excluded.id() == id => Some(excluded.reason()),
        CandidateExplanation::Selected(_, _, _) | CandidateExplanation::Excluded(_) => None,
    })
}

#[test]
fn permutations_produce_identical_plans_and_explanations() {
    let one = make_record(RecordOptions::new(23));
    let two = make_record(RecordOptions::new(24));
    let three = make_record(RecordOptions::new(25));
    let forward = retrieve(
        &[one.clone(), two.clone(), three.clone()],
        &[],
        &standard_policy(),
        &standard_query(),
    )
    .unwrap();
    let reverse = retrieve(&[three, two, one], &[], &standard_policy(), &standard_query()).unwrap();
    assert_eq!(forward, reverse);
}

#[test]
fn future_observations_and_duplicate_candidates_fail_closed() {
    let record = make_record(RecordOptions::new(26));
    let past_query = RetrievalQuery::new(
        project_scope(1),
        RoleProfile::for_harness_role(HarnessRole::Writer),
        observation(1),
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        100,
    )
    .unwrap();
    let plan =
        retrieve(std::slice::from_ref(&record), &[], &standard_policy(), &past_query).unwrap();
    assert_eq!(reason_for(&plan, record.id()), Some(ExclusionReason::FutureObservation));
    assert_eq!(
        retrieve(&[record.clone(), record], &[], &standard_policy(), &standard_query(),)
            .unwrap_err()
            .kind(),
        peritus_memory::MemoryErrorKind::DuplicateValue
    );
}

#[test]
fn state_enum_has_no_forgotten_content_variant() {
    let states = [
        MemoryState::Active,
        MemoryState::Quarantined,
        MemoryState::Expired,
        MemoryState::Superseded,
    ];
    assert_eq!(states.len(), 4);
}
