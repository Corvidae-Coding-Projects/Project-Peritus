//! Task-cluster paired comparison, bootstrap interval, and task-level sign diagnostic.

use std::collections::BTreeMap;

use crate::{
    EvaluationError, EvaluationErrorKind, EvaluationOperation, ProbabilityMillionths,
    ProfileDigest, TaskId,
};

const EFFECT_SCALE: i64 = 1_000_000;
const WEIGHT_SCALE: u128 = 1_000_000_000_000_000_000;

/// One exact evaluated baseline/candidate task/ordinal pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PairedCell {
    task_id: TaskId,
    ordinal: u16,
    baseline_passed: bool,
    candidate_passed: bool,
}

impl PairedCell {
    /// Creates a nonzero-ordinal evaluated pair.
    ///
    /// # Errors
    /// Rejects ordinal zero.
    pub const fn new(
        task_id: TaskId,
        ordinal: u16,
        baseline_passed: bool,
        candidate_passed: bool,
    ) -> Result<Self, EvaluationError> {
        if ordinal == 0 {
            return Err(crate::invalid(
                EvaluationErrorKind::Statistics,
                EvaluationOperation::Analyze,
                "paired rollout ordinal is zero",
            ));
        }
        Ok(Self { task_id, ordinal, baseline_passed, candidate_passed })
    }
    /// Task identity.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }
    /// Paired ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u16 {
        self.ordinal
    }
    /// Baseline verdict.
    #[must_use]
    pub const fn baseline_passed(self) -> bool {
        self.baseline_passed
    }
    /// Candidate verdict.
    #[must_use]
    pub const fn candidate_passed(self) -> bool {
        self.candidate_passed
    }
}

/// Complete raw paired transition table.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PairedTable {
    /// Both arms passed.
    pub both_passed: u32,
    /// Candidate fixed a baseline failure.
    pub candidate_only: u32,
    /// Candidate regressed a baseline pass.
    pub baseline_only: u32,
    /// Both arms failed.
    pub both_failed: u32,
}

impl PairedTable {
    /// Returns complete valid pair count.
    #[must_use]
    pub fn total(self) -> Option<u32> {
        self.both_passed
            .checked_add(self.candidate_only)?
            .checked_add(self.baseline_only)?
            .checked_add(self.both_failed)
    }
}

/// Deterministic primary task-cluster bootstrap interval for paired effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootstrapInterval {
    lower_millionths: i32,
    upper_millionths: i32,
    replicates: u32,
    confidence_millionths: u32,
}

impl BootstrapInterval {
    /// Lower paired-effect bound.
    #[must_use]
    pub const fn lower_millionths(self) -> i32 {
        self.lower_millionths
    }
    /// Upper paired-effect bound.
    #[must_use]
    pub const fn upper_millionths(self) -> i32 {
        self.upper_millionths
    }
    /// Frozen resample count.
    #[must_use]
    pub const fn replicates(self) -> u32 {
        self.replicates
    }
    /// Frozen confidence level.
    #[must_use]
    pub const fn confidence_millionths(self) -> u32 {
        self.confidence_millionths
    }
}

/// Task-level two-sided sign-test diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignTest {
    positive_tasks: u32,
    negative_tasks: u32,
    tied_tasks: u32,
    two_sided_p: ProbabilityMillionths,
}

impl SignTest {
    /// Tasks with positive candidate effect.
    #[must_use]
    pub const fn positive_tasks(self) -> u32 {
        self.positive_tasks
    }
    /// Tasks with negative candidate effect.
    #[must_use]
    pub const fn negative_tasks(self) -> u32 {
        self.negative_tasks
    }
    /// Tasks with tied effect.
    #[must_use]
    pub const fn tied_tasks(self) -> u32 {
        self.tied_tasks
    }
    /// Deterministic fixed-point two-sided p value.
    #[must_use]
    pub const fn two_sided_p(self) -> ProbabilityMillionths {
        self.two_sided_p
    }
}

/// Complete paired evidence; it is not a promotion decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PairedComparison {
    table: PairedTable,
    net_effect_millionths: i32,
    interval: BootstrapInterval,
    sign_test: SignTest,
}

impl PairedComparison {
    /// Raw transition table.
    #[must_use]
    pub const fn table(self) -> PairedTable {
        self.table
    }
    /// Candidate minus baseline effect across valid rollout pairs.
    #[must_use]
    pub const fn net_effect_millionths(self) -> i32 {
        self.net_effect_millionths
    }
    /// Primary task-cluster bootstrap interval.
    #[must_use]
    pub const fn interval(self) -> BootstrapInterval {
        self.interval
    }
    /// Task-level sign-test diagnostic.
    #[must_use]
    pub const fn sign_test(self) -> SignTest {
        self.sign_test
    }
}

/// Computes complete paired evidence from canonical task/ordinal cells.
///
/// # Errors
/// Rejects empty, duplicate/noncanonical pairs, invalid replicate/confidence values, or arithmetic
/// overflow.
pub fn compare_paired(
    profile: ProfileDigest,
    cells: &[PairedCell],
    replicates: u32,
    confidence_millionths: u32,
) -> Result<PairedComparison, EvaluationError> {
    if cells.is_empty() || replicates == 0 || confidence_millionths != 950_000 {
        return Err(invalid("paired comparison inputs are empty or unsupported"));
    }
    if cells.windows(2).any(|pair| {
        (pair[0].task_id(), pair[0].ordinal()) >= (pair[1].task_id(), pair[1].ordinal())
    }) {
        return Err(invalid("paired cells are duplicated or noncanonical"));
    }
    let mut table = PairedTable::default();
    let mut tasks: BTreeMap<TaskId, Vec<i8>> = BTreeMap::new();
    for cell in cells {
        let effect = match (cell.baseline_passed(), cell.candidate_passed()) {
            (true, true) => {
                table.both_passed += 1;
                0
            }
            (false, true) => {
                table.candidate_only += 1;
                1
            }
            (true, false) => {
                table.baseline_only += 1;
                -1
            }
            (false, false) => {
                table.both_failed += 1;
                0
            }
        };
        tasks.entry(cell.task_id()).or_default().push(effect);
    }
    let total = i64::from(table.total().ok_or_else(arithmetic)?);
    let net = (i64::from(table.candidate_only) - i64::from(table.baseline_only))
        .checked_mul(EFFECT_SCALE)
        .ok_or_else(arithmetic)?
        / total;
    let task_effects: Vec<i64> = tasks
        .values()
        .map(|values| {
            values.iter().map(|value| i64::from(*value)).sum::<i64>() * EFFECT_SCALE
                / i64::try_from(values.len()).unwrap_or(i64::MAX)
        })
        .collect();
    let interval = bootstrap(profile, &task_effects, replicates, confidence_millionths)?;
    let sign_test = sign(&task_effects)?;
    Ok(PairedComparison {
        table,
        net_effect_millionths: i32::try_from(net).map_err(|_| arithmetic())?,
        interval,
        sign_test,
    })
}

fn bootstrap(
    profile: ProfileDigest,
    task_effects: &[i64],
    replicates: u32,
    confidence: u32,
) -> Result<BootstrapInterval, EvaluationError> {
    let mut results = Vec::with_capacity(usize::try_from(replicates).map_err(|_| arithmetic())?);
    for replicate in 0..replicates {
        let mut sum = 0_i128;
        for draw in 0..task_effects.len() {
            let mut bytes = b"peritus.evaluation.task-bootstrap.v1\0".to_vec();
            bytes.extend_from_slice(profile.as_bytes());
            bytes.extend_from_slice(&replicate.to_be_bytes());
            bytes.extend_from_slice(&u32::try_from(draw).map_err(|_| arithmetic())?.to_be_bytes());
            let digest = peritus_codec::sha256(&bytes);
            let raw = u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("exact slice"));
            let index =
                usize::try_from(raw % u64::try_from(task_effects.len()).map_err(|_| arithmetic())?)
                    .map_err(|_| arithmetic())?;
            sum = sum.checked_add(i128::from(task_effects[index])).ok_or_else(arithmetic)?;
        }
        let mean = sum / i128::try_from(task_effects.len()).map_err(|_| arithmetic())?;
        results.push(i32::try_from(mean).map_err(|_| arithmetic())?);
    }
    results.sort_unstable();
    let tail = (1_000_000_u64 - u64::from(confidence)) / 2;
    let length = u64::try_from(results.len()).map_err(|_| arithmetic())?;
    let lower =
        usize::try_from(length.saturating_mul(tail) / 1_000_000).map_err(|_| arithmetic())?;
    let upper_rank = length.saturating_mul(1_000_000 - tail).div_ceil(1_000_000).max(1);
    let upper = usize::try_from(upper_rank - 1).map_err(|_| arithmetic())?.min(results.len() - 1);
    Ok(BootstrapInterval {
        lower_millionths: results[lower.min(results.len() - 1)],
        upper_millionths: results[upper],
        replicates,
        confidence_millionths: confidence,
    })
}

fn sign(task_effects: &[i64]) -> Result<SignTest, EvaluationError> {
    let positive = u32::try_from(task_effects.iter().filter(|value| **value > 0).count())
        .map_err(|_| arithmetic())?;
    let negative = u32::try_from(task_effects.iter().filter(|value| **value < 0).count())
        .map_err(|_| arithmetic())?;
    let tied = u32::try_from(task_effects.len()).map_err(|_| arithmetic())? - positive - negative;
    let n = usize::try_from(positive + negative).map_err(|_| arithmetic())?;
    let p = if n == 0 {
        1_000_000
    } else {
        let mode = n / 2;
        let mut weights = vec![0_u128; n + 1];
        weights[mode] = WEIGHT_SCALE;
        for index in (1..=mode).rev() {
            weights[index - 1] = weights[index]
                .checked_mul(u128::try_from(index).map_err(|_| arithmetic())?)
                .ok_or_else(arithmetic)?
                / u128::try_from(n - index + 1).map_err(|_| arithmetic())?;
        }
        for index in mode..n {
            weights[index + 1] = weights[index]
                .checked_mul(u128::try_from(n - index).map_err(|_| arithmetic())?)
                .ok_or_else(arithmetic)?
                / u128::try_from(index + 1).map_err(|_| arithmetic())?;
        }
        let denominator = weights
            .iter()
            .try_fold(0_u128, |sum, value| sum.checked_add(*value))
            .ok_or_else(arithmetic)?;
        let minor = usize::try_from(positive.min(negative)).map_err(|_| arithmetic())?;
        let tail = weights[..=minor]
            .iter()
            .try_fold(0_u128, |sum, value| sum.checked_add(*value))
            .ok_or_else(arithmetic)?;
        let doubled = tail.saturating_mul(2).min(denominator);
        u32::try_from(
            doubled
                .checked_mul(1_000_000)
                .ok_or_else(arithmetic)?
                .checked_add(denominator / 2)
                .ok_or_else(arithmetic)?
                / denominator,
        )
        .map_err(|_| arithmetic())?
    };
    Ok(SignTest {
        positive_tasks: positive,
        negative_tasks: negative,
        tied_tasks: tied,
        two_sided_p: ProbabilityMillionths::new(p)?,
    })
}

const fn invalid(detail: &'static str) -> EvaluationError {
    crate::invalid(EvaluationErrorKind::Statistics, EvaluationOperation::Analyze, detail)
}
const fn arithmetic() -> EvaluationError {
    invalid("paired comparison checked arithmetic overflowed")
}
