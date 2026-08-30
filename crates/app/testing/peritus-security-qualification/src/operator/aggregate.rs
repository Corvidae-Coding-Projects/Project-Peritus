//! Final three-host H0 aggregation and verified-policy reduction.

use std::env;
use std::ffi::OsString;

use crate::{
    QualificationReport, QualificationRunner, QualificationShard, final_report_json,
    parse_review_json,
};

use super::aggregate_args::AggregateOptions;
use super::{publish_report, read_bounded};

const MAX_SHARD_BYTES: u64 = 16 * 1024 * 1024;
const MAX_REVIEW_BYTES: u64 = 8 * 1024 * 1024;

/// Terminal status of a successfully assembled final H0 report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H0AggregateStatus {
    /// All three native shards and the independent review satisfy verified H0 policy.
    Ready,
    /// The final report was retained, but one or more H0 obligations remain unmet.
    NotReady,
}

/// Parses process arguments, aggregates three native shards, and publishes one final report.
///
/// # Errors
///
/// Returns syntax, filesystem, shard, external-review, policy, or publication failures.
pub fn run_from_env() -> Result<H0AggregateStatus, Box<dyn std::error::Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    run(&arguments)
}

fn run(arguments: &[OsString]) -> Result<H0AggregateStatus, Box<dyn std::error::Error>> {
    let options = AggregateOptions::parse(arguments)?;
    let shards = [&options.linux, &options.macos, &options.windows]
        .into_iter()
        .map(|path| {
            let bytes = read_bounded(path, MAX_SHARD_BYTES, "native shard")?;
            QualificationShard::parse_ready_json(&bytes).map_err(Box::<dyn std::error::Error>::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let run = QualificationRunner.aggregate(shards)?;
    let review_bytes = read_bounded(&options.review, MAX_REVIEW_BYTES, "external review")?;
    let review = parse_review_json(&review_bytes)?;
    let report = QualificationReport::evaluate(run, Some(review))?;
    let status =
        if report.is_ready() { H0AggregateStatus::Ready } else { H0AggregateStatus::NotReady };
    publish_report(&options.report, &final_report_json(&report)?)?;
    Ok(status)
}
