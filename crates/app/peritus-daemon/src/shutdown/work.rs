//! Exact aggregate registry counts and bounded redaction-safe summaries.

use std::num::NonZeroUsize;

use peritus_app_protocol::{AppProtocolLimits, RemainingWork, RemainingWorkKind};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

const WORK_CATEGORY_COUNT: usize = 8;
const LONGEST_DESCRIPTOR_LABEL: &str = "indeterminate-effects=";
const REQUIRED_DESCRIPTOR_BYTES: usize =
    LONGEST_DESCRIPTOR_LABEL.len() + decimal_digits(usize::MAX);

/// Bounds that always admit all exact aggregate shutdown summaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownBounds {
    maximum_remaining_work: NonZeroUsize,
    maximum_descriptor_bytes: NonZeroUsize,
}

impl ShutdownBounds {
    /// Creates bounds large enough for every category and any platform `usize` count.
    ///
    /// # Errors
    ///
    /// Rejects zero or a bound too small to preserve exactness in the worst case.
    pub(crate) fn new(
        maximum_remaining_work: usize,
        maximum_descriptor_bytes: usize,
    ) -> Result<Self, DaemonError> {
        let maximum_remaining_work =
            NonZeroUsize::new(maximum_remaining_work).ok_or_else(invalid_bounds)?;
        let maximum_descriptor_bytes =
            NonZeroUsize::new(maximum_descriptor_bytes).ok_or_else(invalid_bounds)?;
        let bounds = Self { maximum_remaining_work, maximum_descriptor_bytes };
        bounds.validate()?;
        Ok(bounds)
    }

    /// Derives coordinator bounds from the negotiated A3 protocol limits.
    ///
    /// # Errors
    ///
    /// Rejects negotiated limits too small for exact aggregate shutdown truth.
    pub(crate) fn from_protocol(limits: AppProtocolLimits) -> Result<Self, DaemonError> {
        Self::new(limits.max_remaining_work_items(), limits.max_diagnostic_bytes())
    }

    pub(super) fn validate(self) -> Result<(), DaemonError> {
        if self.maximum_remaining_work.get() < WORK_CATEGORY_COUNT
            || self.maximum_descriptor_bytes.get() < REQUIRED_DESCRIPTOR_BYTES
        {
            return Err(invalid_bounds());
        }
        Ok(())
    }

    pub(super) const fn maximum_remaining_work(self) -> usize {
        self.maximum_remaining_work.get()
    }

    const fn maximum_descriptor_bytes(self) -> usize {
        self.maximum_descriptor_bytes.get()
    }
}

/// Point-in-time counts derived from the daemon's effect-owning registries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShutdownWorkCounts {
    requests: usize,
    subscriptions: usize,
    artifact_transfers: usize,
    terminal_attachments: usize,
    workers: usize,
    processes: usize,
    outbox: usize,
    indeterminate_effects: usize,
}

impl ShutdownWorkCounts {
    /// Creates an all-zero registry snapshot.
    #[must_use]
    pub(crate) const fn empty() -> Self {
        Self {
            requests: 0,
            subscriptions: 0,
            artifact_transfers: 0,
            terminal_attachments: 0,
            workers: 0,
            processes: 0,
            outbox: 0,
            indeterminate_effects: 0,
        }
    }

    /// Sets the exact active application-request count.
    #[must_use]
    pub(crate) const fn with_requests(mut self, count: usize) -> Self {
        self.requests = count;
        self
    }

    /// Sets the exact active subscription count.
    #[must_use]
    pub(crate) const fn with_subscriptions(mut self, count: usize) -> Self {
        self.subscriptions = count;
        self
    }

    /// Sets the exact active artifact-transfer count.
    #[must_use]
    pub(crate) const fn with_artifact_transfers(mut self, count: usize) -> Self {
        self.artifact_transfers = count;
        self
    }

    /// Sets the exact active terminal-attachment count.
    #[must_use]
    pub(crate) const fn with_terminal_attachments(mut self, count: usize) -> Self {
        self.terminal_attachments = count;
        self
    }

    /// Sets the exact supervised-worker count.
    #[must_use]
    pub(crate) const fn with_workers(mut self, count: usize) -> Self {
        self.workers = count;
        self
    }

    /// Sets the exact owned-process count.
    #[must_use]
    pub(crate) const fn with_processes(mut self, count: usize) -> Self {
        self.processes = count;
        self
    }

    /// Sets the exact unsettled durable-outbox count.
    #[must_use]
    pub(crate) const fn with_outbox(mut self, count: usize) -> Self {
        self.outbox = count;
        self
    }

    /// Sets the exact count of effects whose terminal status remains indeterminate.
    #[must_use]
    pub(crate) const fn with_indeterminate_effects(mut self, count: usize) -> Self {
        self.indeterminate_effects = count;
        self
    }

    /// Returns true only when every externally relevant registry count is zero.
    #[must_use]
    pub(crate) const fn is_empty(self) -> bool {
        self.requests == 0
            && self.subscriptions == 0
            && self.artifact_transfers == 0
            && self.terminal_attachments == 0
            && self.workers == 0
            && self.processes == 0
            && self.outbox == 0
            && self.indeterminate_effects == 0
    }
}

pub(super) fn summarize(
    counts: ShutdownWorkCounts,
    bounds: ShutdownBounds,
) -> Result<Vec<RemainingWork>, DaemonError> {
    let mut remaining = Vec::with_capacity(WORK_CATEGORY_COUNT);
    append(&mut remaining, RemainingWorkKind::Request, "requests=", counts.requests, bounds)?;
    append(
        &mut remaining,
        RemainingWorkKind::Subscription,
        "subscriptions=",
        counts.subscriptions,
        bounds,
    )?;
    append(
        &mut remaining,
        RemainingWorkKind::ArtifactTransfer,
        "artifact-transfers=",
        counts.artifact_transfers,
        bounds,
    )?;
    append(
        &mut remaining,
        RemainingWorkKind::TerminalAttachment,
        "terminal-attachments=",
        counts.terminal_attachments,
        bounds,
    )?;
    for (label, count) in [
        ("workers=", counts.workers),
        ("processes=", counts.processes),
        ("outbox=", counts.outbox),
        ("indeterminate-effects=", counts.indeterminate_effects),
    ] {
        append(&mut remaining, RemainingWorkKind::Other, label, count, bounds)?;
    }
    Ok(remaining)
}

fn append(
    remaining: &mut Vec<RemainingWork>,
    kind: RemainingWorkKind,
    label: &'static str,
    count: usize,
    bounds: ShutdownBounds,
) -> Result<(), DaemonError> {
    if count == 0 {
        return Ok(());
    }
    let descriptor = format!("{label}{count}");
    let work = RemainingWork::new(kind, descriptor, bounds.maximum_descriptor_bytes()).map_err(
        |error| {
            DaemonError::with_source(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "summarize daemon shutdown work",
                "validated shutdown bounds rejected an exact aggregate descriptor",
                error,
            )
        },
    )?;
    remaining.push(work);
    Ok(())
}

const fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn invalid_bounds() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "configure daemon shutdown",
        "shutdown bounds cannot encode every exact aggregate work category",
    )
}
