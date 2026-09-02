//! Deterministic bounded measurement sampling and campaign-wide resequencing.

use std::collections::BTreeMap;

use peritus_benchmarks::{
    MeasurementIngestor, MeasurementRecord, MeasurementSet, MeasurementSink, Metric,
    QualificationError, QualificationProfile, StableId,
};

const DIAGNOSTIC_SAMPLES_PER_METRIC: usize = 64;
const OBJECTIVE_SAMPLE_MULTIPLIER: usize = 2;

#[derive(Clone)]
pub struct Sample {
    workload_id: StableId,
    metric: Metric,
    source_sequence: u64,
    elapsed_micros: u64,
    value: u64,
}

struct Bucket {
    target: usize,
    seen: u64,
    samples: Vec<Sample>,
}

/// Per-workload reservoir that bounds long soaks without biasing toward their opening minutes.
pub struct SamplingSink {
    workload_id: StableId,
    elapsed_offset_micros: u64,
    targets: BTreeMap<Metric, usize>,
    buckets: BTreeMap<Metric, Bucket>,
}

impl SamplingSink {
    pub fn new(
        profile: &QualificationProfile,
        workload_id: StableId,
        elapsed_offset_micros: u64,
    ) -> Self {
        let mut targets = BTreeMap::<Metric, usize>::new();
        for objective in
            profile.objectives().iter().filter(|objective| objective.workload_id() == &workload_id)
        {
            let target = objective
                .minimum_samples()
                .saturating_mul(OBJECTIVE_SAMPLE_MULTIPLIER)
                .max(DIAGNOSTIC_SAMPLES_PER_METRIC);
            targets
                .entry(objective.metric())
                .and_modify(|current| *current = (*current).max(target))
                .or_insert(target);
        }
        Self { workload_id, elapsed_offset_micros, targets, buckets: BTreeMap::new() }
    }

    pub fn finish(self) -> Vec<Sample> {
        self.buckets.into_values().flat_map(|bucket| bucket.samples).collect()
    }
}

impl MeasurementSink for SamplingSink {
    fn record(&mut self, measurement: MeasurementRecord) -> Result<(), QualificationError> {
        if measurement.workload_id() != &self.workload_id {
            return Err(QualificationError::MeasurementBinding {
                field: "sampling.workload_id",
                expected: self.workload_id.to_string(),
                observed: measurement.workload_id().to_string(),
            });
        }
        let metric = measurement.metric();
        let target = self.targets.get(&metric).copied().unwrap_or(DIAGNOSTIC_SAMPLES_PER_METRIC);
        let sample = Sample {
            workload_id: self.workload_id.clone(),
            metric,
            source_sequence: measurement.sequence(),
            elapsed_micros: self
                .elapsed_offset_micros
                .checked_add(measurement.elapsed_micros())
                .ok_or(QualificationError::ArithmeticOverflow("sample elapsed time"))?,
            value: measurement.value(),
        };
        let bucket = self.buckets.entry(metric).or_insert_with(|| Bucket {
            target,
            seen: 0,
            samples: Vec::with_capacity(target),
        });
        bucket.seen = bucket
            .seen
            .checked_add(1)
            .ok_or(QualificationError::ArithmeticOverflow("sample observation count"))?;
        if bucket.samples.len() < bucket.target {
            bucket.samples.push(sample);
        } else {
            let slot = mix64(bucket.seen ^ measurement.sequence()) % bucket.seen;
            if slot < bucket.target as u64 {
                bucket.samples[usize::try_from(slot).expect("slot is below usize target")] = sample;
            }
        }
        Ok(())
    }
}

pub fn merge_samples(
    run_id: &StableId,
    profile_id: &StableId,
    workload_ids: impl IntoIterator<Item = StableId>,
    max_records: usize,
    samples: impl IntoIterator<Item = Sample>,
) -> Result<MeasurementSet, QualificationError> {
    let mut samples = samples.into_iter().collect::<Vec<_>>();
    samples.sort_by(|left, right| {
        (left.elapsed_micros, &left.workload_id, left.metric, left.source_sequence).cmp(&(
            right.elapsed_micros,
            &right.workload_id,
            right.metric,
            right.source_sequence,
        ))
    });
    let mut ingestor =
        MeasurementIngestor::new(run_id.clone(), profile_id.clone(), workload_ids, max_records)?;
    for (sequence, sample) in samples.into_iter().enumerate() {
        ingestor.record(MeasurementRecord::new(
            run_id.clone(),
            profile_id.clone(),
            sample.workload_id,
            sample.metric,
            u64::try_from(sequence)
                .map_err(|_| QualificationError::ArithmeticOverflow("merged sample sequence"))?,
            sample.elapsed_micros,
            sample.value,
        )?)?;
    }
    Ok(ingestor.finish())
}

const fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use peritus_benchmarks::{
        CapacityLimits, ConcurrencyLimits, MeasurementRecord, MeasurementSink, ObjectiveBound,
        QualificationProfileBuilder, QueueLimits, ReferenceMachine, RegressionPolicy,
        ResourceEnvelope, SloObjective, Statistic,
    };

    use super::*;

    #[test]
    fn reservoir_is_bounded_and_merged_sequences_are_contiguous() {
        let profile = profile();
        let workload = id("workload");
        let mut sink = SamplingSink::new(&profile, workload.clone(), 10);
        for sequence in 0..1_000 {
            sink.record(
                MeasurementRecord::new(
                    id("run"),
                    id("profile"),
                    workload.clone(),
                    Metric::EventAppendLatency,
                    sequence,
                    sequence,
                    sequence + 1,
                )
                .expect("record"),
            )
            .expect("sample");
        }
        let merged = merge_samples(&id("run"), &id("profile"), [workload], 100, sink.finish())
            .expect("merge");
        assert_eq!(merged.records().len(), 64);
        assert!(merged.records().iter().enumerate().all(|(sequence, record)| {
            record.sequence() == sequence as u64 && record.elapsed_micros() >= 10
        }));
    }

    fn profile() -> QualificationProfile {
        let envelope = ResourceEnvelope::new(
            ConcurrencyLimits::new(1, 1, 1).expect("concurrency"),
            CapacityLimits::new(1, 1, 1).expect("capacity"),
            QueueLimits::new(1, 1, 1, 1).expect("queues"),
        );
        QualificationProfileBuilder::new(
            id("profile"),
            "sampling",
            ReferenceMachine::new(id("linux"), id("x86_64"), "cpu", 1, 1, id("storage"))
                .expect("machine"),
            envelope,
            RegressionPolicy::new(1, 2, 1, false).expect("policy"),
        )
        .objective(
            SloObjective::new(
                id("objective"),
                id("workload"),
                Metric::EventAppendLatency,
                Statistic::P99,
                ObjectiveBound::AtMost,
                100,
                16,
            )
            .expect("objective"),
        )
        .build()
        .expect("profile")
    }

    fn id(value: &str) -> StableId {
        StableId::new(value).expect("stable id")
    }
}
