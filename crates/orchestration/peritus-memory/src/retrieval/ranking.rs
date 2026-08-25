//! Bounded integer retrieval ranking with stable identity tie-breaking.

#![allow(clippy::collapsible_if, reason = "the pinned Verus frontend lacks Rust let-chains")]

use super::filter::contradiction_ratio;
use super::output::RankScore;
use super::types::{RetrievalPolicy, RetrievalQuery};
use crate::{BasisPoints, MemoryError, MemoryErrorKind, MemoryField, MemoryRecord};
use vstd::prelude::*;

verus! {

pub(super) fn score(
    record: &MemoryRecord,
    policy: &RetrievalPolicy,
    query: &RetrievalQuery,
) -> Result<RankScore, MemoryError> {
    let scope = bounded(record.scope().specificity())?;
    let relevance = feature_relevance(record, query)?;
    let confidence = record.lifecycle().confidence().basis_points();
    let evidence = evidence_balance(record)?;
    let recency = recency(record, query)?;
    let feedback = record.lifecycle().feedback().rank_component();
    let weights = policy.ranking();
    let mut weighted = 0_u64;
    weighted = weighted_component(weighted, scope, weights.scope())?;
    weighted = weighted_component(weighted, relevance, weights.relevance())?;
    weighted = weighted_component(weighted, confidence, weights.confidence())?;
    weighted = weighted_component(weighted, evidence, weights.evidence())?;
    weighted = weighted_component(weighted, recency, weights.recency())?;
    weighted = weighted_component(weighted, feedback, weights.feedback())?;
    let total = bounded_u64(weighted / 10_000)?;
    Ok(RankScore::from_components(
        scope,
        relevance,
        confidence,
        evidence,
        recency,
        feedback,
        total,
    ))
}

fn feature_relevance(
    record: &MemoryRecord,
    query: &RetrievalQuery,
) -> Result<BasisPoints, MemoryError> {
    let features = query.features().values();
    let features_len = features.len();
    let mut total = 0_u64;
    let mut matched = 0_u64;
    let mut index = 0;
    while index < features_len
        invariant
            index <= features_len,
            features_len == features@.len(),
        decreases features_len - index,
    {
        let query_feature = features[index];
        let weight = u64::from(query_feature.weight().basis_points().get());
        total = total.checked_add(weight).ok_or(MemoryError::field(
            MemoryErrorKind::ArithmeticOverflow,
            MemoryField::Score,
        ))?;
        if let Some(record_feature) = record.features().get(query_feature.key()) {
            if record_feature.digest() == query_feature.digest() {
                matched = matched.checked_add(weight).ok_or(MemoryError::field(
                    MemoryErrorKind::ArithmeticOverflow,
                    MemoryField::Score,
                ))?;
            }
        }
        index += 1;
    }
    if total == 0 {
        return Ok(BasisPoints::ZERO);
    }
    let value = matched.checked_mul(10_000).ok_or(MemoryError::field(
        MemoryErrorKind::ArithmeticOverflow,
        MemoryField::Score,
    ))? / total;
    bounded_u64(value)
}

fn evidence_balance(record: &MemoryRecord) -> Result<BasisPoints, MemoryError> {
    let contradiction = contradiction_ratio(record)?;
    let contradiction_value = contradiction.get();
    if contradiction_value > 10_000 {
        return Err(MemoryError::field(MemoryErrorKind::ArithmeticOverflow, MemoryField::Score));
    }
    bounded(10_000 - contradiction_value)
}

fn recency(
    record: &MemoryRecord,
    query: &RetrievalQuery,
) -> Result<BasisPoints, MemoryError> {
    let observation = record.timing().reviewed().unwrap_or_else(|| record.timing().created());
    if observation.epoch() != query.observation().epoch() {
        return Ok(BasisPoints::ZERO);
    }
    let observation_tick = observation.tick();
    let query_tick = query.observation().tick();
    if observation_tick > query_tick {
        return Err(MemoryError::memory(
            MemoryErrorKind::StaleObservation,
            MemoryField::Observation,
            record.id(),
        ));
    }
    let age = query_tick - observation_tick;
    let penalty = if age > 10_000 {
        10_000
    } else {
        let Ok(value) = u16::try_from(age) else {
            return Err(MemoryError::field(
                MemoryErrorKind::ArithmeticOverflow,
                MemoryField::Score,
            ));
        };
        value
    };
    bounded(10_000 - penalty)
}

fn weighted_component(
    sum: u64,
    component: BasisPoints,
    weight: BasisPoints,
) -> Result<u64, MemoryError> {
    let product = u64::from(component.get()).checked_mul(u64::from(weight.get())).ok_or(
        MemoryError::field(MemoryErrorKind::ArithmeticOverflow, MemoryField::Score),
    )?;
    sum.checked_add(product).ok_or(MemoryError::field(
        MemoryErrorKind::ArithmeticOverflow,
        MemoryField::Score,
    ))
}

const fn bounded(value: u16) -> Result<BasisPoints, MemoryError> {
    BasisPoints::new(value)
}

fn bounded_u64(value: u64) -> Result<BasisPoints, MemoryError> {
    let Ok(converted) = u16::try_from(value) else {
        return Err(MemoryError::field(
            MemoryErrorKind::ArithmeticOverflow,
            MemoryField::Score,
        ));
    };
    bounded(converted)
}

} // verus!
