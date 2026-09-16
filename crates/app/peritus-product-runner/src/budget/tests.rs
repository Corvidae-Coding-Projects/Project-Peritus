//! Aggregate budget accounting regressions.

use super::*;

#[test]
fn rejected_model_request_preserves_prior_usage_and_every_counter() {
    let mut accounting = RunAccounting::direct_folder(PRODUCT_RUN_MAX_ELAPSED).unwrap();
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
        let mut accounting = RunAccounting::direct_folder(PRODUCT_RUN_MAX_ELAPSED).unwrap();
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
    let mut accounting = RunAccounting::direct_folder(PRODUCT_RUN_MAX_ELAPSED).unwrap();
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
fn completed_work_is_retained_when_a_hard_ceiling_rejects_the_next_boundary() {
    let mut accounting = RunAccounting::direct_folder(PRODUCT_RUN_MAX_ELAPSED).unwrap();
    accounting.state.progress.model_requests = PRODUCT_RUN_MAX_MODEL_REQUESTS;
    let error = accounting
        .record_event(DeveloperAccountingEvent::ModelRequest { retry: true })
        .expect_err("provider request ceiling");
    assert_eq!(error.detail(), "the cumulative provider-request budget was exhausted");
    assert_eq!(accounting.latest_snapshot().model_requests(), PRODUCT_RUN_MAX_MODEL_REQUESTS + 1);
    assert_eq!(accounting.latest_snapshot().retries(), 1);

    let mut accounting = RunAccounting::direct_folder(PRODUCT_RUN_MAX_ELAPSED).unwrap();
    let event = DeveloperAccountingEvent::Usage(peritus_model_protocol::UsageCounters::new(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(PRODUCT_RUN_MAX_COST_MICROUNITS + 1),
    ));
    assert!(accounting.record_event(event).is_err());
    assert!(accounting.record_event(event).is_err());
    assert_eq!(
        accounting.latest_snapshot().provider_cost_microunits(),
        PRODUCT_RUN_MAX_COST_MICROUNITS + 1,
    );
    assert_eq!(accounting.latest_snapshot().usage_observations(), 1);
}

#[test]
fn later_explicit_total_replaces_the_derived_total_without_double_counting() {
    let mut accounting = RunAccounting::direct_folder(PRODUCT_RUN_MAX_ELAPSED).unwrap();
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
fn exact_numeric_ceilings_are_admitted_and_rejections_keep_their_priority() {
    let mut progress = ProductRunProgress {
        model_requests: PRODUCT_RUN_MAX_MODEL_REQUESTS,
        tool_calls: PRODUCT_RUN_MAX_TOOL_CALLS,
        total_tokens: PRODUCT_RUN_MAX_TOTAL_TOKENS,
        provider_cost_microunits: PRODUCT_RUN_MAX_COST_MICROUNITS,
        peak_rss_bytes: PRODUCT_RUN_MAX_PEAK_RSS_BYTES,
        workspace_growth_bytes: PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES,
        ..ProductRunProgress::default()
    };
    assert_eq!(budget_violation(progress, PRODUCT_RUN_MAX_ELAPSED, PRODUCT_RUN_MAX_ELAPSED), None);
    progress.model_requests += 1;
    progress.tool_calls += 1;
    progress.total_tokens += 1;
    progress.provider_cost_microunits += 1;
    progress.peak_rss_bytes += 1;
    progress.workspace_growth_bytes += 1;
    assert_eq!(
        budget_violation(
            progress,
            PRODUCT_RUN_MAX_ELAPSED + Duration::from_nanos(1),
            PRODUCT_RUN_MAX_ELAPSED
        ),
        Some("the configured run horizon was exhausted"),
    );
    let first = |progress: ProductRunProgress| {
        budget_violation(progress, Duration::ZERO, PRODUCT_RUN_MAX_ELAPSED)
    };
    assert_eq!(first(progress), Some("the cumulative provider-request budget was exhausted"));
    progress.model_requests -= 1;
    assert_eq!(first(progress), Some("the cumulative application-tool budget was exhausted"));
    progress.tool_calls -= 1;
    assert_eq!(first(progress), Some("the cumulative model-token budget was exhausted"));
    progress.total_tokens -= 1;
    assert_eq!(
        first(progress),
        Some("the cumulative provider-estimated cost budget was exhausted")
    );
    progress.provider_cost_microunits -= 1;
    assert_eq!(first(progress), Some("the product-run peak resident-memory budget was exhausted"));
    progress.peak_rss_bytes -= 1;
    assert_eq!(first(progress), Some("the product-run workspace-growth budget was exhausted"));
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
