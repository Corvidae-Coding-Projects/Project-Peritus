//! Standard empty suite catalog for protocol-owning slices to extend later.

use crate::{ReportText, StaticSuite, SuiteDescriptor, SuiteId};

fn empty<S>(id: &'static str, summary: &'static str) -> StaticSuite<S> {
    StaticSuite::empty(SuiteDescriptor::new(SuiteId::catalog(id), ReportText::literal(summary)))
}

/// Returns the runnable empty model-provider conformance suite.
///
/// B3 and C5 own the future provider protocol and adapter cases.
#[must_use]
pub fn provider_suite<S>() -> StaticSuite<S> {
    empty(
        "peritus.provider",
        "Model-provider conformance cases supplied after provider protocols exist",
    )
}

/// Returns the runnable empty tool conformance suite.
///
/// B3 and C4 own the future tool protocol and router cases.
#[must_use]
pub fn tool_suite<S>() -> StaticSuite<S> {
    empty("peritus.tool", "Tool conformance cases supplied after tool protocols exist")
}

/// Returns the runnable empty plugin conformance suite.
///
/// G3 owns the future plugin contract and host cases.
#[must_use]
pub fn plugin_suite<S>() -> StaticSuite<S> {
    empty("peritus.plugin", "Plugin conformance cases supplied after plugin contracts exist")
}

/// Returns the runnable empty sandbox conformance suite.
///
/// C2 and C3 own the future abstract and platform sandbox cases.
#[must_use]
pub fn sandbox_suite<S>() -> StaticSuite<S> {
    empty("peritus.sandbox", "Sandbox conformance cases supplied after sandbox contracts exist")
}

/// Returns the runnable empty journal conformance suite.
///
/// C0 owns the future journal contract and storage cases.
#[must_use]
pub fn journal_suite<S>() -> StaticSuite<S> {
    empty("peritus.journal", "Journal conformance cases supplied after journal contracts exist")
}

/// Returns the runnable empty protocol conformance suite.
///
/// B3 owns the future domain protocol and codec cases.
#[must_use]
pub fn protocol_suite<S>() -> StaticSuite<S> {
    empty("peritus.protocol", "Protocol conformance cases supplied after domain protocols exist")
}

/// Returns the runnable empty replay conformance suite.
///
/// B3 and C0 own the future replay definitions and cases.
#[must_use]
pub fn replay_suite<S>() -> StaticSuite<S> {
    empty(
        "peritus.replay",
        "Replay conformance cases supplied after event and journal contracts exist",
    )
}
