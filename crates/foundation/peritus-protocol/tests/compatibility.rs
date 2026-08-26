//! Generated protocol artifacts and compatibility-corpus reproducibility tests.

use peritus_codec::{CodecLimits, decode_message};
use peritus_protocol::schema::{
    FAMILIES, KERNEL_COMMAND_VARIANTS, KERNEL_ERROR_VARIANTS, KERNEL_EVENT_VARIANTS,
    KERNEL_SUBJECT_VARIANTS, LIFECYCLE_PHASE_VARIANTS, LIFECYCLE_VARIANTS,
    generated_agent_binary_artifacts, generated_artifacts, generated_binary_artifacts,
};
use peritus_protocol::{
    AcceptanceContractDto, ActionIntentDto, AgentCommandDto, AgentEventDto, AgentStateDto,
    BudgetAmountsDto, CommandEnvelopeDto, KernelCommandDto, PolicyAmendmentDto,
};
use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"))
        .join("../../..")
}

#[test]
fn checked_in_agent_frames_are_exact_and_decodable() {
    let root = repository_root();
    let artifacts = generated_agent_binary_artifacts().expect("generate agent corpus");
    for artifact in &artifacts {
        let actual = fs::read(root.join(artifact.path)).expect("agent compatibility fixture");
        assert_eq!(actual, artifact.content);
    }
    let read = |name: &str| {
        fs::read(root.join("crates/foundation/peritus-protocol/tests/fixtures/v1").join(name))
            .expect("agent fixture")
    };
    let limits = CodecLimits::PRODUCTION;
    decode_message::<AgentCommandDto>(&read("agent-command.bin"), limits).expect("command");
    decode_message::<AgentEventDto>(&read("agent-event.bin"), limits).expect("event");
    decode_message::<AgentStateDto>(&read("agent-state.bin"), limits).expect("state");
}

#[test]
fn family_registry_is_nonzero_unique_and_strictly_ordered() {
    assert!(!FAMILIES.is_empty());
    for (index, family) in FAMILIES.iter().enumerate() {
        assert_ne!(family.tag, 0);
        assert_ne!(family.schema_version, 0);
        assert!(!family.name.is_empty());
        if index > 0 {
            assert!(FAMILIES[index - 1].tag < family.tag);
        }
    }
}

#[test]
fn f0_family_allocations_are_frozen() {
    let f0 = FAMILIES
        .iter()
        .filter(|family| (88..=93).contains(&family.tag))
        .map(|family| (family.tag, family.name, family.schema_version, family.inert_only))
        .collect::<Vec<_>>();
    assert_eq!(
        f0,
        vec![
            (88, "evolution-campaign-command", 1, true),
            (89, "evolution-campaign-event", 1, true),
            (90, "evolution-campaign-state", 1, true),
            (91, "production-harness-command", 1, true),
            (92, "production-harness-event", 1, true),
            (93, "production-harness-state", 1, true),
        ]
    );
}

#[test]
fn lifecycle_registry_is_complete_unique_and_nonzero() {
    assert_eq!(LIFECYCLE_VARIANTS.len(), 5);
    assert_eq!(KERNEL_COMMAND_VARIANTS.len(), 35);
    assert_eq!(KERNEL_EVENT_VARIANTS.len(), 37);
    assert_eq!(KERNEL_ERROR_VARIANTS.len(), 16);
    assert_eq!(KERNEL_SUBJECT_VARIANTS.len(), 8);
    assert_eq!(LIFECYCLE_PHASE_VARIANTS.len(), 44);
    for set in LIFECYCLE_VARIANTS {
        let mut discriminants = std::collections::BTreeSet::new();
        let mut names = std::collections::BTreeSet::new();
        for variant in set.variants {
            assert_ne!(variant.tag, 0);
            assert!(variant.subtag.is_none_or(|value| value != 0));
            assert!(discriminants.insert((variant.tag, variant.subtag)));
            assert!(names.insert(variant.name));
        }
    }
}

#[test]
fn checked_in_client_artifacts_are_exact_generator_output() {
    let root = repository_root();
    for artifact in generated_artifacts() {
        let actual = fs::read_to_string(root.join(artifact.path)).expect("generated text artifact");
        assert_eq!(actual, artifact.content);
    }
}

#[test]
fn checked_in_compatibility_frames_are_exact_and_decodable() {
    let root = repository_root();
    let artifacts = generated_binary_artifacts().expect("generate corpus");
    for artifact in &artifacts {
        let actual = fs::read(root.join(artifact.path)).expect("compatibility fixture");
        assert_eq!(actual, artifact.content);
    }

    let read =
        |name: &str| fs::read(root.join("protocol/fixtures/v1").join(name)).expect("fixture");
    let limits = CodecLimits::PRODUCTION;
    decode_message::<KernelCommandDto>(&read("kernel-command-pause-session.bin"), limits)
        .expect("command");
    decode_message::<CommandEnvelopeDto>(&read("command-envelope.bin"), limits).expect("envelope");
    decode_message::<BudgetAmountsDto>(&read("budget-amounts.bin"), limits).expect("budget");
    decode_message::<ActionIntentDto>(&read("action-intent.bin"), limits).expect("action");
    decode_message::<PolicyAmendmentDto>(&read("policy-amendment.bin"), limits).expect("amendment");
    decode_message::<AcceptanceContractDto>(&read("acceptance-contract.bin"), limits)
        .expect("contract");
}
