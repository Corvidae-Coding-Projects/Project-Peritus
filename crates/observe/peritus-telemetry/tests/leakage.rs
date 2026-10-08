//! Focused exporter-failure and complete error-chain leakage tests.

mod support;

use std::{error::Error, fmt::Write as _, num::NonZeroUsize};

use peritus_telemetry::{
    BufferConfig, ExportBatch, ExportPhase, ExportPoll, ExportPollControl, ExportProgress,
    ExportStreamId, Exporter, ExporterError, ExporterErrorCode, ExporterShutdownPoll,
    ObservationLossPolicy, TelemetryBuffer, TelemetryPump,
};

use support::metric_record;

const CANARY: &str = "token=C7-TELEMETRY-NEVER-PRINT-cf21";

#[test]
fn exporter_adapter_state_never_enters_errors_metrics_or_source_chains() {
    let config = BufferConfig::new(
        NonZeroUsize::new(512).expect("page bytes"),
        NonZeroUsize::new(2).expect("pages"),
        NonZeroUsize::new(512).expect("batch bytes"),
        ObservationLossPolicy::RejectNewest,
    )
    .expect("config");
    let buffer = TelemetryBuffer::new(config).expect("buffer");
    let mut pump = TelemetryPump::new(
        ExportStreamId::new([51; 16]).expect("stream"),
        buffer,
    )
    .expect("pump");
    pump.enqueue(metric_record(1)).expect("enqueue");
    let mut exporter = CanaryExporter { private_provider_detail: CANARY.to_owned() };
    let error = pump
        .poll_flush(
            &mut exporter,
            ExportPollControl::continue_with(NonZeroUsize::new(512).expect("poll bytes")),
        )
        .expect_err("export fails");
    let mut rendered = format!("{error:?}\n{error}");
    let mut source = error.source();
    while let Some(current) = source {
        write!(&mut rendered, "\n{current}").expect("format error chain");
        source = current.source();
    }
    assert!(!rendered.contains(CANARY));
    assert!(error.source().is_none());
    assert_eq!(error.exporter_code(), Some(ExporterErrorCode::Rejected));
    assert_eq!(error.exporter_retryable(), Some(false));
    assert_eq!(error.exporter_cleanup_complete(), Some(true));
    assert_eq!(pump.buffer().len(), 1);
}

struct CanaryExporter {
    private_provider_detail: String,
}

impl Exporter for CanaryExporter {
    fn begin_export(&mut self, _batch: &ExportBatch) -> Result<ExportProgress, ExporterError> {
        assert_eq!(self.private_provider_detail, CANARY);
        Err(ExporterError::new(ExporterErrorCode::Rejected, false))
    }

    fn poll_export(&mut self, _control: ExportPollControl) -> Result<ExportPoll, ExporterError> {
        Err(ExporterError::new(ExporterErrorCode::Protocol, false))
    }

    fn begin_shutdown(&mut self) -> Result<ExportProgress, ExporterError> {
        Ok(ExportProgress::new(ExportPhase::Cleaning, 0, None))
    }

    fn poll_shutdown(
        &mut self,
        _control: ExportPollControl,
    ) -> Result<ExporterShutdownPoll, ExporterError> {
        Ok(ExporterShutdownPoll::Complete(ExportProgress::new(
            ExportPhase::Complete,
            0,
            None,
        )))
    }
}
