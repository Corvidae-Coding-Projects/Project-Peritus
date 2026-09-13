//! Aggregate budget accounting regressions.

use super::*;

#[test]
fn streaming_usage_replaces_each_response_snapshot_and_survives_failure() {
    let mut accounting = RunAccounting::direct_folder(PRODUCT_RUN_MAX_ELAPSED).unwrap();
    let usage = |output| {
        DeveloperAccountingEvent::Usage(peritus_model_protocol::UsageCounters::new(
            Some(10),
            Some(3),
            None,
            Some(output),
            Some(1),
            None,
            None,
            None,
        ))
    };
    accounting.record_event(DeveloperAccountingEvent::ModelRequest { retry: false }).unwrap();
    accounting.record_event(usage(2)).unwrap();
    accounting.record_event(usage(4)).unwrap();
    accounting.record_event(usage(4)).unwrap();
    accounting.record_role_retry().unwrap();
    accounting.record_event(DeveloperAccountingEvent::ModelRequest { retry: false }).unwrap();
    accounting.record_event(usage(2)).unwrap();
    let progress = accounting.latest_snapshot();
    assert_eq!(progress.model_requests(), 2);
    assert_eq!(progress.retries(), 1);
    assert_eq!(progress.input_tokens(), 20);
    assert_eq!(progress.output_tokens(), 6);
    assert_eq!(progress.total_tokens(), 26);
    assert_eq!(progress.cached_input_tokens(), 6);
    assert_eq!(progress.usage_observations(), 2);
}

#[test]
fn work_events_accumulate_without_a_successful_role_outcome() {
    let temporary = tempfile::tempdir().expect("workspace");
    let mut accounting =
        RunAccounting::new(temporary.path(), PRODUCT_RUN_MAX_ELAPSED).expect("accounting");
    for index in 0..51 {
        accounting
            .record_event(DeveloperAccountingEvent::ModelRequest { retry: index < 3 })
            .unwrap();
    }
    for _ in 0..512 {
        accounting.record_event(DeveloperAccountingEvent::ToolCall).unwrap();
    }
    for _ in 0..2 {
        accounting.record_event(DeveloperAccountingEvent::Compaction).unwrap();
    }
    let progress = accounting.snapshot().expect("bounded progress");

    assert_eq!(progress.model_requests(), 51);
    assert_eq!(progress.tool_calls(), 512);
    assert_eq!(progress.retries(), 3);
    assert_eq!(progress.compactions(), 2);
}

#[test]
fn provider_failovers_are_counted_separately_from_same_provider_retries() {
    let temporary = tempfile::tempdir().expect("workspace");
    let mut accounting =
        RunAccounting::new(temporary.path(), PRODUCT_RUN_MAX_ELAPSED).expect("accounting");
    accounting.record_provider_failover().expect("record failover");
    let progress = accounting.snapshot().expect("bounded progress");
    assert_eq!(progress.provider_failovers(), 1);
    assert_eq!(progress.retries(), 0);
}

#[test]
fn successful_probe_closes_a_run_scoped_provider_circuit() {
    let temporary = tempfile::tempdir().expect("workspace");
    let mut accounting =
        RunAccounting::new(temporary.path(), PRODUCT_RUN_MAX_ELAPSED).expect("accounting");
    let profile = ProviderProfileId::new([0x51; 16]).expect("profile ID");

    accounting.open_provider_circuit(profile);
    assert!(accounting.provider_circuit_open(profile));
    accounting.close_provider_circuit(profile);
    assert!(!accounting.provider_circuit_open(profile));
}

#[test]
fn workspace_growth_and_peak_memory_are_observed_at_effect_boundaries() {
    let temporary = tempfile::tempdir().expect("workspace");
    let mut accounting =
        RunAccounting::new(temporary.path(), PRODUCT_RUN_MAX_ELAPSED).expect("accounting");
    std::fs::write(temporary.path().join("candidate.bin"), vec![0_u8; 4096]).expect("candidate");

    let progress = accounting.snapshot().expect("resource snapshot");

    assert_eq!(progress.workspace_growth_bytes(), 4096);
    assert_eq!(progress.workspace_bytes(), 4096);
    assert!(progress.peak_rss_bytes() > 0);
}

#[test]
fn memory_and_workspace_growth_have_distinct_hard_failures() {
    let memory = ProductRunProgress {
        peak_rss_bytes: PRODUCT_RUN_MAX_PEAK_RSS_BYTES + 1,
        ..ProductRunProgress::default()
    };
    assert_eq!(
        budget_violation(memory, Duration::ZERO, PRODUCT_RUN_MAX_ELAPSED),
        Some("the product-run peak resident-memory budget was exhausted")
    );

    let workspace = ProductRunProgress {
        workspace_growth_bytes: PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES + 1,
        ..ProductRunProgress::default()
    };
    assert_eq!(
        budget_violation(workspace, Duration::ZERO, PRODUCT_RUN_MAX_ELAPSED),
        Some("the product-run workspace-growth budget was exhausted")
    );
}

#[test]
fn caller_run_horizon_is_bounded_and_drives_elapsed_budget() {
    assert!(validate_run_horizon(Duration::ZERO).is_err());
    assert!(validate_run_horizon(PRODUCT_RUN_MAX_ELAPSED + Duration::from_secs(1)).is_err());
    assert!(validate_run_horizon(Duration::from_mins(1)).is_ok());
    assert_eq!(
        budget_violation(
            ProductRunProgress::default(),
            Duration::from_secs(61),
            Duration::from_mins(1),
        ),
        Some("the configured run horizon was exhausted")
    );
}
