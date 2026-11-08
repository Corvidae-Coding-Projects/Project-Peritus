//! Capacity contracts, using owned runtime files and independent numeric expectations.
use super::*;
use peritus_model_protocol::*;
use peritus_types::ProviderProfileId;
use serde_json::json;
use std::{fs, time::SystemTime};

fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([19; 16]).unwrap(),
        1,
        ProviderName::new("openai".into()).unwrap(),
        ModelName::new("selected".into()).unwrap(),
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).unwrap(),
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 128, 8, 1).unwrap(),
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .unwrap()
}
fn selected() -> Vec<DiscoveredModel> {
    vec![DiscoveredModel::new("selected".into(), "Selected".into()).unwrap()]
}
fn row() -> Value {
    json!({"slug":"selected","context_window":500_000,"max_context_window":872_000,"effective_context_window_percent":95})
}

#[test]
fn maximum_window_and_reply_space_are_independent_of_bootstrap_limits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models_cache.json");
    fs::write(&path, json!({"models":[row()]}).to_string()).unwrap();
    let model = enrich_file(selected(), &path).unwrap().remove(0);
    assert_eq!(model.usable_context_tokens().unwrap(), Some(828_400));
    let p = profile();
    for (reply, input) in [(256, 828_144), (32_000, 796_400), (u64::MAX, 796_400)] {
        let effective = crate::catalog::profile_with_model_limits(&p, &model, reply).unwrap();
        assert_eq!(effective.model(), p.model());
        assert_eq!(effective.profile_id(), p.profile_id());
        assert_eq!(effective.limits().max_input_tokens(), input);
    }
    assert_eq!(
        crate::catalog::output_tokens_for_input(&p, &model, 828_000, u64::MAX).unwrap(),
        400
    );
    assert!(crate::catalog::output_tokens_for_input(&p, &model, 828_400, u64::MAX).is_err());
}

#[test]
fn cache_never_adds_models_and_expired_or_absent_metadata_is_unknown() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models_cache.json");
    assert_eq!(enrich_file(selected(), &path).unwrap()[0].usable_context_tokens().unwrap(), None);
    fs::write(&path, json!({"models":[{"slug":"other","context_window":999_999}]}).to_string())
        .unwrap();
    let models = enrich_file(selected(), &path).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].usable_context_tokens().unwrap(), None);
    fs::write(&path, json!({"models":[row()]}).to_string()).unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_mins(10)))
        .unwrap();
    assert_eq!(enrich_file(selected(), &path).unwrap()[0].usable_context_tokens().unwrap(), None);
}

#[test]
fn contradictory_duplicate_and_malformed_runtime_metadata_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models_cache.json");
    for bad in [
        json!({"models":[row(),row()]}),
        json!({"models":[{"slug":"selected","context_window":500_000,"max_context_window":100}]}),
        json!({"models":[{"slug":"selected","context_window":0}]}),
        json!({"models":[{"slug":"selected","context_window":800,"effective_context_window_percent":101}]}),
        json!({"models":[{"slug":"selected","context_window":u64::MAX,"effective_context_window_percent":100}]}),
    ] {
        fs::write(&path, bad.to_string()).unwrap();
        assert!(enrich_file(selected(), &path).is_err(), "{bad}");
    }
    fs::write(&path, "not-json").unwrap();
    assert!(enrich_file(selected(), &path).is_err());
}
