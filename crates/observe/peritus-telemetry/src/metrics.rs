//! Stable metric names, points, and checked cumulative state.

use std::collections::{BTreeMap, btree_map};

use peritus_trace::{DiagnosticCode, ObservedTime, TraceId};

use crate::{TelemetryError, TelemetryErrorKind};

/// Stable C7 metric names.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MetricName {
    /// Provider requests started.
    ProviderRequests,
    /// Provider failures.
    ProviderFailures,
    /// Tool dispatches started.
    ToolDispatches,
    /// Tool failures.
    ToolFailures,
    /// Gates started.
    GateExecutions,
    /// Gates passed.
    GatePasses,
    /// Gates failed or blocked.
    GateFailures,
    /// Budget observations.
    BudgetEvents,
    /// Bounded retries scheduled.
    Retries,
    /// Cancellation observations.
    Cancellations,
    /// Recovery operations.
    Recoveries,
    /// Resource observations.
    ResourceObservations,
    /// Explicit exporter failures.
    ExporterFailures,
    /// Bounded buffer drops.
    BufferDrops,
    /// Shutdown operations.
    Shutdowns,
}

impl MetricName {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::ProviderRequests => 1,
            Self::ProviderFailures => 2,
            Self::ToolDispatches => 3,
            Self::ToolFailures => 4,
            Self::GateExecutions => 5,
            Self::GatePasses => 6,
            Self::GateFailures => 7,
            Self::BudgetEvents => 8,
            Self::Retries => 9,
            Self::Cancellations => 10,
            Self::Recoveries => 11,
            Self::ResourceObservations => 12,
            Self::ExporterFailures => 13,
            Self::BufferDrops => 14,
            Self::Shutdowns => 15,
        }
    }
}

/// One redaction-safe monotonic metric observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MetricPoint {
    name: MetricName,
    value: u64,
    time: ObservedTime,
    trace_id: TraceId,
}

impl MetricPoint {
    /// Creates a metric point.
    #[must_use]
    pub const fn new(name: MetricName, value: u64, time: ObservedTime, trace_id: TraceId) -> Self {
        Self { name, value, time, trace_id }
    }
    /// Returns the stable metric name.
    #[must_use]
    pub const fn name(self) -> MetricName {
        self.name
    }
    /// Returns the monotonic cumulative value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }
    /// Returns caller-observed time.
    #[must_use]
    pub const fn time(self) -> ObservedTime {
        self.time
    }
    /// Returns the correlated trace identity.
    #[must_use]
    pub const fn trace_id(self) -> TraceId {
        self.trace_id
    }
}

/// Checked cumulative metric state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MetricState {
    counters: BTreeMap<MetricName, u64>,
}

/// Canonical borrowed iterator over copied metric names and cumulative values.
pub struct MetricIter<'a> {
    inner: btree_map::Iter<'a, MetricName, u64>,
}

impl Iterator for MetricIter<'_> {
    type Item = (MetricName, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(name, value)| (*name, *value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for MetricIter<'_> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'a> IntoIterator for &'a MetricState {
    type Item = (MetricName, u64);
    type IntoIter = MetricIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl MetricState {
    /// Returns one metric's current cumulative value.
    #[must_use]
    pub fn value(&self, name: MetricName) -> u64 {
        self.counters.get(&name).copied().unwrap_or(0)
    }

    /// Iterates stable names and cumulative values in canonical name order.
    #[must_use]
    pub fn iter(&self) -> MetricIter<'_> {
        MetricIter { inner: self.counters.iter() }
    }

    pub(crate) fn observe(
        &mut self,
        code: DiagnosticCode,
        time: ObservedTime,
        trace_id: TraceId,
    ) -> Result<Option<MetricPoint>, TelemetryError> {
        let Some(name) = metric_for(code) else { return Ok(None) };
        let current = self.value(name);
        let next = current.checked_add(1).ok_or_else(|| {
            TelemetryError::new(
                TelemetryErrorKind::SequenceOverflow,
                "project diagnostic metric",
                "metric counter overflow",
            )
        })?;
        self.counters.insert(name, next);
        Ok(Some(MetricPoint::new(name, next, time, trace_id)))
    }
}

const fn metric_for(code: DiagnosticCode) -> Option<MetricName> {
    match code {
        DiagnosticCode::ProviderRequestStarted => Some(MetricName::ProviderRequests),
        DiagnosticCode::ProviderRequestFailed => Some(MetricName::ProviderFailures),
        DiagnosticCode::ToolDispatchStarted => Some(MetricName::ToolDispatches),
        DiagnosticCode::ToolDispatchFailed => Some(MetricName::ToolFailures),
        DiagnosticCode::GateStarted => Some(MetricName::GateExecutions),
        DiagnosticCode::GatePassed => Some(MetricName::GatePasses),
        DiagnosticCode::GateFailed | DiagnosticCode::GateBlocked => Some(MetricName::GateFailures),
        DiagnosticCode::BudgetReserved
        | DiagnosticCode::BudgetCharged
        | DiagnosticCode::BudgetExhausted => Some(MetricName::BudgetEvents),
        DiagnosticCode::RetryScheduled => Some(MetricName::Retries),
        DiagnosticCode::CancellationRequested | DiagnosticCode::CancellationObserved => {
            Some(MetricName::Cancellations)
        }
        DiagnosticCode::RecoveryStarted
        | DiagnosticCode::RecoveryCompleted
        | DiagnosticCode::RecoveryFailed => Some(MetricName::Recoveries),
        DiagnosticCode::ResourceObserved => Some(MetricName::ResourceObservations),
        DiagnosticCode::ExporterFailed => Some(MetricName::ExporterFailures),
        DiagnosticCode::BufferDropped => Some(MetricName::BufferDrops),
        DiagnosticCode::ShutdownStarted | DiagnosticCode::ShutdownCompleted => {
            Some(MetricName::Shutdowns)
        }
        _ => None,
    }
}
