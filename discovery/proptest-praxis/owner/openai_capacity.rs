//! Stateful capacity refresh properties for the exact selected Codex model.
use super::*;
use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits, ModelName,
    OutputLimitEnforcement, ProviderName, ResumeKind, StateMode, WireDialect,
};
use peritus_types::ProviderProfileId;
use proptest::prelude::*;

fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([0x6a; 16]).unwrap(),
        1,
        ProviderName::new("openai".to_owned()).unwrap(),
        ModelName::new("selected".to_owned()).unwrap(),
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

fn discovered(id: &str) -> DiscoveredModel {
    DiscoveredModel::new(id.to_owned(), id.to_owned()).unwrap()
}

fn valid(maximum: u64, percent: u8) -> DiscoveredModel {
    let mut model = discovered("selected");
    model.context_tokens = Some(maximum / 2);
    model.max_context_tokens = Some(maximum);
    model.effective_context_percent = Some(percent);
    model
}

#[test]
fn missing_metadata_uses_the_same_models_configured_envelope() {
    let cache = Cache::default();
    assert!(!cache.observe(profile().model(), vec![discovered("selected")]).unwrap());
    let prepared = cache.prepared(&profile(), 256).unwrap();
    assert_eq!(prepared.profile().model(), profile().model());
    assert_eq!(prepared.profile().limits().max_input_tokens(), 200_000);
}

#[test]
fn failed_refresh_retains_the_last_valid_exact_model_capacity() {
    let cache = Cache::default();
    assert!(cache.observe(profile().model(), vec![valid(872_000, 95)]).unwrap());
    let before = cache.prepared(&profile(), 256).unwrap();
    assert_eq!(before.profile().limits().max_input_tokens(), 828_144);

    let mut invalid = discovered("selected");
    invalid.max_context_tokens = Some(0);
    assert!(!cache.observe(profile().model(), vec![invalid]).unwrap());
    assert!(
        !cache
            .observe(profile().model(), vec![valid(1_400_000, 95), valid(2_000_000, 95)])
            .unwrap()
    );
    assert!(
        !cache.observe(profile().model(), vec![valid(2_000_000, 95).with_id("other")]).unwrap()
    );

    let after = cache.prepared(&profile(), 256).unwrap();
    assert_eq!(after.profile().model(), profile().model());
    assert_eq!(after.profile().limits().max_input_tokens(), 828_144);
}

trait WithId {
    fn with_id(self, id: &str) -> Self;
}

impl WithId for DiscoveredModel {
    fn with_id(mut self, id: &str) -> Self {
        self.id = ModelName::new(id.to_owned()).unwrap();
        self
    }
}

#[derive(Clone, Debug)]
enum Observation {
    Valid { maximum: u64, percent: u8 },
    Missing,
    Invalid,
    OtherModel { maximum: u64, percent: u8 },
    Empty,
}

fn observations() -> impl Strategy<Value = Vec<Observation>> {
    prop::collection::vec(
        prop_oneof![
            (10_000_u64..2_000_000, 50_u8..=100)
                .prop_map(|(maximum, percent)| { Observation::Valid { maximum, percent } }),
            Just(Observation::Missing),
            Just(Observation::Invalid),
            (10_000_u64..2_000_000, 50_u8..=100)
                .prop_map(|(maximum, percent)| { Observation::OtherModel { maximum, percent } }),
            Just(Observation::Empty),
        ],
        1..64,
    )
}

proptest! {
    #[test]
    fn refresh_histories_never_erase_or_cross_assign_the_last_valid_capacity(
        history in observations(),
    ) {
        let cache = Cache::default();
        let selected = profile();
        let mut expected = None;
        for observation in history {
            let (models, accepted) = match observation {
                Observation::Valid { maximum, percent } => {
                    expected = Some(maximum * u64::from(percent) / 100);
                    (vec![valid(maximum, percent)], true)
                }
                Observation::Missing => (vec![discovered("selected")], false),
                Observation::Invalid => {
                    let mut model = discovered("selected");
                    model.max_context_tokens = Some(0);
                    (vec![model], false)
                }
                Observation::OtherModel { maximum, percent } => {
                    (vec![valid(maximum, percent).with_id("other")], false)
                }
                Observation::Empty => (Vec::new(), false),
            };
            prop_assert_eq!(cache.observe(selected.model(), models).unwrap(), accepted);
            let observed = cache
                .selected(selected.model())
                .unwrap()
                .map(|model| model.usable_context_tokens().unwrap().unwrap());
            prop_assert_eq!(observed, expected);
        }
    }
}
