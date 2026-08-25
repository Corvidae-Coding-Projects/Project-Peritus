# peritus-telemetry

`peritus-telemetry` derives stable metrics and OpenTelemetry-compatible records from checked C7
trace projections. It owns deterministic bounded queues, explicit exporter acknowledgement,
shutdown flushing, durable V2 contiguous final-disposition checkpoints, and restart recovery. V2
requires exact `exported + dropped == submitted == disposed_through` accounting; V1 checkpoints are
unsupported. The family-60 observation protocol remains independently versioned at schema 1.

The crate sees only redaction-safe C7 values. It cannot run work or mutate Peritus authority.
See `docs/c7-trace-telemetry.md` for operational integration guidance.
