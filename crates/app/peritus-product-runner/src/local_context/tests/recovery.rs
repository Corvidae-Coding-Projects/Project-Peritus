//! Restart and trace/journal publication gaps never redispatch archived tool effects.

use super::{super::*, support::*};
use peritus_agent::{DeveloperToolObservation, DeveloperTrace, DeveloperTraceEvent};
use peritus_codec::sha256;
use peritus_model_protocol::{ProtocolLimits, decode_messages, encode_messages};
use serde_json::Value;

#[test]
fn a_trace_committed_before_memory_is_ingested_once_after_restart() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "first");
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&view).unwrap();
    let call = call("effect");
    proposal(&mut memory, &call);
    let observation = DeveloperToolObservation {
        output: canonical(&Value::from_iter([(
            "diagnostic",
            Value::from("middle-tail exact result"),
        )])),
        is_error: true,
    };
    let mut trace = crate::trace::FileDeveloperTrace::new(fixture.state.path().join("run.trace"))
        .with_memory_scope(memory.store.scope_digest(), memory.transcript.invocation);
    trace
        .record(DeveloperTraceEvent::ToolObservation { call: &call, observation: &observation })
        .unwrap();
    let prior = memory.state.through_observation();
    drop(memory);
    let mut reopened = fixture.open();
    assert_eq!(reopened.state.through_observation(), prior + 1);
    assert!(reopened.transcript.pending.is_empty());
    assert_eq!(reopened.artifact(prior + 1).unwrap(), observation.output.canonical_bytes());
    let proposal = update(
        reopened.model_revision,
        prior + 1,
        "cause",
        "The old approach failed for the preserved reason.",
    );
    assert!(
        tools::update::execute(&mut reopened, proposal.to_string().as_bytes())
            .unwrap()
            .get("rejected")
            .is_none()
    );
    let state = reopened.state.clone();
    drop(reopened);
    let mut reopened = fixture.open();
    assert_eq!(reopened.state, state);
    begin(&mut reopened, "different-provider");
    assert_eq!(reopened.transcript.invocation, 2);
    assert!(reopened.transcript.pending.is_empty());
    assert!(
        render(&reopened.prepare_view(&profile(32_768), &[]).unwrap()).contains("preserved reason")
    );
}

#[test]
fn reused_provider_call_ids_are_distinct_completed_episodes() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "one");
    let first = observation(&mut memory, "same-provider-call-id", "first", false);
    let second = observation(&mut memory, "same-provider-call-id", "second", false);
    assert_ne!(first, second);
    let state = memory.state.clone();
    drop(memory);
    assert_eq!(fixture.open().state, state);
}

#[test]
fn checkpoint_and_uncovered_pending_proposal_survive_without_claiming_execution() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "one");
    let id = observation(&mut memory, "read", "exact input", false);
    let proposal_json = update(memory.model_revision, id, "cause", "A preserved hypothesis");
    assert!(
        tools::update::execute(&mut memory, proposal_json.to_string().as_bytes())
            .unwrap()
            .get("rejected")
            .is_none()
    );
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&view).unwrap();
    proposal(&mut memory, &call("unfinished"));
    drop(memory);
    let mut memory = fixture.open();
    assert_eq!(memory.last_view, view);
    assert_eq!(memory.transcript.pending.len(), 1);
    begin(&mut memory, "two");
    assert_eq!(memory.transcript.pending[0].state, record::PendingState::Unknown);
    let next = memory.prepare_view(&profile(32_768), &[]).unwrap();
    assert!(render(&next).contains("DO NOT REDISPATCH"));
    assert!(
        next.iter()
            .flat_map(peritus_model_protocol::Message::content)
            .all(|block| !matches!(block, peritus_model_protocol::ContentBlock::ToolCall(_)))
    );
}

#[derive(Clone, Copy, Debug)]
enum SemanticCorruption {
    ProviderView,
    SelectedReplacement,
    SelectedDeletion,
    Profile,
    ProfileRevision,
    TokenEstimate,
}

#[test]
fn restart_rejects_semantically_changed_v2_views_with_valid_outer_hashes() {
    for corruption in [
        SemanticCorruption::ProviderView,
        SemanticCorruption::SelectedReplacement,
        SemanticCorruption::SelectedDeletion,
        SemanticCorruption::Profile,
        SemanticCorruption::ProfileRevision,
        SemanticCorruption::TokenEstimate,
    ] {
        let fixture = Fixture::new();
        let mut memory = fixture.open();
        begin(&mut memory, "validation-corruption");
        let unselected = observation(&mut memory, "read", "unselected exact output", false);
        let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
        let selected = &memory.prepared.as_ref().unwrap().validation.selected_observations;
        assert!(!selected.contains(&unselected), "fixture requires one valid unselected source");
        assert!(selected.len() >= 2, "fixture requires a deletable canonical selection");
        memory.publish(&view).unwrap();

        append_semantically_corrupt_v2_checkpoint(&mut memory, corruption);
        drop(memory);

        let reopened = memory::LocalMemory::load(
            &fixture.state.path().join("memory"),
            fixture.workspace.path(),
            &fixture.state.path().join("run.trace"),
            binding(),
            LocalContextConfig::default(),
        );
        let Err(error) = reopened else {
            panic!("{corruption:?} checkpoint unexpectedly recovered")
        };
        assert!(
            error.to_string().contains("checkpoint provider-view binding mismatch"),
            "{corruption:?}: {error}"
        );
    }
}

#[test]
fn publish_rejects_changed_prepared_profile_before_writing_a_checkpoint() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "publish-validation");
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    let generation = memory.store.generation();
    memory.prepared.as_mut().unwrap().validation.profile = [0x55; 16];
    let error = memory.publish(&view).unwrap_err();
    assert!(error.to_string().contains("stale or changed prepared view"));
    assert_eq!(memory.store.generation(), generation);
    assert!(memory.last_checkpoint.is_none());
}

#[test]
fn changed_render_and_operational_config_rebinds_the_next_view_without_bricking_recovery() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "configuration-change");
    let published = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&published).unwrap();
    drop(memory);

    let changed = LocalContextConfig {
        trigger_percent: 90,
        retain_recent_messages: 4,
        retrieved_evidence_max_tokens: 0,
        max_read_bytes: 8_192,
        checkpoint_every_completed_batch: false,
        ..LocalContextConfig::default()
    };
    let mut reopened = memory::LocalMemory::load(
        &fixture.state.path().join("memory"),
        fixture.workspace.path(),
        &fixture.state.path().join("run.trace"),
        binding(),
        changed.clone(),
    )
    .unwrap();
    assert_eq!(reopened.config, changed);
    assert_eq!(reopened.last_view, published);

    let rebound = reopened.prepare_view(&profile(32_768), &[]).unwrap();
    reopened.publish(&rebound).unwrap();
    drop(reopened);

    let recovered = memory::LocalMemory::load(
        &fixture.state.path().join("memory"),
        fixture.workspace.path(),
        &fixture.state.path().join("run.trace"),
        binding(),
        changed,
    )
    .unwrap();
    assert_eq!(recovered.last_view, rebound);
}

#[test]
fn authentic_v1_only_checkpoint_lineage_still_recovers() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "legacy-checkpoint-one");
    let first = memory.prepare_view(&profile(32_768), &[]).unwrap();
    publish_legacy_v1(&mut memory, &first);

    begin(&mut memory, "legacy-checkpoint-two");
    let second = memory.prepare_view(&profile(32_768), &[]).unwrap();
    publish_legacy_v1(&mut memory, &second);
    drop(memory);

    let reopened = fixture.open();
    assert_eq!(reopened.last_view, second);
    assert_eq!(
        reopened.last_checkpoint.as_ref().unwrap().schema_version,
        record::LEGACY_CHECKPOINT_SCHEMA_VERSION
    );
}

#[test]
fn restart_rejects_schema_v1_downgrade_after_schema_v2() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "current-checkpoint");
    let current = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&current).unwrap();

    let downgraded = memory.prepare_view(&profile(32_768), &[]).unwrap();
    publish_legacy_v1(&mut memory, &downgraded);
    let inspection = storage::inspect(&fixture.state.path().join("memory"), binding()).unwrap_err();
    assert!(inspection.to_string().contains("checkpoint schema downgrade"), "{inspection}");
    drop(memory);

    let reopened = memory::LocalMemory::load(
        &fixture.state.path().join("memory"),
        fixture.workspace.path(),
        &fixture.state.path().join("run.trace"),
        binding(),
        LocalContextConfig::default(),
    );
    let Err(error) = reopened else { panic!("schema downgrade unexpectedly recovered") };
    assert!(error.to_string().contains("checkpoint schema downgrade"), "{error}");
}

fn append_semantically_corrupt_v2_checkpoint(
    memory: &mut memory::LocalMemory,
    corruption: SemanticCorruption,
) {
    let previous = memory.last_checkpoint.clone().unwrap();
    let previous_bytes = record::encode(&previous).unwrap();
    let mut validation: record::ViewValidation =
        record::decode(&memory.store.read(previous.validation).unwrap()).unwrap();
    let mut view_bytes = memory.store.read(previous.view).unwrap();
    let mut successor = previous;
    successor.previous = Some(sha256(&previous_bytes).into_bytes());
    successor.generation = memory.store.generation() + 1;
    successor.view_binding = Some(
        view_binding::checkpoint(
            successor.scope,
            successor.generation,
            successor.through_event,
            successor.render_policy,
            &view_bytes,
            &validation,
        )
        .unwrap(),
    );

    match corruption {
        SemanticCorruption::ProviderView => {
            let mut messages = decode_messages(&view_bytes, ProtocolLimits::PRODUCTION).unwrap();
            messages.pop().expect("fixture has a removable provider-view message");
            view_bytes = encode_messages(&messages, ProtocolLimits::PRODUCTION).unwrap();
        }
        SemanticCorruption::SelectedReplacement => {
            let replacement = (1..=memory.sources.len() as u64)
                .find(|source| !validation.selected_observations.contains(source))
                .expect("fixture has one valid unselected source");
            let insertion = validation
                .selected_observations
                .binary_search(&replacement)
                .expect_err("replacement is not already selected");
            let replaced = insertion.saturating_sub(1).min(
                validation
                    .selected_observations
                    .len()
                    .checked_sub(1)
                    .expect("fixture has selected sources"),
            );
            validation.selected_observations[replaced] = replacement;
            assert!(
                validation.selected_observations.windows(2).all(|pair| pair[0] < pair[1]),
                "replacement must remain canonical"
            );
        }
        SemanticCorruption::SelectedDeletion => {
            validation.selected_observations.remove(0);
        }
        SemanticCorruption::Profile => validation.profile = [0x44; 16],
        SemanticCorruption::ProfileRevision => {
            validation.profile_revision = validation.profile_revision.checked_add(1).unwrap();
        }
        SemanticCorruption::TokenEstimate => {
            validation.estimated_input_tokens =
                if validation.estimated_input_tokens < validation.max_input_tokens {
                    validation.estimated_input_tokens + 1
                } else {
                    validation.estimated_input_tokens - 1
                };
            validation.input_tokens_saved = validation
                .uncompacted_input_tokens
                .saturating_sub(validation.estimated_input_tokens);
        }
    }

    successor.view = memory.store.store(&view_bytes).unwrap();
    successor.validation = memory.store.store(&record::encode(&validation).unwrap()).unwrap();
    let successor_bytes = record::encode(&successor).unwrap();
    append_manifest(memory, &successor, successor_bytes);
}
