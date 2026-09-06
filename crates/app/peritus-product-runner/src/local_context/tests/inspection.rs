//! The active writer and exact read-only view inspector do not share a mutable owner.

use super::{super::storage, support::*};
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
