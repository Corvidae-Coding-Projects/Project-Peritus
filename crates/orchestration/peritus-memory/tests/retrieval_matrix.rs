//! Deterministic retrieval filter, ranking, budget, and explanation matrix.

mod support;

use peritus_memory::{
    BasisPoints, CandidateExplanation, ClaimType, ClaimTypeSet, Confidence, DeletionReason,
    ExclusionReason, FeedbackPolicy, MemoryTombstone, RankingWeights, RequiredFeatures,
    RetrievalFeature, RetrievalFeatures, RetrievalLimits, RetrievalPolicy, RetrievalQuery,
    ScopePolicy, retrieve,
};
use peritus_role::{HarnessRole, RoleProfile};
use peritus_types::Sha256Digest;
use support::{
    RecordOptions, feature_key, make_record, memory_id, observation, project_scope,
    repository_scope, revision,
};

fn all_claims() -> ClaimTypeSet {
    ClaimTypeSet::new(vec![
        ClaimType::Fact,
        ClaimType::Preference,
        ClaimType::Procedure,
        ClaimType::Outcome,
        ClaimType::Warning,
        ClaimType::Constraint,
        ClaimType::Hypothesis,
    ])
    .unwrap()
}

fn weights() -> RankingWeights {
    RankingWeights::new(
        BasisPoints::new(1_000).unwrap(),
        BasisPoints::new(3_000).unwrap(),
        BasisPoints::new(2_000).unwrap(),
        BasisPoints::new(1_500).unwrap(),
        BasisPoints::new(1_500).unwrap(),
        BasisPoints::new(1_000).unwrap(),
    )
    .unwrap()
}

fn policy(
    scope: ScopePolicy,
    max_results: u16,
    minimum_confidence: u16,
    max_review_age: Option<u64>,
    feedback: FeedbackPolicy,
) -> RetrievalPolicy {
    RetrievalPolicy::new(
        RetrievalLimits::new(
            max_results,
            Confidence::new(minimum_confidence).unwrap(),
            max_review_age,
        )
        .unwrap(),
        all_claims(),
        weights(),
        feedback,
        scope,
    )
}

fn query(
    scope: peritus_memory::MemoryScope,
    role: HarnessRole,
    features: RetrievalFeatures,
    required: RequiredFeatures,
    budget: u32,
) -> RetrievalQuery {
    RetrievalQuery::new(
        scope,
        RoleProfile::for_harness_role(role),
        observation(10),
        features,
        required,
        budget,
    )
    .unwrap()
}

fn standard_query() -> RetrievalQuery {
    query(
        project_scope(1),
        HarnessRole::Writer,
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        1_000,
    )
}

fn standard_policy() -> RetrievalPolicy {
    policy(ScopePolicy::Exact, 20, 0, None, FeedbackPolicy::new(None, None))
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
fn selected_candidate_is_scored_bounded_and_mandatorily_quoted() {
    let record = make_record(RecordOptions::new(1));
    let plan = retrieve(&[record], &[], &standard_policy(), &standard_query()).unwrap();
    assert_eq!(plan.selected().len(), 1);
    let candidate = &plan.selected()[0];
    assert!(candidate.quoted_evidence());
    assert_eq!(peritus_memory::MemoryCandidate::quote_open(), b"<peritus-memory-evidence>");
    assert_eq!(peritus_memory::MemoryCandidate::quote_close(), b"</peritus-memory-evidence>");
    assert!(candidate.score().total().get() <= 10_000);
    assert_eq!(plan.used_tokens(), candidate.estimated_tokens());
    assert!(peritus_memory::retrieval_is_bounded(&plan));
}

#[test]
fn role_policy_filters_memory_before_ranking() {
    let record = make_record(RecordOptions::new(2));
    let reviewer = query(
        project_scope(1),
        HarnessRole::Reviewer,
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        100,
    );
    let plan = retrieve(std::slice::from_ref(&record), &[], &standard_policy(), &reviewer).unwrap();
    assert_eq!(reason_for(&plan, record.id()), Some(ExclusionReason::RolePolicy));
}

#[test]
fn exact_and_explicit_broader_scope_policies_differ() {
    let record = make_record(RecordOptions::new(3));
    let scoped_query = query(
        repository_scope(1),
        HarnessRole::Writer,
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        100,
    );
    let exact =
        retrieve(std::slice::from_ref(&record), &[], &standard_policy(), &scoped_query).unwrap();
    assert_eq!(reason_for(&exact, record.id()), Some(ExclusionReason::ScopeMismatch));
    let broader_policy =
        policy(ScopePolicy::IncludeBroader, 20, 0, None, FeedbackPolicy::new(None, None));
    let broader = retrieve(&[record], &[], &broader_policy, &scoped_query).unwrap();
    assert_eq!(broader.selected().len(), 1);
}

#[test]
fn explicit_and_observed_lifecycle_filters_are_typed() {
    let active = make_record(RecordOptions::new(4));
    let quarantined = active
        .quarantine(revision(2), observation(3), peritus_memory::QuarantineReason::ManualReview)
        .unwrap();
    let expired = make_record(RecordOptions::new(5)).expire(revision(2), observation(3)).unwrap();
    let superseded = make_record(RecordOptions::new(6))
        .supersede(revision(2), observation(3), memory_id(7))
        .unwrap();
    let records = vec![quarantined, expired, superseded];
    let plan = retrieve(&records, &[], &standard_policy(), &standard_query()).unwrap();
    assert_eq!(reason_for(&plan, memory_id(4)), Some(ExclusionReason::Quarantined));
    assert_eq!(reason_for(&plan, memory_id(5)), Some(ExclusionReason::Expired));
    assert_eq!(reason_for(&plan, memory_id(6)), Some(ExclusionReason::Superseded));

    let mut expiring = RecordOptions::new(8);
    expiring.expiry_tick = Some(10);
    let expiring = make_record(expiring);
    let plan =
        retrieve(std::slice::from_ref(&expiring), &[], &standard_policy(), &standard_query())
            .unwrap();
    assert_eq!(reason_for(&plan, expiring.id()), Some(ExclusionReason::ExpiryReached));
}

#[test]
fn confidence_claim_support_and_feature_filters_are_typed() {
    let mut low = RecordOptions::new(9);
    low.confidence = 4_999;
    let low = make_record(low);
    let confidence_policy =
        policy(ScopePolicy::Exact, 20, 5_000, None, FeedbackPolicy::new(None, None));
    let plan =
        retrieve(std::slice::from_ref(&low), &[], &confidence_policy, &standard_query()).unwrap();
    assert_eq!(reason_for(&plan, low.id()), Some(ExclusionReason::BelowConfidence));

    let mut unsupported = RecordOptions::new(10);
    unsupported.supporting = Vec::new();
    let unsupported = make_record(unsupported);
    let plan =
        retrieve(std::slice::from_ref(&unsupported), &[], &standard_policy(), &standard_query())
            .unwrap();
    assert_eq!(reason_for(&plan, unsupported.id()), Some(ExclusionReason::UnsupportedEvidence));

    let required = RequiredFeatures::new(vec![feature_key(99)]).unwrap();
    let required_query =
        query(project_scope(1), HarnessRole::Writer, RetrievalFeatures::empty(), required, 100);
    let record = make_record(RecordOptions::new(11));
    let plan =
        retrieve(std::slice::from_ref(&record), &[], &standard_policy(), &required_query).unwrap();
    assert_eq!(reason_for(&plan, record.id()), Some(ExclusionReason::MissingRequiredFeature));
}

#[test]
fn accepted_claim_policy_filters_before_ranking() {
    let mut options = RecordOptions::new(12);
    options.claim_type = ClaimType::Warning;
    let record = make_record(options);
    let fact_only = RetrievalPolicy::new(
        RetrievalLimits::new(20, Confidence::new(0).unwrap(), None).unwrap(),
        ClaimTypeSet::new(vec![ClaimType::Fact]).unwrap(),
        weights(),
        FeedbackPolicy::new(None, None),
        ScopePolicy::Exact,
    );
    let plan = retrieve(std::slice::from_ref(&record), &[], &fact_only, &standard_query()).unwrap();
    assert_eq!(reason_for(&plan, record.id()), Some(ExclusionReason::UnsupportedClaim));
}

#[test]
fn stale_review_is_excluded_under_explicit_policy() {
    let record = make_record(RecordOptions::new(13));
    let fresh_policy = policy(ScopePolicy::Exact, 20, 0, Some(3), FeedbackPolicy::new(None, None));
    let plan =
        retrieve(std::slice::from_ref(&record), &[], &fresh_policy, &standard_query()).unwrap();
    assert_eq!(reason_for(&plan, record.id()), Some(ExclusionReason::StaleReview));
}

#[test]
fn negative_feedback_and_contradiction_trigger_explicit_policy_quarantine() {
    let mut negative = RecordOptions::new(14);
    negative.positive_feedback = 1;
    negative.negative_feedback = 1;
    let negative = make_record(negative);
    let feedback_policy = policy(
        ScopePolicy::Exact,
        20,
        0,
        None,
        FeedbackPolicy::new(Some(BasisPoints::new(5_000).unwrap()), None),
    );
    let plan = retrieve(std::slice::from_ref(&negative), &[], &feedback_policy, &standard_query())
        .unwrap();
    assert_eq!(reason_for(&plan, negative.id()), Some(ExclusionReason::NegativeFeedback));

    let mut contradiction = RecordOptions::new(15);
    contradiction.contradicting = vec![56];
    let contradiction = make_record(contradiction);
    let contradiction_policy = policy(
        ScopePolicy::Exact,
        20,
        0,
        None,
        FeedbackPolicy::new(None, Some(BasisPoints::new(5_000).unwrap())),
    );
    let plan = retrieve(
        std::slice::from_ref(&contradiction),
        &[],
        &contradiction_policy,
        &standard_query(),
    )
    .unwrap();
    assert_eq!(reason_for(&plan, contradiction.id()), Some(ExclusionReason::Contradiction));
}

#[test]
fn tombstone_dominance_precedes_ranking_and_checks_digest_binding() {
    let record = make_record(RecordOptions::new(16));
    let tombstone =
        record.forget(revision(2), observation(3), DeletionReason::UserRequest).unwrap();
    let plan = retrieve(
        std::slice::from_ref(&record),
        &[tombstone],
        &standard_policy(),
        &standard_query(),
    )
    .unwrap();
    assert_eq!(reason_for(&plan, record.id()), Some(ExclusionReason::Tombstoned));

    let bad = MemoryTombstone::new(
        record.id(),
        revision(1),
        observation(3),
        DeletionReason::InvalidContent,
        Sha256Digest::new([99; 32]),
    );
    assert_eq!(
        retrieve(&[record], &[bad], &standard_policy(), &standard_query()).unwrap_err().kind(),
        peritus_memory::MemoryErrorKind::TombstoneDigestMismatch
    );
}

#[test]
fn budget_and_result_limits_explain_every_unselected_candidate() {
    let mut first_options = RecordOptions::new(17);
    first_options.tokens = 7;
    first_options.features = Vec::new();
    let first = make_record(first_options);
    let mut second_options = RecordOptions::new(18);
    second_options.tokens = 7;
    second_options.features = Vec::new();
    let second = make_record(second_options);
    let tight_query = query(
        project_scope(1),
        HarnessRole::Writer,
        RetrievalFeatures::empty(),
        RequiredFeatures::empty(),
        10,
    );
    let plan =
        retrieve(&[second.clone(), first.clone()], &[], &standard_policy(), &tight_query).unwrap();
    assert_eq!(plan.selected()[0].id(), first.id());
    assert_eq!(reason_for(&plan, second.id()), Some(ExclusionReason::TokenBudget));
    assert_eq!(plan.explanations().len(), 2);

    let one_result = policy(ScopePolicy::Exact, 1, 0, None, FeedbackPolicy::new(None, None));
    let plan = retrieve(&[first, second.clone()], &[], &one_result, &standard_query()).unwrap();
    assert_eq!(reason_for(&plan, second.id()), Some(ExclusionReason::ResultLimit));
}

#[test]
fn ranking_is_feature_sensitive_and_stable_id_breaks_ties() {
    let mut matching_options = RecordOptions::new(19);
    matching_options.features = vec![(1, 2, 10_000)];
    let matching = make_record(matching_options);
    let mut other_options = RecordOptions::new(20);
    other_options.features = vec![(1, 3, 10_000)];
    let other = make_record(other_options);
    let feature = RetrievalFeature::new(
        feature_key(1),
        Sha256Digest::new([2; 32]),
        peritus_memory::FeatureWeight::new(10_000).unwrap(),
    );
    let feature_query = query(
        project_scope(1),
        HarnessRole::Writer,
        RetrievalFeatures::new(vec![feature]).unwrap(),
        RequiredFeatures::empty(),
        100,
    );
    let plan = retrieve(&[other, matching], &[], &standard_policy(), &feature_query).unwrap();
    assert_eq!(plan.selected()[0].id(), memory_id(19));
    assert!(plan.selected()[0].score().relevance() > plan.selected()[1].score().relevance());

    let mut tie_one = RecordOptions::new(21);
    tie_one.features = Vec::new();
    let tie_one = make_record(tie_one);
    let mut tie_two = RecordOptions::new(22);
    tie_two.features = Vec::new();
    let tie_two = make_record(tie_two);
    let tied = retrieve(&[tie_two, tie_one], &[], &standard_policy(), &standard_query()).unwrap();
    assert_eq!(tied.selected()[0].id(), memory_id(21));
    assert_eq!(tied.selected()[1].id(), memory_id(22));
}
