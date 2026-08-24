//! Executable C4 tool protocol and router conformance cases.

use super::fixtures::{AUTHORITY_DRIFTS, fixture};
use super::{ToolConformanceSubject, ToolDisposition, ToolReplayMode, ToolScenario};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct ToolCase {
    descriptor: CaseDescriptor,
    kind: ToolCaseKind,
}

#[derive(Clone, Copy)]
enum ToolCaseKind {
    DescriptorSchema,
    SchemaNoEffect,
    Exposure,
    Dispatch,
    AuthorizationNoEffect,
    ResultTruth,
    Controls,
    Replay,
}

impl<S: ToolConformanceSubject> ConformanceCase<S> for ToolCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move {
            let exact = match self.kind {
                ToolCaseKind::DescriptorSchema => descriptor_schema(subject),
                ToolCaseKind::SchemaNoEffect => schema_no_effect(subject),
                ToolCaseKind::Exposure => exposure(subject),
                ToolCaseKind::Dispatch => dispatch(subject),
                ToolCaseKind::AuthorizationNoEffect => authorization_no_effect(subject),
                ToolCaseKind::ResultTruth => result_truth(subject),
                ToolCaseKind::Controls => controls(subject),
                ToolCaseKind::Replay => replay(subject),
            };
            result(exact, self.kind)
        })
    }
}

/// Returns the complete runtime-neutral C4 tool conformance suite.
#[must_use]
pub fn tool_suite<S: ToolConformanceSubject + 'static>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.tool"),
            ReportText::literal(
                "C4 descriptor, routing, authority, lifecycle, and replay contract",
            ),
        ),
        vec![
            boxed(
                "authorization-no-effect",
                "Authority drift creates no permit, dispatch, or effect",
                ToolCaseKind::AuthorizationNoEffect,
            ),
            boxed(
                "controls-and-deadline",
                "Control and deadline paths retain owned terminal results",
                ToolCaseKind::Controls,
            ),
            boxed(
                "descriptor-schema",
                "Descriptor and schema generation is deterministic and exact",
                ToolCaseKind::DescriptorSchema,
            ),
            boxed(
                "dispatch-once",
                "Exact authority reaches the bound dispatcher once",
                ToolCaseKind::Dispatch,
            ),
            boxed("exposure", "Role and capability exposure is canonical", ToolCaseKind::Exposure),
            boxed("replay", "Replay never duplicates or guesses an effect", ToolCaseKind::Replay),
            boxed(
                "result-truth",
                "Structured status cannot be contradicted by prose",
                ToolCaseKind::ResultTruth,
            ),
            boxed(
                "schema-no-effect",
                "Invalid arguments are rejected before authority",
                ToolCaseKind::SchemaNoEffect,
            ),
        ],
    )
}

fn boxed<S: ToolConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    kind: ToolCaseKind,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(ToolCase {
        descriptor: CaseDescriptor::new(
            CaseId::catalog(match suffix {
                "authorization-no-effect" => "peritus.tool.authorization-no-effect",
                "controls-and-deadline" => "peritus.tool.controls-and-deadline",
                "descriptor-schema" => "peritus.tool.descriptor-schema",
                "dispatch-once" => "peritus.tool.dispatch-once",
                "exposure" => "peritus.tool.exposure",
                "replay" => "peritus.tool.replay",
                "result-truth" => "peritus.tool.result-truth",
                _ => "peritus.tool.schema-no-effect",
            }),
            ReportText::literal(summary),
        ),
        kind,
    })
}

fn descriptor_schema<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    let request = fixture(ToolScenario::DescriptorSchema);
    subject.exercise(&request).is_ok_and(|observed| {
        observed.descriptor().is_some_and(|descriptor| {
            descriptor.name() == request.tool_name()
                && descriptor.deterministic()
                && descriptor.operation_matches()
                && descriptor.implementation_matches()
                && descriptor.schema_bounded()
        }) && observed.canonical_exposure()
    })
}

fn schema_no_effect<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    subject.exercise(&fixture(ToolScenario::SchemaRejection)).is_ok_and(|observed| {
        observed.disposition() == ToolDisposition::Rejected
            && !observed.schema_accepted()
            && !observed.effects().any()
    })
}

fn exposure<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    subject.exercise(&fixture(ToolScenario::Exposure)).is_ok_and(|observed| {
        observed.exposed() && observed.canonical_exposure() && !observed.effects().any()
    })
}

fn dispatch<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    let request = fixture(ToolScenario::Dispatch);
    subject.exercise(&request).is_ok_and(|observed| {
        let effects = observed.effects();
        let result = observed.result();
        observed.disposition() == ToolDisposition::Succeeded
            && observed.schema_accepted()
            && effects.permits_created() == 1
            && effects.permits_consumed() == 1
            && effects.dispatcher_starts() == 1
            && effects.target_effects() == 1
            && result.structured_result()
            && !result.structured_failure()
            && result.human_bytes() <= request.output_limit()
            && result.model_bytes() <= request.output_limit()
            && result.timing_present()
    })
}

fn authorization_no_effect<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    AUTHORITY_DRIFTS.into_iter().all(|drift| {
        subject.exercise(&fixture(ToolScenario::Authorization(drift))).is_ok_and(|observed| {
            observed.disposition() == ToolDisposition::Rejected && !observed.effects().any()
        })
    })
}

fn result_truth<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    let request = fixture(ToolScenario::ResultTruth);
    subject.exercise(&request).is_ok_and(|observed| {
        let result = observed.result();
        observed.disposition() == ToolDisposition::Failed
            && !result.structured_result()
            && result.structured_failure()
            && result.timing_present()
            && result.human_bytes() <= request.output_limit()
            && result.model_bytes() <= request.output_limit()
            && (result.artifact_count() == 0 || result.truncation_declared())
            && !result.retryable()
    })
}

fn controls<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    let cancelled = subject.exercise(&fixture(ToolScenario::Cancellation));
    let timed_out = subject.exercise(&fixture(ToolScenario::Deadline));
    cancelled.is_ok_and(|observed| {
        observed.disposition() == ToolDisposition::Cancelled
            && observed.control_observed()
            && observed.execution_joined()
            && monotonic_nonzero(observed.progress_sequences())
    }) && timed_out.is_ok_and(|observed| {
        observed.disposition() == ToolDisposition::TimedOut
            && observed.control_observed()
            && observed.execution_joined()
            && monotonic_nonzero(observed.progress_sequences())
    })
}

fn replay<S: ToolConformanceSubject>(subject: &mut S) -> bool {
    [
        ToolReplayMode::ExactIdempotent,
        ToolReplayMode::Conflicting,
        ToolReplayMode::NonIdempotent,
        ToolReplayMode::Indeterminate,
    ]
    .into_iter()
    .all(|mode| {
        subject.exercise(&fixture(ToolScenario::Replay(mode))).is_ok_and(|observed| {
            let replay = observed.replay();
            !replay.second_effect()
                && match mode {
                    ToolReplayMode::ExactIdempotent => {
                        observed.disposition() == ToolDisposition::Replayed
                            && replay.prior_result_returned()
                    }
                    ToolReplayMode::Conflicting | ToolReplayMode::NonIdempotent => {
                        observed.disposition() == ToolDisposition::Rejected
                            && replay.conflict_rejected()
                    }
                    ToolReplayMode::Indeterminate => {
                        observed.disposition() == ToolDisposition::Indeterminate
                            && replay.indeterminate_rejected()
                    }
                }
        })
    })
}

fn result(exact: bool, kind: ToolCaseKind) -> CaseResult {
    let observations =
        vec![Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact))];
    if exact {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(observations, assertion(kind))
    }
}

fn assertion(kind: ToolCaseKind) -> AssertionFailure {
    let (code, summary) = match kind {
        ToolCaseKind::DescriptorSchema => ("001", "descriptor or schema identity drifted"),
        ToolCaseKind::SchemaNoEffect => ("002", "invalid arguments reached authority or effect"),
        ToolCaseKind::Exposure => ("003", "exposure was not canonical and capability-bound"),
        ToolCaseKind::Dispatch => ("004", "exact call did not dispatch and complete exactly once"),
        ToolCaseKind::AuthorizationNoEffect => ("005", "authority drift produced a tool effect"),
        ToolCaseKind::ResultTruth => ("006", "failure was hidden or envelope bounds were lost"),
        ToolCaseKind::Controls => {
            ("007", "control path lost ordering, ownership, or terminal state")
        }
        ToolCaseKind::Replay => ("008", "replay duplicated or guessed a prior effect"),
    };
    AssertionFailure::new(
        FailureCode::catalog(match code {
            "001" => "PERITUS-TOOL-CONFORMANCE-001",
            "002" => "PERITUS-TOOL-CONFORMANCE-002",
            "003" => "PERITUS-TOOL-CONFORMANCE-003",
            "004" => "PERITUS-TOOL-CONFORMANCE-004",
            "005" => "PERITUS-TOOL-CONFORMANCE-005",
            "006" => "PERITUS-TOOL-CONFORMANCE-006",
            "007" => "PERITUS-TOOL-CONFORMANCE-007",
            _ => "PERITUS-TOOL-CONFORMANCE-008",
        }),
        ReportText::literal(summary),
        None,
        None,
    )
}

fn monotonic_nonzero(values: &[u64]) -> bool {
    values.first().is_some_and(|value| *value > 0)
        && values.windows(2).all(|pair| pair[0] < pair[1])
}
