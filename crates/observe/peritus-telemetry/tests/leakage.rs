//! Focused exporter-failure and complete error-chain leakage tests.

mod support;

use std::{error::Error, fmt::Write as _, num::NonZeroUsize};

use peritus_telemetry::{
    BackpressurePolicy, BufferConfig, ExportAck, ExportBatch, ExportStreamId, Exporter,
    ExporterError, ExporterErrorCode, TelemetryBuffer, TelemetryPump,
};

use support::metric_record;

const CANARY: &str = "token=C7-TELEMETRY-NEVER-PRINT-cf21";

#[test]
fn exporter_adapter_state_never_enters_errors_metrics_or_source_chains() {
    let config = BufferConfig::new(
        NonZeroUsize::new(2).expect("capacity"),
        NonZeroUsize::new(1).expect("batch"),
        BackpressurePolicy::RejectNewest,
    )
    .expect("config");
    let mut pump = TelemetryPump::new(
        ExportStreamId::new([51; 16]).expect("stream"),
        TelemetryBuffer::new(config),
    );
    pump.buffer_mut().enqueue(metric_record(1)).expect("enqueue");
    let mut exporter = CanaryExporter { private_provider_detail: CANARY.to_owned() };
    let error = pump.flush_one(&mut exporter).expect_err("export fails");
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
    assert_eq!(pump.buffer().len(), 1);
}

struct CanaryExporter {
    private_provider_detail: String,
}

impl Exporter for CanaryExporter {
    fn export(&mut self, _batch: &ExportBatch) -> Result<ExportAck, ExporterError> {
        assert_eq!(self.private_provider_detail, CANARY);
        Err(ExporterError::new(ExporterErrorCode::Rejected, false))
    }

    fn shutdown(&mut self) -> Result<(), ExporterError> {
        Ok(())
    }
}
