//! Task-cluster paired comparison, bootstrap interval, and task-level sign diagnostic.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
};

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

use crate::{
    BootstrapBatchWork, EvaluationError, EvaluationErrorKind, EvaluationOperation,
    EvaluationRecovery, ProbabilityMillionths, ProfileDigest, TaskId,
};

const EFFECT_SCALE: i64 = 1_000_000;
const WEIGHT_SCALE: u128 = 1_000_000_000_000_000_000;
const LEGACY_BOOTSTRAP_DOMAIN: &[u8] = b"peritus.evaluation.task-bootstrap.v1\0";
const RESUMABLE_BOOTSTRAP_DOMAIN: &[u8] = b"peritus.evaluation.task-bootstrap.v2\0";
const PAIRED_CELLS_DOMAIN: &[u8] = b"peritus.evaluation.paired-cells.v1\0";
const BOOTSTRAP_CHECKPOINT_DOMAIN: &[u8] =
    b"peritus.evaluation.task-bootstrap-checkpoint.v1\0";
const COUNTER_GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

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

/// Exact position of the next deterministic bootstrap draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootstrapCursor {
    completed_replicates: u32,
    next_draw: u32,
    total_replicates: u32,
}

impl BootstrapCursor {
    /// Fully incorporated replicate count.
    #[must_use]
    pub const fn completed_replicates(self) -> u32 {
        self.completed_replicates
    }
    /// Zero-based draw within the next replicate.
    #[must_use]
    pub const fn next_draw(self) -> u32 {
        self.next_draw
    }
    /// Frozen cumulative replicate request.
    #[must_use]
    pub const fn total_replicates(self) -> u32 {
        self.total_replicates
    }
}

/// Outcome of one explicitly bounded bootstrap turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairedBootstrapStatus {
    /// The physical draw budget was consumed at an exact resumable cursor.
    Pending,
    /// Cancellation was observed before the next draw.
    Cancelled,
    /// Every requested replicate was incorporated.
    Complete,
}

/// Deterministic task-cluster bootstrap with an exact resumable frontier.
#[derive(Clone, Debug)]
pub struct PairedComparisonJob {
    profile: ProfileDigest,
    cells_digest: peritus_types::Sha256Digest,
    table: PairedTable,
    net_effect_millionths: i32,
    sign_test: SignTest,
    task_effects: Vec<i64>,
    replicates: u32,
    confidence_millionths: u32,
    seed: u64,
    completed_replicates: u32,
    next_draw: u32,
    partial_sum: i128,
    lower_frontier: BinaryHeap<i32>,
    upper_frontier: BinaryHeap<Reverse<i32>>,
}

impl PairedComparisonJob {
    /// Starts the version-two counter-based bootstrap stream.
    ///
    /// # Errors
    /// Rejects empty, duplicate/noncanonical pairs, unsupported confidence, or arithmetic overflow.
    pub fn new(
        profile: ProfileDigest,
        cells: &[PairedCell],
        replicates: u32,
        confidence_millionths: u32,
    ) -> Result<Self, EvaluationError> {
        validate_request(cells, replicates, confidence_millionths)?;
        let inputs = paired_inputs(cells)?;
        let cells_digest = paired_cells_digest(cells)?;
        Ok(Self {
            profile,
            cells_digest,
            table: inputs.table,
            net_effect_millionths: inputs.net_effect_millionths,
            sign_test: inputs.sign_test,
            task_effects: inputs.task_effects,
            replicates,
            confidence_millionths,
            seed: resumable_seed(profile),
            completed_replicates: 0,
            next_draw: 0,
            partial_sum: 0,
            lower_frontier: BinaryHeap::new(),
            upper_frontier: BinaryHeap::new(),
        })
    }

    /// Restores an exact frontier after independently rebuilding and binding its immutable inputs.
    ///
    /// # Errors
    /// Rejects malformed, noncanonical, drifted, or arithmetically impossible checkpoint bytes.
    pub fn restore(
        profile: ProfileDigest,
        cells: &[PairedCell],
        replicates: u32,
        confidence_millionths: u32,
        checkpoint: &[u8],
    ) -> Result<Self, EvaluationError> {
        let mut job = Self::new(profile, cells, replicates, confidence_millionths)?;
        job.restore_checkpoint(checkpoint)?;
        Ok(job)
    }

    /// Advances at most the caller-selected number of draws and checks cancellation before each one.
    ///
    /// # Errors
    /// Returns a checked arithmetic error if the frozen workload cannot be represented exactly.
    pub fn advance(
        &mut self,
        work: BootstrapBatchWork,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<PairedBootstrapStatus, EvaluationError> {
        if self.completed_replicates == self.replicates {
            return Ok(PairedBootstrapStatus::Complete);
        }
        let task_count = u32::try_from(self.task_effects.len()).map_err(|_| arithmetic())?;
        let mut remaining = work.maximum_draws();
        while self.completed_replicates < self.replicates {
            if is_cancelled() {
                return Ok(PairedBootstrapStatus::Cancelled);
            }
            if remaining == 0 {
                return Ok(PairedBootstrapStatus::Pending);
            }
            let index = self.draw_index(task_count)?;
            self.partial_sum = self
                .partial_sum
                .checked_add(i128::from(self.task_effects[index]))
                .ok_or_else(arithmetic)?;
            self.next_draw = self.next_draw.checked_add(1).ok_or_else(arithmetic)?;
            remaining -= 1;
            if self.next_draw == task_count {
                self.finish_replicate(task_count)?;
            }
        }
        Ok(PairedBootstrapStatus::Complete)
    }

    /// Returns the exact next-draw cursor.
    #[must_use]
    pub const fn cursor(&self) -> BootstrapCursor {
        BootstrapCursor {
            completed_replicates: self.completed_replicates,
            next_draw: self.next_draw,
            total_replicates: self.replicates,
        }
    }

    /// Encodes the complete deterministic frontier for durable artifact storage.
    ///
    /// # Errors
    /// Rejects a frontier that exceeds canonical codec bounds.
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(BOOTSTRAP_CHECKPOINT_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.profile.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.cells_digest.as_bytes()).map_err(codec)?;
        writer.write_u32(self.replicates).map_err(codec)?;
        writer.write_u32(self.confidence_millionths).map_err(codec)?;
        writer.write_u64(self.seed).map_err(codec)?;
        writer
            .write_u32(u32::try_from(self.task_effects.len()).map_err(|_| arithmetic())?)
            .map_err(codec)?;
        writer.write_u32(self.completed_replicates).map_err(codec)?;
        writer.write_u32(self.next_draw).map_err(codec)?;
        writer.write_fixed(&self.partial_sum.to_be_bytes()).map_err(codec)?;
        let mut lower = self.lower_frontier.iter().copied().collect::<Vec<_>>();
        lower.sort_unstable();
        writer.write_collection_len(lower.len()).map_err(codec)?;
        for value in lower {
            writer.write_fixed(&i64::from(value).to_be_bytes()).map_err(codec)?;
        }
        let mut upper =
            self.upper_frontier.iter().map(|value| value.0).collect::<Vec<_>>();
        upper.sort_unstable();
        writer.write_collection_len(upper.len()).map_err(codec)?;
        for value in upper {
            writer.write_fixed(&i64::from(value).to_be_bytes()).map_err(codec)?;
        }
        Ok(writer.into_bytes())
    }

    /// Materializes the same public evidence shape after every requested replicate is complete.
    ///
    /// # Errors
    /// Rejects an incomplete or internally inconsistent frontier.
    pub fn comparison(&self) -> Result<PairedComparison, EvaluationError> {
        if self.completed_replicates != self.replicates || self.next_draw != 0 {
            return Err(invalid("paired bootstrap frontier is incomplete"));
        }
        let lower = self.lower_frontier.peek().copied().ok_or_else(arithmetic)?;
        let upper = self.upper_frontier.peek().map(|value| value.0).ok_or_else(arithmetic)?;
        Ok(PairedComparison {
            table: self.table,
            net_effect_millionths: self.net_effect_millionths,
            interval: BootstrapInterval {
                lower_millionths: lower,
                upper_millionths: upper,
                replicates: self.replicates,
                confidence_millionths: self.confidence_millionths,
            },
            sign_test: self.sign_test,
        })
    }

    fn draw_index(&self, task_count: u32) -> Result<usize, EvaluationError> {
        let task_count = u64::from(task_count);
        let counter = u64::from(self.completed_replicates)
            .checked_mul(task_count)
            .and_then(|value| value.checked_add(u64::from(self.next_draw)))
            .ok_or_else(arithmetic)?;
        let raw = counter_word(self.seed, counter);
        usize::try_from(raw % task_count).map_err(|_| arithmetic())
    }

    fn finish_replicate(&mut self, task_count: u32) -> Result<(), EvaluationError> {
        let mean = self.partial_sum / i128::from(task_count);
        let value = i32::try_from(mean).map_err(|_| arithmetic())?;
        let (lower_keep, upper_keep) = quantile_frontier_sizes(
            self.replicates,
            self.confidence_millionths,
        )?;
        self.lower_frontier.push(value);
        if self.lower_frontier.len() > lower_keep {
            self.lower_frontier.pop();
        }
        self.upper_frontier.push(Reverse(value));
        if self.upper_frontier.len() > upper_keep {
            self.upper_frontier.pop();
        }
        self.completed_replicates =
            self.completed_replicates.checked_add(1).ok_or_else(arithmetic)?;
        self.next_draw = 0;
        self.partial_sum = 0;
        Ok(())
    }

    fn restore_checkpoint(&mut self, checkpoint: &[u8]) -> Result<(), EvaluationError> {
        let mut reader = CanonicalReader::new(checkpoint, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != BOOTSTRAP_CHECKPOINT_DOMAIN
            || ProfileDigest::new(peritus_types::Sha256Digest::new(
                reader.read_fixed().map_err(codec)?,
            )) != self.profile
            || peritus_types::Sha256Digest::new(reader.read_fixed().map_err(codec)?)
                != self.cells_digest
            || reader.read_u32().map_err(codec)? != self.replicates
            || reader.read_u32().map_err(codec)? != self.confidence_millionths
            || reader.read_u64().map_err(codec)? != self.seed
            || reader.read_u32().map_err(codec)?
                != u32::try_from(self.task_effects.len()).map_err(|_| arithmetic())?
        {
            return Err(invalid("paired bootstrap checkpoint input binding differs"));
        }
        let completed_replicates = reader.read_u32().map_err(codec)?;
        let next_draw = reader.read_u32().map_err(codec)?;
        let partial_sum = i128::from_be_bytes(reader.read_fixed().map_err(codec)?);
        let lower = read_frontier(&mut reader)?;
        let upper = read_frontier(&mut reader)?;
        reader.finish().map_err(codec)?;
        let task_count = u32::try_from(self.task_effects.len()).map_err(|_| arithmetic())?;
        let (lower_keep, upper_keep) = quantile_frontier_sizes(
            self.replicates,
            self.confidence_millionths,
        )?;
        let completed = usize::try_from(completed_replicates).map_err(|_| arithmetic())?;
        let partial_bound = i128::from(EFFECT_SCALE)
            .checked_mul(i128::from(next_draw))
            .ok_or_else(arithmetic)?;
        if completed_replicates > self.replicates
            || next_draw >= task_count
            || (completed_replicates == self.replicates && next_draw != 0)
            || partial_sum < -partial_bound
            || partial_sum > partial_bound
            || lower.len() != completed.min(lower_keep)
            || upper.len() != completed.min(upper_keep)
            || lower.windows(2).any(|values| values[0] > values[1])
            || upper.windows(2).any(|values| values[0] > values[1])
            || lower.iter().chain(&upper).any(|value| {
                !(-i32::try_from(EFFECT_SCALE).unwrap_or(i32::MAX)
                    ..=i32::try_from(EFFECT_SCALE).unwrap_or(i32::MAX))
                    .contains(value)
            })
        {
            return Err(invalid("paired bootstrap checkpoint frontier is noncanonical"));
        }
        self.completed_replicates = completed_replicates;
        self.next_draw = next_draw;
        self.partial_sum = partial_sum;
        self.lower_frontier = BinaryHeap::from(lower);
        self.upper_frontier = BinaryHeap::from(upper.into_iter().map(Reverse).collect::<Vec<_>>());
        Ok(())
    }
}

#[derive(Debug)]
struct PairedInputs {
    table: PairedTable,
    net_effect_millionths: i32,
    sign_test: SignTest,
    task_effects: Vec<i64>,
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
    validate_request(cells, replicates, confidence_millionths)?;
    let inputs = paired_inputs(cells)?;
    let interval = bootstrap_legacy(
        profile,
        &inputs.task_effects,
        replicates,
        confidence_millionths,
    )?;
    Ok(PairedComparison {
        table: inputs.table,
        net_effect_millionths: inputs.net_effect_millionths,
        interval,
        sign_test: inputs.sign_test,
    })
}

fn validate_request(
    cells: &[PairedCell],
    replicates: u32,
    confidence_millionths: u32,
) -> Result<(), EvaluationError> {
    if cells.is_empty() || replicates == 0 || confidence_millionths != 950_000 {
        return Err(invalid("paired comparison inputs are empty or unsupported"));
    }
    if cells.windows(2).any(|pair| {
        (pair[0].task_id(), pair[0].ordinal()) >= (pair[1].task_id(), pair[1].ordinal())
    }) {
        return Err(invalid("paired cells are duplicated or noncanonical"));
    }
    Ok(())
}

fn paired_inputs(cells: &[PairedCell]) -> Result<PairedInputs, EvaluationError> {
    let mut table = PairedTable::default();
    let mut tasks: BTreeMap<TaskId, Vec<i8>> = BTreeMap::new();
    for cell in cells {
        let effect = match (cell.baseline_passed(), cell.candidate_passed()) {
            (true, true) => {
                table.both_passed = table.both_passed.checked_add(1).ok_or_else(arithmetic)?;
                0
            }
            (false, true) => {
                table.candidate_only = table.candidate_only.checked_add(1).ok_or_else(arithmetic)?;
                1
            }
            (true, false) => {
                table.baseline_only = table.baseline_only.checked_add(1).ok_or_else(arithmetic)?;
                -1
            }
            (false, false) => {
                table.both_failed = table.both_failed.checked_add(1).ok_or_else(arithmetic)?;
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
    let task_effects = tasks
        .values()
        .map(|values| {
            let sum = values.iter().try_fold(0_i64, |total, value| {
                total.checked_add(i64::from(*value)).ok_or_else(arithmetic)
            })?;
            sum.checked_mul(EFFECT_SCALE).ok_or_else(arithmetic)?
                / i64::try_from(values.len()).map_err(|_| arithmetic())?
        })
        .collect::<Result<Vec<_>, EvaluationError>>()?;
    let sign_test = sign(&task_effects)?;
    Ok(PairedInputs {
        table,
        net_effect_millionths: i32::try_from(net).map_err(|_| arithmetic())?,
        sign_test,
        task_effects,
    })
}

fn bootstrap_legacy(
    profile: ProfileDigest,
    task_effects: &[i64],
    replicates: u32,
    confidence: u32,
) -> Result<BootstrapInterval, EvaluationError> {
    let mut results = Vec::with_capacity(usize::try_from(replicates).map_err(|_| arithmetic())?);
    for replicate in 0..replicates {
        let mut sum = 0_i128;
        for draw in 0..task_effects.len() {
            let mut bytes = [0_u8; LEGACY_BOOTSTRAP_DOMAIN.len() + 40];
            let profile_start = LEGACY_BOOTSTRAP_DOMAIN.len();
            let replicate_start = profile_start + 32;
            let draw_start = replicate_start + 4;
            bytes[..profile_start].copy_from_slice(LEGACY_BOOTSTRAP_DOMAIN);
            bytes[profile_start..replicate_start].copy_from_slice(profile.as_bytes());
            bytes[replicate_start..draw_start].copy_from_slice(&replicate.to_be_bytes());
            bytes[draw_start..]
                .copy_from_slice(&u32::try_from(draw).map_err(|_| arithmetic())?.to_be_bytes());
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

fn paired_cells_digest(cells: &[PairedCell]) -> Result<peritus_types::Sha256Digest, EvaluationError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_bytes(PAIRED_CELLS_DOMAIN).map_err(codec)?;
    writer.write_collection_len(cells.len()).map_err(codec)?;
    for cell in cells {
        writer.write_fixed(cell.task_id().as_bytes()).map_err(codec)?;
        writer.write_u16(cell.ordinal()).map_err(codec)?;
        writer.write_bool(cell.baseline_passed()).map_err(codec)?;
        writer.write_bool(cell.candidate_passed()).map_err(codec)?;
    }
    Ok(peritus_codec::sha256(&writer.into_bytes()))
}

fn resumable_seed(profile: ProfileDigest) -> u64 {
    let mut bytes = Vec::with_capacity(RESUMABLE_BOOTSTRAP_DOMAIN.len() + 32);
    bytes.extend_from_slice(RESUMABLE_BOOTSTRAP_DOMAIN);
    bytes.extend_from_slice(profile.as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("exact digest prefix"))
}

const fn counter_word(seed: u64, counter: u64) -> u64 {
    let mut value = seed.wrapping_add(counter.wrapping_mul(COUNTER_GAMMA));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn quantile_frontier_sizes(
    replicates: u32,
    confidence: u32,
) -> Result<(usize, usize), EvaluationError> {
    let length = u64::from(replicates);
    let tail = (1_000_000_u64 - u64::from(confidence)) / 2;
    let lower = length.saturating_mul(tail) / 1_000_000;
    let upper_rank = length.saturating_mul(1_000_000 - tail).div_ceil(1_000_000).max(1);
    let upper = upper_rank.saturating_sub(1).min(length.saturating_sub(1));
    Ok((
        usize::try_from(lower.checked_add(1).ok_or_else(arithmetic)?).map_err(|_| arithmetic())?,
        usize::try_from(length.checked_sub(upper).ok_or_else(arithmetic)?)
            .map_err(|_| arithmetic())?,
    ))
}

fn read_frontier(reader: &mut CanonicalReader<'_>) -> Result<Vec<i32>, EvaluationError> {
    let length = reader.read_collection_len(8).map_err(codec)?;
    let mut values = reader.reserve_collection(length).map_err(codec)?;
    for _ in 0..length {
        values.push(
            i32::try_from(i64::from_be_bytes(reader.read_fixed().map_err(codec)?))
                .map_err(|_| invalid("bootstrap checkpoint value is outside effect bounds"))?,
        );
    }
    Ok(values)
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
const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::Analyze,
        EvaluationRecovery::ReduceScope,
        "paired bootstrap checkpoint exceeds canonical codec limits",
    )
}
