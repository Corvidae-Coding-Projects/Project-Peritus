//! Pure durable debugger job state machine.

#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-private codec helpers cross sibling private modules without becoming public API"
)]

mod command;
mod event;
mod reducer;
mod state;
mod types;

pub use command::{DebuggerCommand, DebuggerCommandKind};
pub use event::{DebuggerEvent, DebuggerEventKind, DebuggerTransition};
pub use reducer::{apply_event, decide, replay};
pub use state::DebuggerState;
pub use types::{
    AnalysisCounts, DebuggerPhase, JobFailure, JobFailureCode, ModelAcceptanceCertainty,
    ModelAttemptFailure, ModelAttemptFailureCode, ModelAttemptObservation, ModelAttemptResult,
    ModelBudget, ModelFailureContext, ModelFailureOrigin, ModelFailurePhase, ModelFailureRecovery,
    ModelProgress, ModelProviderFailureCause, ModelRetryPolicy, ModelRetrySchedule,
    ModelStartBasis, ModelWorkState, PublicationRecord, ReportRecord, SelectionRecord,
};

pub(super) fn encode_kind(
    writer: &mut peritus_codec::CanonicalWriter,
    kind: &DebuggerCommandKind,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_kind(writer, kind)
}

pub(super) fn encode_revision(
    writer: &mut peritus_codec::CanonicalWriter,
    revision: &peritus_types::RevisionTuple,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_revision(writer, revision)
}

pub(super) fn encode_counts(
    writer: &mut peritus_codec::CanonicalWriter,
    counts: AnalysisCounts,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_counts(writer, counts)
}

pub(super) fn encode_model_budget(
    writer: &mut peritus_codec::CanonicalWriter,
    budget: ModelBudget,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_model_budget(writer, budget)
}

pub(super) fn encode_retry_policy(
    writer: &mut peritus_codec::CanonicalWriter,
    policy: ModelRetryPolicy,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_retry_policy(writer, policy)
}

pub(super) fn encode_authority_instant(
    writer: &mut peritus_codec::CanonicalWriter,
    instant: peritus_policy::AuthorityInstant,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_authority_instant(writer, instant)
}

pub(super) fn encode_retry_schedule(
    writer: &mut peritus_codec::CanonicalWriter,
    schedule: ModelRetrySchedule,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_retry_schedule(writer, schedule)
}

pub(super) fn decode_authority_instant(
    reader: &mut peritus_codec::CanonicalReader<'_>,
) -> Result<peritus_policy::AuthorityInstant, crate::DebuggerError> {
    command::codec::decode_authority_instant(reader)
}

pub(super) fn decode_retry_schedule(
    reader: &mut peritus_codec::CanonicalReader<'_>,
) -> Result<ModelRetrySchedule, crate::DebuggerError> {
    command::codec::decode_retry_schedule(reader)
}

pub(super) fn encode_model_failure(
    writer: &mut peritus_codec::CanonicalWriter,
    failure: &ModelAttemptFailure,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_model_failure(writer, failure)
}

pub(super) fn decode_model_failure(
    reader: &mut peritus_codec::CanonicalReader<'_>,
    rich: bool,
) -> Result<ModelAttemptFailure, crate::DebuggerError> {
    command::codec::decode_model_failure(reader, rich)
}

pub(super) fn encode_report(
    writer: &mut peritus_codec::CanonicalWriter,
    report: ReportRecord,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_report(writer, report)
}

pub(super) fn encode_publication(
    writer: &mut peritus_codec::CanonicalWriter,
    publication: PublicationRecord,
) -> Result<(), crate::DebuggerError> {
    command::codec::encode_publication(writer, publication)
}
