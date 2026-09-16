//! The active writer and exact read-only view inspector do not share a mutable owner.

use super::{
    super::{LocalContextConfig, memory, record, storage},
    support::*,
};
use peritus_codec::sha256;
use peritus_model_protocol::{ProtocolLimits, encode_messages};
use serde_json::Value;

#[test]
fn inspection_keeps_exact_published_view_and_reports_unpublished_tail() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "inspect");
    observation(&mut memory, "read", "evidence", false);
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&view).unwrap();
    let inspect = || -> Value {
        serde_json::from_str(
            &storage::inspect(&fixture.state.path().join("memory"), binding()).unwrap(),
        )
        .unwrap()
    };
    let first = inspect();
    assert_eq!(first["uncovered_tail"], false);
    let archive = encode_messages(&view, ProtocolLimits::PRODUCTION).unwrap();
    assert_eq!(first["canonical_view_archive_hex"], super::super::tools::hex(&archive));
    let sequence = memory.store.sequence();
    assert_eq!(inspect(), first);
    assert_eq!(memory.store.sequence(), sequence);
    observation(&mut memory, "later", "not yet published", false);
    let later = inspect();
    assert_eq!(later["uncovered_tail"], true);
    assert_eq!(later["canonical_view_archive_hex"], first["canonical_view_archive_hex"]);
    assert!(storage::inspect(&fixture.state.path().join("missing"), binding()).is_err());
    assert!(!fixture.state.path().join("missing").exists());
}

#[test]
fn inspection_and_recovery_reject_the_same_corrupt_legacy_accounting() {
    let fixture = Fixture::new();
    let mut memory = fixture.open();
    begin(&mut memory, "legacy-inspection-corruption");
    observation(&mut memory, "read", "legacy evidence", false);
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    publish_legacy_v1(&mut memory, &view);

    let previous = memory.last_checkpoint.clone().unwrap();
    let previous_bytes = record::encode(&previous).unwrap();
    let mut validation: record::ViewValidation =
        record::decode(&memory.store.read(previous.validation).unwrap()).unwrap();
    validation.archive_bytes = validation.archive_bytes.checked_add(1).unwrap();
    let validation = memory.store.store(&record::encode(&validation).unwrap()).unwrap();
    let mut corrupt = previous;
    corrupt.generation = memory.store.generation().checked_add(1).unwrap();
    corrupt.through_event = memory.store.sequence();
    corrupt.previous = Some(sha256(&previous_bytes).into_bytes());
    corrupt.validation = validation;
    let corrupt_bytes = record::encode(&corrupt).unwrap();
    append_manifest(&mut memory, &corrupt, corrupt_bytes);

    let inspection = storage::inspect(&fixture.state.path().join("memory"), binding()).unwrap_err();
    assert!(
        inspection.to_string().contains("checkpoint validation does not bind its exact state"),
        "{inspection}"
    );
    drop(memory);

    let recovery = memory::LocalMemory::load(
        &fixture.state.path().join("memory"),
        fixture.workspace.path(),
        &fixture.state.path().join("run.trace"),
        binding(),
        LocalContextConfig::default(),
    );
    let Err(recovery) = recovery else {
        panic!("corrupt legacy checkpoint unexpectedly recovered")
    };
    assert!(
        recovery.to_string().contains("checkpoint validation does not bind its exact state"),
        "{recovery}"
    );
}
