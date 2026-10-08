//! Preserve exact history nodes while removing synthetic logical state capacities.

pub(super) const SQL: &str = r"CREATE TABLE state_history_nodes_v3 (
    node_digest BLOB PRIMARY KEY CHECK (length(node_digest) = 32),
    level INTEGER NOT NULL CHECK (level BETWEEN 0 AND 255),
    byte_length INTEGER NOT NULL CHECK (byte_length >= 0),
    payload BLOB NOT NULL CHECK (length(payload) <= 512)
) STRICT, WITHOUT ROWID;
INSERT INTO state_history_nodes_v3 SELECT * FROM state_history_nodes;
DROP TABLE state_history_nodes;
ALTER TABLE state_history_nodes_v3 RENAME TO state_history_nodes;
UPDATE store_meta SET schema_version = 3 WHERE singleton = 1;
PRAGMA user_version = 3;
";
pub(super) const DIGEST: [u8; 32] = [
    0xb7, 0x13, 0xa2, 0xce, 0x4b, 0x79, 0x54, 0xf9, 0x6a, 0xf5, 0x45, 0xd8, 0xf5, 0xb6, 0x24, 0x0d,
    0x54, 0x52, 0x2a, 0x6e, 0xeb, 0xb9, 0x66, 0xcf, 0x74, 0xb9, 0xc5, 0xa4, 0x9b, 0x44, 0xc0, 0x03,
];
