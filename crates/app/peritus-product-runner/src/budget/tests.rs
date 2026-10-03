//! Aggregate budget accounting regressions.

use super::*;

const TEST_RUN_HORIZON: Duration = Duration::from_hours(8);

#[test]
fn rejected_model_request_preserves_prior_usage_and_every_counter() {
    let mut accounting = RunAccounting::direct_folder(Some(TEST_RUN_HORIZON)).unwrap();
    accounting.state.progress.model_requests = 7;
    accounting.state.progress.retries = u32::MAX;
    accounting.state.progress.input_tokens = 10;
    accounting.state.progress.output_tokens = 2;
    accounting.state.progress.total_tokens = 12;
    accounting.state.progress.usage_observations = 1;
    accounting.state.response_usage = UsageSnapshot {
        input_tokens: 10,
        output_tokens: 2,
        total_tokens: 12,
        observations: 1,
        ..UsageSnapshot::default()
    };
    let before_progress = accounting.state.progress;
    let before_usage = accounting.state.response_usage;

    assert!(
        accounting.record_event(DeveloperAccountingEvent::ModelRequest { retry: true }).is_err()
    );
    assert_eq!(accounting.state.progress, before_progress);
    assert_eq!(accounting.state.response_usage, before_usage);
}

#[test]
fn usage_replacement_rejection_preserves_totals_and_the_retained_response() {
    for observation_overflow in [false, true] {
        let mut accounting = RunAccounting::direct_folder(Some(TEST_RUN_HORIZON)).unwrap();
        accounting.state.progress.input_tokens = 10;
        accounting.state.progress.output_tokens = 20;
        if observation_overflow {
            accounting.state.progress.usage_observations = u32::MAX;
        } else {
            accounting.state.progress.provider_cost_microunits = u64::MAX;
        }
        let before = accounting.state;
        let mut usage = DeveloperUsage::default();
        usage
            .observe(peritus_model_protocol::UsageCounters::new(
                Some(2),
                Some(1),
                None,
                Some(3),
                None,
                None,
                Some(5),
                Some(1),
            ))
            .unwrap();
        let error = accounting.record_usage(usage).expect_err("late arithmetic failure");
        assert_eq!(accounting.state, before);
        assert_eq!(
            error.detail(),
            if observation_overflow {
                "run usage observation counter overflowed"
            } else {
                "run accounting counter overflowed"
            },
        );
    }
}

#[test]
fn replacement_subtracts_before_adding_and_rejects_missing_prior_contributions() {
    let mut accounting = RunAccounting::direct_folder(Some(TEST_RUN_HORIZON)).unwrap();
    let mut usage = DeveloperUsage::default();
    usage
        .observe(peritus_model_protocol::UsageCounters::new(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(u64::MAX),
        ))
        .unwrap();
    accounting.record_usage(usage).unwrap();
    let before = accounting.state;
    accounting.record_usage(usage).expect("same maximum contribution is representable");
    assert_eq!(accounting.state, before);

    accounting.state.progress.provider_cost_microunits = u64::MAX - 1;
    let before = accounting.state;
    assert!(accounting.record_usage(usage).is_err());
    assert_eq!(accounting.state, before);
}

#[test]
fn cumulative_work_and_cost_continue_past_former_product_ceilings() {
    let mut accounting = RunAccounting::direct_folder(Some(TEST_RUN_HORIZON)).unwrap();
    accounting.state.progress.model_requests = 4_096;
    accounting
        .record_event(DeveloperAccountingEvent::ModelRequest { retry: true })
        .expect("provider request remains observational");
    assert_eq!(accounting.latest_snapshot().model_requests(), 4_097);
    assert_eq!(accounting.latest_snapshot().retries(), 1);

    let mut accounting = RunAccounting::direct_folder(Some(TEST_RUN_HORIZON)).unwrap();
    let event = DeveloperAccountingEvent::Usage(peritus_model_protocol::UsageCounters::new(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(500_000_001),
    ));
    accounting.record_event(event).expect("cost remains observational");
    accounting.record_event(event).expect("replacement remains observational");
    assert_eq!(accounting.latest_snapshot().provider_cost_microunits(), 500_000_001);
    assert_eq!(accounting.latest_snapshot().usage_observations(), 1);
}

#[test]
fn later_explicit_total_replaces_the_derived_total_without_double_counting() {
    let mut accounting = RunAccounting::direct_folder(Some(TEST_RUN_HORIZON)).unwrap();
    let event = |total| {
        DeveloperAccountingEvent::Usage(peritus_model_protocol::UsageCounters::new(
            Some(10),
            None,
            None,
            Some(2),
            None,
            None,
            total,
            None,
        ))
    };
    accounting.record_event(event(None)).unwrap();
    assert_eq!(accounting.latest_snapshot().total_tokens(), 12);
    accounting.record_event(event(Some(11))).unwrap();
    accounting.record_event(event(Some(11))).unwrap();
    assert_eq!(accounting.latest_snapshot().total_tokens(), 11);
    assert_eq!(accounting.latest_snapshot().usage_observations(), 1);
}

#[test]
fn streaming_usage_replaces_each_response_snapshot_and_survives_failure() {
    let mut accounting = RunAccounting::direct_folder(Some(TEST_RUN_HORIZON)).unwrap();
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
        RunAccounting::new(temporary.path(), Some(TEST_RUN_HORIZON)).expect("accounting");
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
        RunAccounting::new(temporary.path(), Some(TEST_RUN_HORIZON)).expect("accounting");
    accounting.record_provider_failover().expect("record failover");
    let progress = accounting.snapshot().expect("bounded progress");
    assert_eq!(progress.provider_failovers(), 1);
    assert_eq!(progress.retries(), 0);
}

#[test]
fn successful_probe_closes_a_run_scoped_provider_circuit() {
    let temporary = tempfile::tempdir().expect("workspace");
    let mut accounting =
        RunAccounting::new(temporary.path(), Some(TEST_RUN_HORIZON)).expect("accounting");
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
        RunAccounting::new(temporary.path(), Some(TEST_RUN_HORIZON)).expect("accounting");
    std::fs::write(temporary.path().join("candidate.bin"), vec![0_u8; 4096]).expect("candidate");

    let progress = accounting.snapshot().expect("resource snapshot");

    assert_eq!(progress.workspace_growth_bytes(), 4096);
    assert_eq!(progress.workspace_bytes(), 4096);
    assert!(progress.peak_rss_bytes() > 0);
}

#[test]
fn cumulative_resource_observations_do_not_create_an_implicit_run_budget() {
    let progress = ProductRunProgress {
        model_requests: 4_097,
        tool_calls: 20_001,
        total_tokens: 100_000_001,
        provider_cost_microunits: 500_000_001,
        peak_rss_bytes: 12 * 1_024 * 1_024 * 1_024 + 1,
        workspace_growth_bytes: 50 * 1_024 * 1_024 * 1_024 + 1,
        ..ProductRunProgress::default()
    };
    let mut accounting = RunAccounting::direct_folder(None).expect("accounting");
    accounting.state.progress = progress;
    accounting
        .record_event(DeveloperAccountingEvent::ToolCall)
        .expect("effect admission accepts cumulative observations beyond the old ceilings");
    assert_eq!(accounting.latest_snapshot().tool_calls(), 20_002);
    assert_eq!(
        accounting.latest_snapshot().workspace_growth_bytes(),
        progress.workspace_growth_bytes()
    );
    accounting.check().expect("cumulative observations never exhaust a run");
    assert_eq!(accounting.latest_snapshot().model_requests(), 4_097);
    assert_eq!(accounting.latest_snapshot().provider_cost_microunits(), 500_000_001);
}

#[test]
fn any_positive_caller_run_horizon_is_accepted_and_drives_elapsed_budget() {
    assert!(validate_run_horizon(Some(Duration::ZERO)).is_err());
    assert!(validate_run_horizon(Some(Duration::from_hours(24))).is_ok());
    assert!(validate_run_horizon(Some(Duration::from_mins(1))).is_ok());
    assert_eq!(
        budget_violation(Duration::from_secs(61), Some(Duration::from_mins(1)),),
        Some("the configured run horizon was exhausted")
    );
}

#[test]
fn unbounded_run_has_no_elapsed_budget_violation() {
    assert!(validate_run_horizon(None).is_ok());
    assert_eq!(budget_violation(Duration::MAX, None), None);
    assert_eq!(RunAccounting::direct_folder(None).unwrap().remaining(), None);
}
